use std::{
    fs::{create_dir_all, File},
    io::BufReader,
    path::PathBuf,
    sync::Mutex,
    time::Duration,
};

use gpio_cdev::Chip;
use resfet_controller_2::{
    config::Configuration,
    hardware::{
        spi::{Bus, Device},
        Mcp3208,
    },
    thread::sensor_listen,
    ControllerError, ControllerState, StateGuard,
};

/// The main function for the RESFET controller.
///
/// # Arguments
///
/// The first argument to this executable (via `std::env::args`) is the path to
/// a configuration JSON file, formatted according to the specification in
/// `api.md`.
///
/// The second argument to this executable is a path to a directory where log
/// files should be created.
/// If the directory does not exist, it will be created.
fn main() -> Result<(), ControllerError> {
    println!("=== RESFET 2 by Rice Eclipse ===");
    println!("Parsing configuration file...");
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Use arguments to get configuration file
    let json_path = args.get(0).ok_or(ControllerError::Args)?;

    let config_file = File::open(json_path)?;
    let config = Configuration::parse(&mut BufReader::new(config_file))?;
    let config_ref = &config;
    println!("Successfully parsed configuration file.");
    println!("Creating log files...");

    let logs_path = args.get(1).ok_or(ControllerError::Args)?;

    // create path to log files directory
    create_dir_all(&logs_path)?;
    println!("Created directory {logs_path}");

    let mut sensor_log_files: Vec<Vec<File>> = Vec::new();
    for sensor_group in &config.sensor_groups {
        let mut group_files = Vec::new();
        let sensor_group_path = PathBuf::from_iter(&[logs_path, &sensor_group.label]);
        // create subfolder for this sensor group
        create_dir_all(&sensor_group_path)?;
        println!("Created directory {:?}", sensor_group_path.as_os_str());

        for sensor in &sensor_group.sensors {
            // create file for this specific sensor
            let mut sensor_file_path = sensor_group_path.clone();
            sensor_file_path.push(&format!("{}.csv", sensor.label));
            group_files.push(File::create(&sensor_file_path)?);
            println!("Created log file {:?}", sensor_file_path.as_os_str());
        }

        sensor_log_files.push(group_files);
    }

    println!("Successfully created log files");
    println!("Now spawning sensor listener threads...");

    let state = StateGuard::new(ControllerState::Standby);
    let state_ref = &state;
    let dashboard_stream: Mutex<Option<File>> = Mutex::new(None);
    let dashboard_stream_ref = &dashboard_stream;
    let mut gpio_chip = Chip::new("/dev/gpiochip0")?;
    let bus = Bus {
        period: Duration::from_secs(1) / config.spi_frequency_clk,
        pin_clk: gpio_chip.get_line(config.spi_clk as u32)?,
        pin_mosi: gpio_chip.get_line(config.spi_mosi as u32)?,
        pin_miso: gpio_chip.get_line(config.spi_miso as u32)?,
    };
    let mut adcs = Vec::new();
    for &cs_pin in &config.adc_cs {
        adcs.push(Mcp3208::new(Device::new(&bus, &mut gpio_chip, cs_pin)?));
    }
    let adcs_ref = &adcs;

    std::thread::scope(|s| {
        for (group_id, mut log_file_group) in sensor_log_files.into_iter().enumerate() {
            s.spawn(move || {
                sensor_listen(
                    s,
                    group_id as u8,
                    config_ref,
                    &mut log_file_group,
                    adcs_ref,
                    state_ref,
                    dashboard_stream_ref,
                )
            });
        }

        println!("Successfully spawned sensor listener threads.");
    });
    // successful termination!
    Ok(())
}
