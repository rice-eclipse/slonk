use std::{fs::File, io::BufReader};

use resfet_controller_2::{config::Configuration, ControllerError};

/// The main function for the RESFET controller.
///
/// # Arguments
///
/// The first argument to this executable (via `std::env::args`) is the path to
/// a configuration JSON file, formatted according to the specification in
/// `api.md`.
fn main() -> Result<(), ControllerError> {
    // Use arguments to get configuration file
    let json_path = std::env::args().nth(1).ok_or(ControllerError::Args)?;

    let config_file = File::open(json_path)?;
    let config = Configuration::parse(&mut BufReader::new(config_file))?;
    println!("RESFET v2 is currently not working, but we were at least able to successfully parse a configuration.");
    println!("Here's the configuration: {config:#?}");

    // successful termination!
    Ok(())
}
