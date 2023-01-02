/*
  slonk, a rocket engine controller.
  Copyright (C) 2022 Rice Eclipse.

  slonk is free software: you can redistribute it and/or modify
  it under the terms of the GNU General Public License as published by
  the Free Software Foundation, either version 3 of the License, or
  (at your option) any later version.

  slonk is distributed in the hope that it will be useful,
  but WITHOUT ANY WARRANTY; without even the implied warranty of
  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  GNU General Public License for more details.

  You should have received a copy of the GNU General Public License
  along with this program.  If not, see <http://www.gnu.org/licenses/>.
*/

//! Functions for command execution.

use crate::{
    config::{Action, Configuration},
    console::UserLog,
    hardware::GpioPin,
    incoming::Command,
    state::{self, Guard, State},
};
use std::{
    io::Write,
    sync::{Mutex, PoisonError},
    thread::sleep,
    time::{Duration, SystemTime},
};

#[derive(Debug)]
/// The set of possible errors that could be encountered while executing a command.
pub enum Error {
    /// A lock was poisoned.
    Poison,
    /// The command tried to actuate a driver that doesn't exist.
    DriverOutOfBounds,
    /// While executing a procedure, an illegal transition was attempted.
    State(state::Error),
}

impl<T> From<PoisonError<T>> for Error {
    fn from(_: PoisonError<T>) -> Self {
        Error::Poison
    }
}

impl From<state::Error> for Error {
    fn from(value: state::Error) -> Self {
        Error::State(value)
    }
}

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
    log_file: &Mutex<impl Write>,
    user_log: &UserLog<impl Write>,
    configuration: &Configuration,
    driver_lines: &Mutex<Vec<impl GpioPin>>,
    state: &Guard,
) -> Result<(), Error> {
    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();

    #[allow(unused_must_use)]
    {
        user_log.info(&format!("Executing command {cmd:?}"));
    }

    #[allow(unused_must_use)]
    if let Err(e) = writeln!(
        log_file.lock().map_err(|_| Error::Poison)?,
        "{},request,{cmd}",
        time.as_nanos()
    ) {
        user_log.warn(&format!("Unable to log command {cmd} to log file: {e:?}"));
    }

    match cmd {
        Command::Actuate { driver_id, value } => {
            if usize::from(*driver_id) > configuration.drivers.len() {
                // we were asked to actuate a non-existent driver
                return Err(Error::DriverOutOfBounds);
            }

            actuate_driver(
                driver_lines.lock().map_err(|_| Error::Poison)?.as_mut(),
                *driver_id,
                *value,
            )?;
        }
        Command::Ignition => ignition(configuration, driver_lines, state)?,
        Command::EmergencyStop => {
            emergency_stop(configuration, driver_lines, state)?;
        }
    };

    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();

    #[allow(unused_must_use)]
    if let Err(e) = writeln!(
        log_file.lock().map_err(|_| Error::Poison)?,
        "{},finish,{cmd}",
        time.as_nanos()
    ) {
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
    driver_lines: &Mutex<Vec<impl GpioPin>>,
    state: &Guard,
) -> Result<(), Error> {
    // transition to EStop, and if it's already in EStopping, don't interfere
    state.move_to(State::EStopping)?;

    perform_actions(driver_lines, &configuration.estop_sequence)?;

    // done doing the estop sequence, move back to standby
    state.move_to(State::Standby)?;

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
    driver_lines: &Mutex<Vec<impl GpioPin>>,
    state: &Guard,
) -> Result<(), Error> {
    state.move_to(State::PreIgnite)?;
    sleep(Duration::from_millis(u64::from(
        configuration.pre_ignite_time,
    )));

    state.move_to(State::Ignite)?;
    perform_actions(driver_lines, &configuration.ignition_sequence)?;

    state.move_to(State::PostIgnite)?;
    sleep(Duration::from_millis(u64::from(
        configuration.post_ignite_time,
    )));

    // done doing the ignition sequence, move back to standby
    state.move_to(State::Standby)?;

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
) -> Result<(), Error> {
    driver_lines[driver_id as usize]
        .write(value)
        .map_err(|_| Error::Poison)
}

/// Perform a sequence of actions, such as for emergency stopping or for
/// ignition.
///
/// # Errors
///
/// This function will return an error if we are unable to write to GPIO.
fn perform_actions(
    driver_lines: &Mutex<Vec<impl GpioPin>>,
    actions: &[Action],
) -> Result<(), Error> {
    for action in actions {
        match action {
            Action::Actuate { driver_id, value } => {
                driver_lines.lock().map_err(|_| Error::Poison)?[*driver_id as usize]
                    .write(*value)
                    .map_err(|_| Error::Poison)?;
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
            "spi_mosi": 11,
            "spi_miso": 13,
            "spi_clk": 14,
            "spi_frequency_clk": 50000,
            "adc_cs": []
        }"#;

        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();

        let driver_lines: Mutex<Vec<ListenerPin>> = Mutex::new(Vec::new());

        let state = Guard::new(State::Standby);
        let state_ref = &state;

        scope(|s| {
            s.spawn(move || ignition(&config, &driver_lines, state_ref).unwrap());

            sleep(Duration::from_millis(250));
            assert_eq!(state.status().unwrap(), State::PreIgnite);

            sleep(Duration::from_millis(500));
            assert_eq!(state.status().unwrap(), State::Ignite);

            sleep(Duration::from_millis(500));
            assert_eq!(state.status().unwrap(), State::PostIgnite);

            sleep(Duration::from_millis(500));
            assert_eq!(state.status().unwrap(), State::Standby);
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
            "drivers": [{
                "label": "OXI_FILL",
                "pin": 21,
                "protected": false
            }],
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
            "spi_mosi": 11,
            "spi_miso": 12,
            "spi_clk": 13,
            "spi_frequency_clk": 50000,
            "adc_cs": []
        }"#;

        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();
        let driver_lines = Mutex::new(vec![ListenerPin::new(false)]);
        let state = Guard::new(State::Standby);

        ignition(&config, &driver_lines, &state).unwrap();

        assert_eq!(
            driver_lines.lock().unwrap()[0].history().as_slice(),
            [false, true, false]
        );
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
            "spi_mosi": 11,
            "spi_miso": 12,
            "spi_clk": 13,
            "spi_frequency_clk": 50000,
            "adc_cs": []
        }"#;

        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();

        let driver_lines = Mutex::new(Vec::<ListenerPin>::new());

        let state = Guard::new(State::Standby);
        let state_ref = &state;

        scope(|s| {
            s.spawn(move || emergency_stop(&config, &driver_lines, state_ref).unwrap());

            sleep(Duration::from_millis(250));
            assert_eq!(state.status().unwrap(), State::EStopping);

            sleep(Duration::from_millis(500));
            assert_eq!(state.status().unwrap(), State::Standby);
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
            "spi_mosi": 11,
            "spi_miso": 12,
            "spi_clk": 13,
            "spi_frequency_clk": 50000,
            "adc_cs": []
        }"#;

        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();
        let driver_lines = Mutex::new(vec![ListenerPin::new(false)]);
        let state = Guard::new(State::Standby);

        emergency_stop(&config, &driver_lines, &state).unwrap();

        assert_eq!(
            driver_lines.lock().unwrap()[0].history().as_slice(),
            [false, true, false]
        );
    }
}
