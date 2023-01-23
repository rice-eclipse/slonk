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

#![warn(clippy::pedantic)]

use std::sync::PoisonError;

mod config;
mod console;
mod data;
mod execution;
pub mod hardware;
mod heartbeat;
mod incoming;
mod outgoing;
pub mod server;
pub mod state;

#[non_exhaustive]
#[derive(Debug)]
/// The full enumeration of all errors which can terminate the execution of the controller.
pub enum ControllerError {
    /// The controller failed because a lock was poisoned, likely due to a panicked thread.
    Poison,
    /// There was an I/O error when writing to a log file.
    Console(std::io::Error),
    /// There was an error while writing an outgoing message to the dashboard.
    Outgoing(outgoing::Error),
    /// There was an error while attempting to perform some GPIO action.
    Gpio(gpio_cdev::Error),
    /// Something went wrong with the hardware.
    Hardware(&'static str),
    /// The configuration was incorrectly formed.
    Configuration(config::Error),
    /// The user gave the wrong input arguments to the main executable.
    Args(&'static str),
    /// An error ocurred while working with a state guard.
    State(state::Error),
}

impl<T> From<PoisonError<T>> for ControllerError {
    fn from(_: PoisonError<T>) -> Self {
        ControllerError::Poison
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
    /// Convert an I/O error into a controller error.
    ///
    /// The only error which should cause the controller to hard-return is writing to a log, so this
    /// should be only called if the source of the I/O error was writing ot a log.
    fn from(err: std::io::Error) -> Self {
        ControllerError::Console(err)
    }
}

impl From<state::Error> for ControllerError {
    fn from(err: state::Error) -> Self {
        ControllerError::State(err)
    }
}

impl From<outgoing::Error> for ControllerError {
    fn from(err: outgoing::Error) -> Self {
        ControllerError::Outgoing(err)
    }
}
