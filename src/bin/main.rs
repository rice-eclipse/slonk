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

use slonk::{server::RaspberryPi, ControllerError};

/// The main function for the `slonk` controller.
///
/// # Arguments
///
/// The first argument to this executable (via `std::env::args`) is the path to a configuration JSON
/// file, formatted according to the specification in `api.md`.
///
/// The second argument to this executable is a path to a directory where log files should be
/// created.
/// If the directory does not exist, it will be created.
fn main() -> Result<(), ControllerError> {
    slonk::server::run::<RaspberryPi>()
}
