//! Functions for command execution.

use crate::{
    config::{Action, Configuration},
    console::UserLog,
    hardware::GpioPin,
    incoming::Command,
    outgoing::{DashChannel, Message},
    ControllerError, ControllerState, StateGuard,
};
use std::{
    io::Write,
    sync::Mutex,
    thread::sleep,
    time::{Duration, SystemTime},
};

/// Execute a command and log the process of execution.
///
/// # Inputs
///
/// * `cmd`: The command to be executed.
/// * `log_file`: Location where log information will be written.
/// * `configuration`: Configuration object for program execution.
/// * `driver_lines`: Output lines for the drivers.
///     Each index in `driver_lines` corresponds one-to-one with the drivers in `configuration`.  
/// * `state`: The controller for the current system state.
/// * `dashboard_stream`: The stream to use for writing messages to the dashboard.
///
/// # Errors
///
/// This function will return an error if a lock is poisoned or if we are unable to actuate GPIO.
///
/// # Panics
///
/// This function will panic if the current system time is before the UNIX epoch.
pub fn handle_command(
    cmd: &Command,
    log_file: &mut impl Write,
    user_log: &UserLog<impl Write>,
    configuration: &Configuration,
    driver_lines: &Mutex<Vec<impl GpioPin>>,
    state: &StateGuard,
    dashboard_stream: &Mutex<DashChannel<impl Write, impl Write>>,
) -> Result<(), ControllerError> {
    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();

    #[allow(unused_must_use)]
    {
        user_log.info(&format!("Executing command {cmd:?}"));
    }

    #[allow(unused_must_use)]
    if let Err(e) = writeln!(log_file, "{},request,{}", time.as_nanos(), cmd) {
        user_log.warn(&format!("Unable to log command {cmd} to log file: {e:?}"));
    }

    match cmd {
        Command::Ready => {
            // write "ready" to the dashboard
            dashboard_stream
                .lock()?
                .send(&Message::Ready)
                .map_err(|_| ControllerError::Io)?;
        }
        Command::Actuate { driver_id, value } => {
            actuate_driver(driver_lines.lock()?.as_mut(), *driver_id, *value)?;
        }
        Command::Ignition => ignition(configuration, driver_lines.lock()?.as_mut(), state)?,
        Command::EmergencyStop => {
            emergency_stop(configuration, driver_lines.lock()?.as_mut(), state)?;
        }
    };

    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();

    #[allow(unused_must_use)]
    if let Err(e) = writeln!(log_file, "{},finish,{}", time.as_nanos(), cmd) {
        user_log.warn(&format!(
            "Unable to log completion of command {cmd} to log file: {e:?}"
        ));
    }
    Ok(())
}

/// Attempt to perform an emergency stop procedure.
///
/// # Errors
///
/// This function can return an `Err` in the following cases:
///
/// * The user attempted to perform an ignition from a state which was not standby.
/// * A lock was poisoned.
/// * We failed to gain control over GPIO.
pub fn emergency_stop(
    configuration: &Configuration,
    driver_lines: &mut [impl GpioPin],
    state: &StateGuard,
) -> Result<(), ControllerError> {
    // transition to EStop, and if it's already in EStopping, don't interfere
    state.move_to(ControllerState::EStopping)?;

    perform_actions(driver_lines, &configuration.estop_sequence)?;

    // done doing the estop sequence, move back to standby
    state.move_to(ControllerState::Standby)?;

    Ok(())
}

/// Attempt to perform an ignition procedure.
///
/// # Errors
///
/// This function can return an `Err` in the following cases:
///
/// * The user attempted to perform an ignition from a state which was not standby.
/// * A lock was poisoned.
/// * We failed to gain control over GPIO.
fn ignition(
    configuration: &Configuration,
    driver_lines: &mut [impl GpioPin],
    state: &StateGuard,
) -> Result<(), ControllerError> {
    state.move_to(ControllerState::PreIgnite)?;
    sleep(Duration::from_millis(u64::from(
        configuration.pre_ignite_time,
    )));

    state.move_to(ControllerState::Ignite)?;
    perform_actions(driver_lines, &configuration.ignition_sequence)?;

    state.move_to(ControllerState::PostIgnite)?;
    sleep(Duration::from_millis(u64::from(
        configuration.post_ignite_time,
    )));

    // done doing the ignition sequence, move back to standby
    state.move_to(ControllerState::Standby)?;

    Ok(())
}

/// Actuate a given driver to a given value using GPIO cdev to interface with OS.
///
/// # Inputs
///
/// * `driver_lines`: A table of GPIO lines for all drivers.
/// * `driver_id`: The ID of the driver to be actuated.
///     An ID is an index into `configuration.drivers` for the associated driver.
///     It is also the same index into `driver_lines`.
/// * `value`: The logic level that the driver should be actuated to.
///     `value` should be `true` to get a high value on the GPIO pin, and `false` for a low value.
///
/// # Errors
///
/// This function may return an error if we are unable to gain control over the GPIO pin associated
/// with the driver.
fn actuate_driver(
    driver_lines: &mut [impl GpioPin],
    driver_id: u8,
    value: bool,
) -> Result<(), ControllerError> {
    driver_lines[driver_id as usize]
        .write(value)
        .map_err(std::convert::Into::into)
}

/// Perform a sequence of actions, such as for emergency stopping or for
/// ignition.
///
/// # Errors
///
/// This function will return an error if we are unable to write to GPIO.
fn perform_actions(
    driver_lines: &mut [impl GpioPin],
    actions: &[Action],
) -> Result<(), ControllerError> {
    for action in actions {
        match action {
            Action::Actuate { driver_id, value } => {
                driver_lines[*driver_id as usize].write(*value)?;
            }
            Action::Sleep { duration } => sleep(*duration),
        };
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, thread::scope};

    use crate::hardware::ListenerPin;

    use super::*;

    #[test]
    /// Test that state transitions are performed correctly during ignition.
    fn ignition_state_transitions() {
        let config = r#"{
            "frequency_status": 1,
            "log_buffer_size": 1,
            "sensor_groups": [],
            "pre_ignite_time": 500,
            "post_ignite_time": 500,
            "drivers": [],
            "ignition_sequence": [
                {
                    "type": "Sleep",
                    "duration": {
                        "secs": 0,
                        "nanos": 500000000
                    }
                }
            ],
            "estop_sequence": [],
            "spi_mosi": 0,
            "spi_miso": 0,
            "spi_clk": 0,
            "spi_frequency_clk": 50000,
            "adc_cs": []
        }"#;

        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();

        let mut driver_lines: [ListenerPin; 0] = [];

        let state = StateGuard::new(ControllerState::Standby);
        let state_ref = &state;

        scope(|s| {
            s.spawn(move || ignition(&config, &mut driver_lines, state_ref).unwrap());

            sleep(Duration::from_millis(250));
            assert_eq!(state.status().unwrap(), ControllerState::PreIgnite);

            sleep(Duration::from_millis(500));
            assert_eq!(state.status().unwrap(), ControllerState::Ignite);

            sleep(Duration::from_millis(500));
            assert_eq!(state.status().unwrap(), ControllerState::PostIgnite);

            sleep(Duration::from_millis(500));
            assert_eq!(state.status().unwrap(), ControllerState::Standby);
        });
    }

    #[test]
    /// Test that valve actuations are performed correctly during ignition.
    fn ignition_actuation() {
        let config = r#"{
            "frequency_status": 1,
            "log_buffer_size": 1,
            "sensor_groups": [],
            "pre_ignite_time": 0,
            "post_ignite_time": 0,
            "drivers": [],
            "ignition_sequence": [
                {
                    "type": "Actuate",
                    "driver_id": 0,
                    "value": true
                },
                {
                    "type": "Actuate",
                    "driver_id": 0,
                    "value": false
                }
            ],
            "estop_sequence": [],
            "spi_mosi": 0,
            "spi_miso": 0,
            "spi_clk": 0,
            "spi_frequency_clk": 50000,
            "adc_cs": []
        }"#;

        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();
        let mut driver_lines = [ListenerPin::new(false)];
        let state = StateGuard::new(ControllerState::Standby);

        ignition(&config, &mut driver_lines, &state).unwrap();

        assert_eq!(driver_lines[0].history().as_slice(), [false, true, false]);
    }

    #[test]
    /// Test that the correct sequence of state transistions are performed during an emergency stop.
    fn estop_state_transitions() {
        let config = r#"{
            "frequency_status": 1,
            "log_buffer_size": 1,
            "sensor_groups": [],
            "pre_ignite_time": 500,
            "post_ignite_time": 500,
            "drivers": [],
            "ignition_sequence": [],
            "estop_sequence": [
                {
                    "type": "Sleep",
                    "duration": {
                        "secs": 0,
                        "nanos": 500000000
                    }
                }
            ],
            "spi_mosi": 0,
            "spi_miso": 0,
            "spi_clk": 0,
            "spi_frequency_clk": 50000,
            "adc_cs": []
        }"#;

        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();

        let mut driver_lines: [ListenerPin; 0] = [];

        let state = StateGuard::new(ControllerState::Standby);
        let state_ref = &state;

        scope(|s| {
            s.spawn(move || emergency_stop(&config, &mut driver_lines, state_ref).unwrap());

            sleep(Duration::from_millis(250));
            assert_eq!(state.status().unwrap(), ControllerState::EStopping);

            sleep(Duration::from_millis(500));
            assert_eq!(state.status().unwrap(), ControllerState::Standby);
        });
    }

    #[test]
    /// Test that driver actuations are performed correctly during emergency stop.
    fn estop_actuation() {
        let config = r#"{
            "frequency_status": 1,
            "log_buffer_size": 1,
            "sensor_groups": [],
            "pre_ignite_time": 0,
            "post_ignite_time": 0,
            "drivers": [],
            "ignition_sequence": [],
            "estop_sequence": [
                {
                    "type": "Actuate",
                    "driver_id": 0,
                    "value": true
                },
                {
                    "type": "Actuate",
                    "driver_id": 0,
                    "value": false
                }
            ],
            "spi_mosi": 0,
            "spi_miso": 0,
            "spi_clk": 0,
            "spi_frequency_clk": 50000,
            "adc_cs": []
        }"#;

        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();
        let mut driver_lines = [ListenerPin::new(false)];
        let state = StateGuard::new(ControllerState::Standby);

        emergency_stop(&config, &mut driver_lines, &state).unwrap();

        assert_eq!(driver_lines[0].history().as_slice(), [false, true, false]);
    }
}
