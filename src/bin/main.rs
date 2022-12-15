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
