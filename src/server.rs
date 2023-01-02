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

use std::{
    fs::{create_dir_all, File},
    io::{self, BufReader, Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Mutex,
    thread::Scope,
    time::Duration,
};

use gpio_cdev::{Chip, LineHandle, LineRequestFlags};

use crate::{
    config::Configuration,
    console::UserLog,
    data::{driver_status_listen, sensor_listen},
    execution::handle_command,
    hardware::{
        spi::{Bus, Device},
        Adc, GpioPin, ListenerPin, Mcp3208, ReturnsNumber,
    },
    incoming::{Command, ParseError},
    outgoing::{DashChannel, Message},
    ControllerError, ControllerState, StateGuard,
};

/// A trait for functions which can create the necessary hardware for the server to run.
///
/// This exists to allow us to "spoof" hardware for the main process so we don't have to test
/// everything on real hardware.
pub trait MakeHardware {
    /// The type of the chip, which can be used for getting a GPIO pin.
    type Chip;
    /// The type of GPIO pin that this trait can make.
    type Pin: GpioPin + Send + Sync;
    /// The internal bus type.
    type Bus;
    /// The type of ADC reader that this trait can make.
    type Reader<'a>: Adc + Send + Sync;

    /// Construct a GPIO chip which can be used to get pins.
    ///
    /// # Errors
    ///
    /// This function will return an error if constructing the chip fails.
    fn chip() -> Result<Self::Chip, ControllerError>;

    /// Construct a bus for use by the readers based on information from the configuration.
    ///
    /// # Errors
    ///
    /// This function wil lreturn an error if acquiring the pins for the bus fails.
    fn bus(config: &Configuration, chip: &mut Self::Chip) -> Result<Self::Bus, ControllerError>;

    #[allow(clippy::type_complexity)]
    /// Construct the ADCs using information from the configuration.
    ///
    /// The length of the vector of `Self::Reader` returned must be equal to the length of `adc_cs`
    /// in the configuration.
    ///
    /// # Errors
    ///
    /// This function may return an error if it is unable to acquire the GPIO needed.
    fn adcs<'a>(
        config: &Configuration,
        chip: &mut Self::Chip,
        bus: &'a Self::Bus,
    ) -> Result<Vec<Mutex<Self::Reader<'a>>>, ControllerError>;

    /// Construct the drivers using information from the configuration.
    ///
    /// # Errors
    ///
    /// This function may return an error if it is unable to acquire the GPIO needed.
    fn drivers(
        config: &Configuration,
        chip: &mut Self::Chip,
    ) -> Result<Vec<Self::Pin>, ControllerError>;
}

/// A hardware maker for actually interfacing with the Raspberry Pi.
pub struct RaspberryPi;

impl MakeHardware for RaspberryPi {
    type Chip = Chip;
    type Pin = LineHandle;

    type Bus = Mutex<Bus<Self::Pin>>;

    type Reader<'a> = Mcp3208<'a, Self::Pin>;

    fn chip() -> Result<Self::Chip, ControllerError> {
        Ok(Chip::new("/dev/gpiochip0")?)
    }

    fn adcs<'a>(
        config: &Configuration,
        chip: &mut Self::Chip,
        bus: &'a Self::Bus,
    ) -> Result<Vec<Mutex<Self::Reader<'a>>>, ControllerError> {
        let mut adcs = Vec::new();
        for &cs_pin in &config.adc_cs {
            adcs.push(Mutex::new(Mcp3208::new(Device::new(
                bus,
                chip.get_line(u32::from(cs_pin))?
                    .request(LineRequestFlags::OUTPUT, 1, "slonk")?,
            ))));
        }

        Ok(adcs)
    }

    fn drivers(
        config: &Configuration,
        chip: &mut Self::Chip,
    ) -> Result<Vec<Self::Pin>, ControllerError> {
        let mut lines = Vec::new();

        for driver in &config.drivers {
            lines.push(chip.get_line(u32::from(driver.pin))?.request(
                LineRequestFlags::OUTPUT,
                0,
                "slonk",
            )?);
        }

        Ok(lines)
    }

    fn bus(config: &Configuration, chip: &mut Self::Chip) -> Result<Self::Bus, ControllerError> {
        Ok(Mutex::new(Bus {
            period: Duration::from_secs(1) / config.spi_frequency_clk,
            pin_clk: chip.get_line(u32::from(config.spi_clk))?.request(
                LineRequestFlags::OUTPUT,
                0,
                "slonk",
            )?,
            pin_mosi: chip.get_line(u32::from(config.spi_mosi))?.request(
                LineRequestFlags::OUTPUT,
                0,
                "slonk",
            )?,
            pin_miso: chip.get_line(u32::from(config.spi_miso))?.request(
                LineRequestFlags::INPUT,
                0,
                "slonk",
            )?,
        }))
    }
}

/// A dummy hardware maker for testing on any Linux computer.
pub struct Dummy;

impl MakeHardware for Dummy {
    type Chip = ();
    type Pin = ListenerPin;

    type Reader<'a> = ReturnsNumber;

    type Bus = ();

    fn chip() -> Result<(), ControllerError> {
        Ok(())
    }

    fn bus(_: &Configuration, _: &mut Self::Chip) -> Result<Self::Bus, ControllerError> {
        Ok(())
    }

    #[allow(clippy::cast_possible_truncation)]
    fn adcs<'a>(
        config: &Configuration,
        _: &mut Self::Chip,
        _: &'a Self::Bus,
    ) -> Result<Vec<Mutex<Self::Reader<'a>>>, ControllerError> {
        Ok((0..config.adc_cs.len())
            .map(|i| Mutex::new(ReturnsNumber(i as u16)))
            .collect())
    }

    fn drivers(
        config: &Configuration,
        _: &mut Self::Chip,
    ) -> Result<Vec<Self::Pin>, ControllerError> {
        Ok((0..config.drivers.len())
            .map(|_| ListenerPin::new(false))
            .collect())
    }
}

#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
/// The primary run function for the `slonk` server.
///
/// `M` is a dependency-injector for creating hardware.
///
/// # Errors
///
/// This function can return any of the possible errors in `ControllerError`.
///
/// # Panics
///
/// This function may panic if it is unable to correctly set up the controller.
pub fn run<M: MakeHardware>() -> Result<(), ControllerError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Use arguments to get configuration file
    let json_path = args
        .get(0)
        .ok_or(ControllerError::Args("No configuration JSON path given"))?;
    let logs_path = args
        .get(1)
        .ok_or(ControllerError::Args("No logs path given"))?;

    create_dir_all(logs_path)?;
    let user_log = UserLog::new(file_create_new(PathBuf::from_iter([
        logs_path,
        "console.txt",
    ]))?);
    let user_log_ref = &user_log;
    if args.len() > 2 {
        user_log.warn(
            "More than two arguments given to controller executable. Ignoring extra arguments.",
        )?;
    }

    user_log.debug("Parsing configuration file...")?;
    let config_file = File::open(json_path)?;
    let config = match Configuration::parse(&mut BufReader::new(config_file)) {
        Ok(c) => c,
        Err(e) => {
            user_log.critical(&format!("Failed to parse configuration: {e}"))?;
            return Err(e.into());
        }
    };
    let config_ref = &config;
    user_log.debug("Successfully parsed configuration file")?;

    user_log.debug("Creating log files")?;

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
            group_files.push(file_create_new(&sensor_file_path)?);

            user_log.info(&format!("Created log file {:}", sensor_file_path.display()))?;
        }

        sensor_log_files.push(group_files);
    }

    // create log file for commands that have been executed
    let cmd_file = Mutex::new(file_create_new(PathBuf::from_iter([
        logs_path,
        "commands.csv",
    ]))?);
    let cmd_file_ref = &cmd_file;

    let mut drivers_file = file_create_new(PathBuf::from_iter([logs_path, "drivers.csv"]))?;
    let drivers_file_ref = &mut drivers_file;

    // when a client connects, the inner value of this mutex will be `Some` containing a TCP stream
    // to the dashboard
    let to_dash = DashChannel::new(file_create_new(PathBuf::from_iter([
        logs_path, "sent.csv",
    ]))?);
    let to_dash_ref = &to_dash;

    user_log.debug("Successfully created log files")?;

    let state = StateGuard::new(ControllerState::Standby);
    let state_ref = &state;

    user_log.debug("Now acquiring GPIO")?;

    let mut gpio_chip = M::chip()?;
    let bus = M::bus(&config, &mut gpio_chip)?;
    let adcs = M::adcs(&config, &mut gpio_chip, &bus)?;
    let adcs_ref = &adcs;

    let driver_lines = Mutex::new(M::drivers(&config, &mut gpio_chip)?);
    let driver_lines_ref = &driver_lines;

    user_log.debug("Successfully acquired GPIO handles")?;
    user_log.debug("Now spawning sensor listener threads...")?;

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
        let address = "0.0.0.0:2707";
        let listener = TcpListener::bind(address)?;

        user_log.info(&format!(
            "Opened TCP listener on address {}",
            listener.local_addr()?
        ))?;
        user_log.debug("Handling clients...")?;

        for client_res in listener.incoming() {
            let mut stream = match client_res {
                Ok(i) => i,
                Err(e) => {
                    user_log.warn(&format!("failed to collect incoming client: {e}"))?;
                    continue;
                }
            };
            user_log.info(&format!("Accepted client {:?}", stream.peer_addr()?))?;
            to_dash.set_channel(Some(stream.try_clone()?))?;

            user_log.debug("Overwrote to dashboard lock, now reading commands")?;

            #[allow(unused_must_use)]
            {
                // keep the port open even in error cases
                handle_client(
                    s,
                    to_dash_ref,
                    &mut stream,
                    config_ref,
                    driver_lines_ref,
                    cmd_file_ref,
                    user_log_ref,
                    state_ref,
                );
            }
        }

        Ok::<(), ControllerError>(())
    })?;
    // successful termination!
    Ok(())
}

/// Construct a new file with path `p` if there is not a file already there.
/// Returns a handle to the file if it was created.
/// IF the file already exists, returns an error.
///
/// TODO: remove this method and substitute with `File::create_new()` when it is stabilized.
fn file_create_new(p: impl AsRef<Path>) -> io::Result<File> {
    File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(p)
}

#[allow(clippy::too_many_arguments)]
/// Handle a single dashboard client.
fn handle_client<'a>(
    thread_scope: &'a Scope<'a, '_>,
    to_dash: &DashChannel<impl Write + Send, impl Write + Send>,
    from_dash: &mut impl Read,
    config: &'a Configuration,
    driver_lines: &'a Mutex<Vec<impl GpioPin + Send>>,
    cmd_log_file: &'a Mutex<impl Write + Send>,
    user_log: &'a UserLog<impl Write + Send>,
    state: &'a StateGuard,
) -> Result<(), ControllerError> {
    to_dash.send(&Message::Config { config })?;
    user_log.debug("Successfully sent configuration to dashboard.")?;
    loop {
        let cmd = match Command::parse(from_dash) {
            Ok(cmd) => cmd,
            Err(e) => {
                match e {
                    ParseError::SourceClosed => {
                        user_log.info("Dashboard disconnected")?;
                        return Ok(());
                    }
                    ParseError::Malformed(s) => {
                        user_log.warn(&format!("Received invalid command {s}"))?;
                    }
                    ParseError::Io(e) => {
                        user_log.warn(&format!("encountered I/O error: {e}"))?;
                        return Err(ControllerError::Io(e));
                    }
                }
                continue;
            }
        };

        if matches!(
            cmd,
            Command::Actuate {
                driver_id: _,
                value: _,
            }
        ) {
            handle_command(&cmd, cmd_log_file, user_log, config, driver_lines, state)?;
        } else {
            // spawn thread to handle command
            thread_scope.spawn(move || {
                handle_command(&cmd, cmd_log_file, user_log, config, driver_lines, state)
            });
        }

        user_log.debug("Finished executing command.")?;
    }
}
