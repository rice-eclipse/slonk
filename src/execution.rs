//! Functions for command execution.

use crate::{config::Configuration, ControllerState, StateGuard};

/// Perform an emergency stop procedure.
///
pub fn emergency_stop(
    _configuration: &Configuration,
    _state: &StateGuard,
) -> Result<(), ControllerState> {
    todo!();
}
