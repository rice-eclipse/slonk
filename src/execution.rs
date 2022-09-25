//! Functions for command execution.

use crate::{
    config::{Action, Configuration},
    hardware::GpioPin,
    incoming::Command,
    outgoing::Message,
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
///     Each index in `driver_lines` corresponds one-to-one with the drivers in
///     `configuration`.  
/// * `state`: The controller for the current system state.
/// * `dashboard_stream`: The stream to use for writing messages to the
///     dashboard.
///
/// # Errors
///
/// TODO: fully examine all callees to describe possible errors.
///
/// # Panics
///
/// This function will panic if the current system time is before the UNIX
/// epoch.
pub fn handle_command(
    cmd: &Command,
    log_file: &mut impl Write,
    configuration: &Configuration,
    driver_lines: &[impl GpioPin],
    state: &StateGuard,
    dashboard_stream: &Mutex<Option<impl Write>>,
) -> Result<(), ControllerError> {
    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    // Second column for execution status of command sent
    writeln!(log_file, "{},request,{}", time.as_nanos(), cmd)?;
    log_file.flush()?;

    match cmd {
        Command::Ready => {
            // write "ready" to the dashboard
            let mut dashboard_guard = dashboard_stream.lock()?;
            if let Some(writer) = dashboard_guard.as_mut() {
                serde_json::to_writer(writer, &Message::Ready).map_err(|_| ControllerError::Io)?;
            }
        }
        Command::Actuate { driver_id, state } => actuate_driver(driver_lines, *driver_id, *state)?,
        Command::Ignition => ignition(configuration, driver_lines, state)?,
        Command::EmergencyStop => emergency_stop(configuration, driver_lines, state)?,
    };

    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    writeln!(log_file, "{},finish,{}", time.as_nanos(), cmd)?;
    log_file.flush()?;

    Ok(())
}

/// Attempt to perform an emergency stop procedure.
///
/// # Errors
///
/// This function can return an `Err` in the following cases:
///
/// * The user attempted to perform an ignition from a state which was not
///     standby.
/// * A lock was poisoned.
/// * We failed to gain control over GPIO.
pub fn emergency_stop(
    configuration: &Configuration,
    driver_lines: &[impl GpioPin],
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
/// * The user attempted to perform an ignition from a state which was not
///     standby.
/// * A lock was poisoned.
/// * We failed to gain control over GPIO.
fn ignition(
    configuration: &Configuration,
    driver_lines: &[impl GpioPin],
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

/// Actuate a given driver to a given state using GPIO cdev to interface with OS.
///
/// # Inputs
///
/// * `driver_lines`: A table of GPIO lines for all drivers.
/// * `driver_id`: The ID of the driver to be actuated.
///     An ID is an index into `configuration.drivers` for the associated driver.
///     It is also the same index into `driver_lines`.
/// * `state`: The state that the driver should be actuated to.
///     `state` should be `true` to get a high value on the GPIO pin, and `false` for a low value.
///
/// # Errors
///
/// This function may return an error if we are unable to gain control over the GPIO pin associated with the driver.
fn actuate_driver(
    driver_lines: &[impl GpioPin],
    driver_id: u8,
    state: bool,
) -> Result<(), ControllerError> {
    driver_lines[driver_id as usize].write("resfet-cmd-handler", state)?;

    Ok(())
}

/// Perform a sequence of actions, such as for emergency stopping or for
/// ignition.
///
/// # Errors
fn perform_actions(
    driver_lines: &[impl GpioPin],
    actions: &[Action],
) -> Result<(), ControllerError> {
    for action in actions {
        match action {
            Action::Actuate { driver_id, state } => {
                driver_lines[*driver_id as usize].write("resfet-action-seq", *state)?;
            }
            Action::Sleep { duration } => sleep(*duration),
        };
    }

    Ok(())
}
