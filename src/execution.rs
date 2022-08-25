//! Functions for command execution.

use crate::{
    config::{Action, Configuration},
    ControllerError, ControllerState, StateGuard,
};

/// Attempt to perform an emergency stop procedure.
/// If an emergency stop procedure is already running, this function will not
/// interrupt that procedure already in progress.
///
/// # Errors
///
/// This function may return an `Error` due to thread poisoning or failed GPIO.
pub fn emergency_stop(
    configuration: &Configuration,
    state: &StateGuard,
) -> Result<(), ControllerError> {
    // transition to EStop, and if it's already in EStopping, don't interfere
    let transition_result = state.move_to(ControllerState::EStopping);
    match transition_result {
        Ok(_) | Err(ControllerError::IllegalTransition { from: _, to: _ }) => (),
        err => return err,
    };

    perform_actions(&configuration.estop_sequence)?;

    Ok(())
}

/// Perform a sequence of actions, such as for emergency stopping or for
/// ignition.
///
/// # Errors
fn perform_actions(actions: &[Action]) -> Result<(), ControllerError> {
    todo!();
}
