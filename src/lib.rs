#![warn(clippy::pedantic)]

use std::sync::{PoisonError, RwLock};

pub mod config;
pub mod console;
pub mod data;
pub mod execution;
pub mod hardware;
pub mod incoming;
pub mod outgoing;
pub mod server;

/// A guard for controller state which can be used to notify other threads of changes to controller
/// state.
pub struct StateGuard {
    /// The current state.
    state: RwLock<ControllerState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The set of all states the engine controller can be in.
pub enum ControllerState {
    /// The engine is in standby - passively logging and awating commands.
    /// This state can only be reached from the `PostIgnite` and `EStopping` states.
    Standby,
    /// The engine is preparing to ignite in the upcoming seconds.
    /// This state can only be reached from the `Standby` state.
    /// During this phase, data logging threads should increase their logging speed.
    PreIgnite,
    /// The engine is currently ignited.
    /// This state can only be reached from the `Standby` state.
    /// Data logging should be fast here.
    Ignite,
    /// The engine is not currently ignited, but was igniting recently.
    /// This state can only be reached from the `Ignite` state.
    /// Since interesting things might still happen in the post-ignition phase, data logging should
    /// still be fast here.
    PostIgnite,
    /// An emergency stop command has just been called.
    /// This state is reachable from any other state except the `Quit` state.
    /// Data logging should be fast, since anything that is worth e-stopping over is probably very
    /// interesting.
    EStopping,
    /// The engine controller is shutting down.
    /// This state can only be reached from the `Standby` state.
    /// During this state, each thread will "wrap up" its work and then exit as soon as possible.
    Quit,
}

#[non_exhaustive]
#[derive(Debug)]
/// The full enumeration of all errors which can occur during the execution of the controller.
pub enum ControllerError {
    /// The controller failed because a lock was poisoned, likely due to a panicked thread.
    Poison,
    /// The library `serde_json` failed to deserialize a structure because it was malformed.
    MalformedDeserialize(serde_json::Error),
    /// There was an I/O error when writing to or reading from the network or a file.
    Io(std::io::Error),
    /// There was an error with serialization or deserialization.
    Serde(serde_json::Error),
    /// There was an error while attempting to perform some GPIO action.
    Gpio(gpio_cdev::Error),
    /// Something went wrong with the hardware.
    Hardware(&'static str),
    /// The configuration was incorrectly formed.
    Configuration(config::Error),
    /// The user gave the wrong input arguments to the main executable.
    Args(String),
    /// An illegal state transition was attempted.
    /// This is often a sign of a critical internal logic error.
    IllegalTransition {
        /// The state that was transitioned from.
        from: ControllerState,
        /// The state that the transition was attempted to be made to.
        to: ControllerState,
    },
}

impl StateGuard {
    #[must_use]
    /// Construct a new `StateGuard`.
    /// Initializes its state to the value of `state`.
    pub fn new(state: ControllerState) -> StateGuard {
        StateGuard {
            state: RwLock::new(state),
        }
    }

    /// Get the status of this guard.
    /// This operation is blocking.
    ///
    /// # Errors
    ///
    /// Will return `Err(Controller::Poison)` in the case that the internal lock of this guard is
    /// poisoned.
    pub fn status(&self) -> Result<ControllerState, ControllerError> {
        Ok(*self.state.read()?)
    }

    /// Move this guard into a new state.
    ///
    /// # Errors
    ///
    /// This function will return an `Err(ControllerError::Poison)` in the case that an internal
    /// lock is poisoned.
    /// If `new_state` is not reachable from the current state, an
    /// `Err(ControllerError::IllegalTransition)` will be returned.
    pub fn move_to(&self, new_state: ControllerState) -> Result<(), ControllerError> {
        let mut write_guard = self.state.write()?;
        let old_state = *write_guard;

        // determine whether the transition is valid
        let valid_transition = match new_state {
            ControllerState::Standby => {
                old_state == ControllerState::EStopping || old_state == ControllerState::PostIgnite
            }
            ControllerState::PreIgnite | ControllerState::Quit => {
                old_state == ControllerState::Standby
            }
            ControllerState::Ignite => old_state == ControllerState::PreIgnite,
            ControllerState::PostIgnite => old_state == ControllerState::Ignite,
            ControllerState::EStopping => old_state != ControllerState::Quit,
        };

        if !valid_transition {
            return Err(ControllerError::IllegalTransition {
                from: old_state,
                to: new_state,
            });
        }

        *write_guard = new_state;
        Ok(())
    }
}

/// The set of errors that can be caused from working with a `StateGuard`.
pub enum GuardError {
    /// The guard's lock was poisoned. This implies a panicked thread owned a lock.
    Poison,
    /// An illegal transition was attempted.
    IllegalTransition,
}

impl<T> From<PoisonError<T>> for GuardError {
    fn from(_: PoisonError<T>) -> Self {
        GuardError::Poison
    }
}

impl<T> From<PoisonError<T>> for ControllerError {
    fn from(_: PoisonError<T>) -> Self {
        ControllerError::Poison
    }
}

impl From<serde_json::Error> for ControllerError {
    fn from(err: serde_json::Error) -> Self {
        ControllerError::Serde(err)
    }
}

impl From<gpio_cdev::Error> for ControllerError {
    fn from(err: gpio_cdev::Error) -> Self {
        ControllerError::Gpio(err)
    }
}

impl From<config::Error> for ControllerError {
    fn from(err: config::Error) -> Self {
        ControllerError::Configuration(err)
    }
}

impl From<std::io::Error> for ControllerError {
    fn from(err: std::io::Error) -> Self {
        ControllerError::Io(err)
    }
}
