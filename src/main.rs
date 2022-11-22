use std::{
    fs::{create_dir_all, File},
    io::{BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::Mutex,
    time::Duration,
};

use gpio_cdev::Chip;
use resfet_controller_2::{
    config::Configuration,
    console::UserLog,
    data::{driver_status_listen, sensor_listen},
    execution::handle_command,
    hardware::{
        spi::{Bus, Device},
        GpioPin, Mcp3208,
    },
    outgoing::DashChannel,
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Use arguments to get configuration file
    let json_path = args.get(0).ok_or(ControllerError::Args)?;
    let logs_path = args.get(1).ok_or(ControllerError::Args)?;

    create_dir_all(logs_path)?;
    let user_log = UserLog::new(File::create(PathBuf::from_iter([
        logs_path,
        "console.txt",
    ]))?);
    let user_log_ref = &user_log;

    user_log.debug("Parsing configuration file...")?;
    let config_file = File::open(json_path)?;
    let config = Configuration::parse(&mut BufReader::new(config_file))?;
    let config_ref = &config;
    user_log.debug("Successfully parsed configuration file")?;

    user_log.debug("Creating configuration files")?;

    let mut sensor_log_files: Vec<Vec<File>> = Vec::new();
    for sensor_group in &config.sensor_groups {
        let mut group_files = Vec::new();
        let sensor_group_path = PathBuf::from_iter([logs_path, &sensor_group.label]);
        // create subfolder for this sensor group
        create_dir_all(&sensor_group_path)?;
        user_log.info(&format!(
            "Created directory {:}",
            sensor_group_path.display()
        ))?;

        for sensor in &sensor_group.sensors {
            // create file for this specific sensor
            let mut sensor_file_path = sensor_group_path.clone();
            sensor_file_path.push(&format!("{}.csv", sensor.label));
            group_files.push(File::create(&sensor_file_path)?);

            user_log.info(&format!("Created log file {:}", sensor_file_path.display()))?;
        }

        sensor_log_files.push(group_files);
    }

    // create log file for commands that have been executed
    let mut cmd_file = File::create(PathBuf::from_iter([logs_path, "commands.csv"]))?;
    let cmd_file_ref = &mut cmd_file;

    let mut drivers_file = File::create(PathBuf::from_iter([logs_path, "drivers.csv"]))?;
    let drivers_file_ref = &mut drivers_file;

    user_log.debug("Successfully created log files")?;
    user_log.debug("Now spawning sensor listener threads...")?;

    let state = StateGuard::new(ControllerState::Standby);
    let state_ref = &state;

    // when a client connects, the inner value of this mutex will be `Some`
    // containing a TCP stream to the dashboard
    let to_dash = Mutex::new(DashChannel::new(File::create(PathBuf::from_iter([
        logs_path, "sent.csv",
    ]))?));
    let to_dash_ref = &to_dash;

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

    let mut driver_lines = Vec::new();
    for driver in &config.drivers {
        driver_lines.push(gpio_chip.get_line(u32::from(driver.pin))?);
    }
    let driver_lines_ref = &driver_lines;

    std::thread::scope(|s| {
        for (group_id, mut log_file_group) in sensor_log_files.into_iter().enumerate() {
            s.spawn(move || {
                sensor_listen(
                    s,
                    group_id as u8,
                    config_ref,
                    driver_lines_ref,
                    &mut log_file_group,
                    user_log_ref,
                    adcs_ref,
                    state_ref,
                    to_dash_ref,
                )
            });
        }

        s.spawn(move || {
            driver_status_listen(
                config_ref,
                driver_lines_ref,
                drivers_file_ref,
                user_log_ref,
                state_ref,
                to_dash_ref,
            )
        });

        user_log.debug("Successfully spawned sensor listener threads.")?;
        user_log.debug("Opening network...")?;

        // TODO: maybe configure this IP number?
        let address = "127.0.0.1:1234";
        let listener = TcpListener::bind(address)?;

        user_log.info(&format!("Opened TCP listener on address {address}"))?;
        user_log.debug("Handling clients...")?;

        for mut from_dash in listener.incoming().flatten() {
            to_dash
                .lock()?
                .set_channel(TcpStream::connect(from_dash.peer_addr()?)?);
            handle_client(
                &mut from_dash,
                to_dash_ref,
                config_ref,
                driver_lines_ref,
                cmd_file_ref,
                user_log_ref,
                state_ref,
            )?;
        }

        Ok::<(), ControllerError>(())
    })?;
    // successful termination!
    Ok(())
}

/// Handle a single dashboard client.
fn handle_client(
    from_dash: &mut impl Read,
    to_dash: &Mutex<DashChannel<impl Write, impl Write>>,
    config: &Configuration,
    driver_lines: &[impl GpioPin],
    cmd_log_file: &mut impl Write,
    user_log: &UserLog<impl Write>,
    state_ref: &StateGuard,
) -> Result<(), ControllerError> {
    loop {
        let cmd_deser_result = serde_json::from_reader(&mut *from_dash);

        let Ok(cmd) = cmd_deser_result else {
            #[allow(unused_must_use)] {
                // don't kill the process even if we get something bad
                user_log.critical(&format!(
                    "encountered error while parsing message. future messages will likely not be parsed correctly: {cmd_deser_result:?}"
                ));
            }
            continue;
        };

        // TODO figure out if we should do something other than returning on
        // error case here.
        handle_command(&cmd, cmd_log_file, config, driver_lines, state_ref, to_dash)?;
    }
}
