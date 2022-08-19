#![warn(clippy::pedantic)]

use std::sync::PoisonError;

pub mod config;
pub mod execution;
pub mod hardware;
pub mod incoming;
pub mod outgoing;
pub mod thread;

/// The set of all states the engine controller can be in.
pub enum ControllerState {
    /// The engine is in standby - passively logging and awating commands.
    /// This state can only be reached from the `PostIgnite` and `EStopping`
    /// states.
    Standby,
    /// The engine is preparing to ignite in the upcoming seconds.
    /// This state can only be reached from the `Standby` state.
    /// During this phase, data logging threads should increase their logging
    /// speed.
    PreIgnite,
    /// The engine is currently ignited.
    /// This state can only be reached from the `Standby` state.
    /// Data logging should be fast here.
    Ignite,
    /// The engine is not currently ignited, but was igniting recently.
    /// This state can only be reached from the `Ignite` state.
    /// Since interesting things might still happen in the post-ignition phase,
    /// data logging should still be fast here.
    PostIgnite,
    /// An emergency stop command has just been called.
    /// This state is reachable from any other state.
    /// Data logging should be fast, since anything that is worth e-stopping
    /// over is probably very interesting.
    EStopping,
}

#[non_exhaustive]
#[derive(Debug)]
/// The full enumeration of all errors which can occur during the execution of
/// the controller.
pub enum ControllerError {
    /// The controller failed because a lock was poisoned, likely due to a
    /// panicked thread.
    Poison,
    /// The library `serde_json` failed to deserialize a structure because it
    /// was malformed.
    MalformedDeserialize(serde_json::Error),
    /// There was an I/O error when writing to or reading from the network or a
    /// file.
    Io,
    /// There was an error while attempting to perform some GPIO action.
    Gpio(gpio_cdev::Error),
    /// The configuration was incorrectly formed.
    Configuration(config::Error),
    /// The user gave the wrong input arguments to the main executable.
    Args,
}

impl<T> From<PoisonError<T>> for ControllerError {
    fn from(_: PoisonError<T>) -> Self {
        ControllerError::Poison
    }
}

impl From<serde_json::Error> for ControllerError {
    fn from(err: serde_json::Error) -> Self {
        match err.classify() {
            serde_json::error::Category::Io => ControllerError::Io,
            _ => ControllerError::MalformedDeserialize(err),
        }
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
    fn from(_: std::io::Error) -> Self {
        ControllerError::Io
    }
}
