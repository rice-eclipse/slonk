//! Functions for command execution.

use std::sync::RwLock;

use crate::{config::Configuration, ControllerState};

/// Perform an emergency stop procedure.
///
pub fn emergency_stop(
    _configuration: &Configuration,
    _state: &RwLock<ControllerState>,
) -> Result<(), ControllerState> {
    todo!();
}
