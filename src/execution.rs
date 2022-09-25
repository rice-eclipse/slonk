//! Functions for command execution.
use gpio_cdev::{Chip, Line, LineRequestFlags};

use crate::{
    config::{Action, Configuration},
    incoming::Command,
    outgoing::Message,
    ControllerError, ControllerState, StateGuard,
};
use std::{io::Write, sync::Mutex, time::SystemTime};

/// Executing and logging sent commands, writing request time and execution time
///
/// #Inputs
///
/// * `cmd`: The command to be executed.
/// * `log_file`: Location where log information will be written.
/// *  `configuration` : Configuration object for program execution
/// *  `driver_lines` : System GPIO output lines
/// *  `state` : requested actuation state for pin
/// *  `dashboard_stream` : Messages to be sent to the dashboard
///
/// #Errors
///
/// *  This function may return an 'Err' if at any point execution of comnand is
/// unsuccessful
///
pub fn handle_command(
    cmd: &Command,
    log_file: &mut impl Write,
    configuration: &Configuration,
    driver_lines: &[Line],
    state: &StateGuard,
    dashboard_stream: &Mutex<Option<impl Write>>,
) -> Result<(), ControllerError> {
    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    // Second column for execution status of command sent
    writeln!(log_file, "{},{},{}", time.as_nanos(), "request", cmd)?;
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
        Command::Ignition => ignition(configuration, state)?,
        Command::EmergencyStop => emergency_stop(configuration, state)?,
    };

    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    writeln!(log_file, "{},{},{}", time.as_nanos(), "finish", cmd)?;
    log_file.flush()?;

    Ok(())
}

/// Attempt to perform an emergency stop procedure.
/// If an emergency stop procedure is already running, this function will not
/// interrupt that procedure already in progress.
///
/// # Errors
///
/// This function may return an `Err` due to thread poisoning or failed GPIO.
pub fn emergency_stop(
    configuration: &Configuration,
    state: &StateGuard,
) -> Result<(), ControllerError> {
    // transition to EStop, and if it's already in EStopping, don't interfere
    let transition_result = state.move_to(ControllerState::EStopping);
    match transition_result {
        Ok(_) => (),
        Err(ControllerError::IllegalTransition { from: _, to: _ }) => return Ok(()),
        err => return err,
    };

    perform_actions(&configuration.estop_sequence)?;

    // done doing the estop sequence, move back to standby
    state.move_to(ControllerState::Standby)?;

    Ok(())
}

fn ignition(configuration: &Configuration, state: &StateGuard) -> Result<(), ControllerError> {
    todo!()
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
    driver_lines: &[Line],
    driver_id: u8,
    state: bool,
) -> Result<(), ControllerError> {
    driver_lines[driver_id as usize]
        .request(LineRequestFlags::OUTPUT, 0, "resfet-cmd-handler")?
        .set_value(if state { 1 } else { 0 })?;

    Ok(())
}

/// Perform a sequence of actions, such as for emergency stopping or for
/// ignition.
///
/// # Errors
fn perform_actions(actions: &[Action]) -> Result<(), ControllerError> {
    todo!();
}
