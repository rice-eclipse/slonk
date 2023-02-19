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

//! Loading and validating configurations for the engine controller.

use std::{collections::HashSet, io::Read, time::Duration};

use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::hardware::{ListenerPin, Mcp3208};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
/// A configuration for the entire engine controller.
/// Contains all necessary data for both the controller and dashboard to operate correctly.
pub struct Configuration {
    /// The frequency at which driver and system status updates should be sent to the dashboard.
    pub frequency_status: u32,
    /// The size that a log buffer should be.
    /// When a log buffer fills up, its readings are saved to a log file.
    pub log_buffer_size: usize,
    /// The families of sensors, each having their own frequencies and manager threads.
    pub sensor_groups: Vec<SensorGroup>,
    /// The drivers, which actuate external digital pins.
    pub drivers: Vec<Driver>,
    /// The amount of time (in millsieconds) to wait in a pre-ignition state after processing an
    /// ignition command.
    ///
    /// During pre-ignition, the sensor logging rate is fast, but no actual ignition has happened
    /// yet.
    ///
    /// This value is *not* the duration of the count down; it is only used for thread
    /// synchronization.
    pub pre_ignite_time: u32,
    /// The amount of time to wait in a post-ignition state after processing an
    /// ignition command.
    ///
    /// During pre-ignition, the sensor logging rate is fast, but there
    /// (should) be no more oxidizer flow and ignition should have stopped.
    pub post_ignite_time: u32,
    /// The sequence of actions to be performed during ignition.
    pub ignition_sequence: Vec<Action>,
    /// The sequence of actions to be performed during emergency stop.
    pub estop_sequence: Vec<Action>,
    /// The Master Output / Slave Input pin ID for the SPI bus.
    pub spi_mosi: u8,
    /// The Master Input / Slave Output pin ID for the SPI bus.
    pub spi_miso: u8,
    /// The clock pin ID for the SPI bus.
    pub spi_clk: u8,
    /// The operating frequency of the SPI bus clock.
    /// Can be no less than 10 kHz for the ADCs to operate correctly.
    pub spi_frequency_clk: u32,
    /// The chip select pins for each device.
    /// For now, we assume that all ADCs are MCP3208s.
    pub adc_cs: Vec<u8>,
    /// The GPIO pin ID of the heartbeat LED.
    pub pin_heartbeat: u8,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
/// Information required to define a driver.
pub struct Driver {
    /// The human-readable name of the driver.
    pub label: String,
    /// The label for the action that will be performed when the driver is turned on.
    pub label_actuate: String,
    /// The label for the action that will be performed when the driver is turned off.
    pub label_deactuate: String,
    /// The pin actuated by the driver.
    pub pin: u8,
    /// Whether this driver is protected from user access.
    pub protected: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "type")]
/// The set of actions that can be taken in an ignition or shutoff sequence.
pub enum Action {
    /// Actuate a driver to a logic level.
    Actuate {
        /// The identifier (i.e. index) of the driver to be actuated.
        driver_id: u8,
        /// The logic level that the driver will be actuated to.
        value: bool,
    },
    /// Sleep for a given amount of time.
    Sleep {
        /// The amount of time to sleep for.
        duration: Duration,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
/// Information needed to define a group of sensors.
pub struct SensorGroup {
    /// The human-readable label of the sensor group.
    pub label: String,
    /// The frequency at which data should be collected while in standby mode.
    pub frequency_standby: u32,
    /// The frequency at which data should be collected while in ignition mode.
    pub frequency_ignition: u32,
    /// The frequency at which data should be transmitted to the dashboard from this sensor group.
    /// If no data is available, no new data will be sent.
    pub frequency_transmission: u32,
    /// The set of sensors managed by this sensor group.
    pub sensors: Vec<Sensor>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
/// Information needed to define a single sensor.
pub struct Sensor {
    /// The label giving the name of the sensor.
    pub label: String,
    /// The color that this sensor should be displayed with.
    pub color: String,
    /// The units of the sensor's calibrated value.
    pub units: String,
    /// The minimum and maximum allowable range of values that the calibrated
    /// value of this sensor can take be allowed to take on until an estop is triggered.
    pub range: Option<(f64, f64)>,
    /// The intercept of the linear calibration function for this sensor.
    /// In the expression of the calibration function `y = mx + b`, this would be `b`.
    pub calibration_intercept: f64,
    /// The slope of the linear calibration function for this sensor.
    /// In the expression of the calibration function `y = mx + b`, this would be `m`.
    pub calibration_slope: f64,
    /// The width of a rolling average for this device, used to filter data on the controller side.
    pub rolling_average_width: Option<u32>,
    /// The ID of the ADC used by this device.
    /// This maps to the field `adc_cs` in `Configuration`.
    /// For instance, if the value of `adc` is 2, and `adc[2]` is 33, then this sensor uses the ADC
    /// with chip select pin 33.
    pub adc: u8,
    /// The channel on the ADC to to read raw sensor data from.
    pub channel: u8,
}

#[derive(Debug)]
/// The set of errors that can occur when validating a configuration.
pub enum Error {
    /// The configuration was malformed and could not be parsed into a`Configuration` object.
    /// The string is a description of the cause of the error.
    Malformed(serde_json::Error),
    /// A sensor's definition referred to an ADC which did not exist.
    NoSuchAdc(u8),
    /// A sensor's definition referred to a channel which is out of bounds on an ADC.
    BadChannel(u8),
    /// The SPI clock frequency was set too slow.
    ClockTooSlow,
    /// A procedure references a driver which does not exist.
    NoSuchDriver(u8),
    /// Two pins are duplicated for differing functions.
    DuplicatePin(u8),
    /// A pin is used for
    ReservedPin(u8),
}

impl Configuration {
    /// Construct a new `Configuration` by parsing some readable source.
    /// Will also check the configuration to determine that there are no logical inconsistencies in
    /// its definition.
    ///
    /// # Errors
    ///
    /// This function will return errors in line with the definition of `Error` in this module.
    pub fn parse(source: &mut impl Read) -> Result<Configuration, Error> {
        // deserialize the configuration
        let config: Configuration = serde_json::from_reader(source).map_err(Error::Malformed)?;

        // now validate it

        // check that SPI frequency is correct
        if u64::from(config.spi_frequency_clk) < Mcp3208::<ListenerPin>::SPI_MIN_FREQUENCY {
            return Err(Error::ClockTooSlow);
        }

        // check that each sensor has an ADC associated with it
        for group in &config.sensor_groups {
            for sensor in &group.sensors {
                if usize::from(sensor.adc) >= config.adc_cs.len() {
                    return Err(Error::NoSuchAdc(sensor.adc));
                }

                if sensor.channel >= 8 {
                    return Err(Error::BadChannel(sensor.channel));
                }
            }
        }

        // check that actuations correspond to real drivers
        for procedure in [&config.ignition_sequence, &config.estop_sequence] {
            for step in procedure {
                let Action::Actuate { driver_id, value: _ } = step else { continue; };
                if usize::from(*driver_id) > config.drivers.len() {
                    return Err(Error::NoSuchDriver(*driver_id));
                }
            }
        }

        // check that no pins are reused in the configuration
        // also, check that no illegal pins (i.e. ones on the Raspberry Pi which are reserved) are
        // used
        let mut pins_used = HashSet::new();
        for pin in config
            .drivers
            .iter()
            .map(|d| d.pin)
            .chain([config.spi_mosi, config.spi_miso, config.spi_clk])
            .chain(config.adc_cs.iter().copied())
        {
            if !is_legal(pin) {
                return Err(Error::ReservedPin(pin));
            }
            if pins_used.contains(&pin) {
                return Err(Error::DuplicatePin(pin));
            }
            pins_used.insert(pin);
        }

        // all validation steps passed
        Ok(config)
    }
}

/// Determine whether a GPIO pin ID is a legal pin for use in the controller.
fn is_legal(pin: u8) -> bool {
    // There are GPIO pins 0 through 27 (inclusive).
    // However, pins 0 and 1 are reserved for EEPROM.
    1 < pin && pin <= 27
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Malformed(json_err) => {
                write!(f, "Failed to parse JSON for configuration: {json_err}")
            }
            Error::NoSuchAdc(a) => write!(
                f,
                "ADC {a} is referenced but not listed in set of ADC chip select pins"
            ),
            Error::BadChannel(c) => write!(f, "ADC channel {c} referenced (must be in 0..=7)"),
            Error::ClockTooSlow => write!(
                f,
                "SPI clock frequency is too slow (must be at least {} Hz)",
                Mcp3208::<ListenerPin>::SPI_MIN_FREQUENCY
            ),
            Error::NoSuchDriver(d) => write!(f, "A procedure refers to a driver with ID {d}, but no such driver is given in the list of drivers"),
            Error::DuplicatePin(p) => write!(f, "GPIO pin {p} is used for multiple purposes"),
            Error::ReservedPin(p) => write!(f, "GPIO pin {p} is not allowed to be used on the Raspberry Pi"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    #[allow(clippy::too_many_lines)]
    /// Test the parsing of a full configuration string.
    fn full_config() {
        let config_str = r##"{
            "frequency_status": 10,
            "log_buffer_size": 256,
            "sensor_groups": [
                {
                    "label": "FAST",
                    "frequency_standby": 10,
                    "frequency_ignition": 1000,
                    "frequency_transmission": 10,
                    "sensors": [
                        {
                            "label": "LC_MAIN",
                            "color": "#ef3b9e",
                            "units": "lb",
                            "calibration_intercept": 0.34,
                            "calibration_slope": 33.2,
                            "rolling_average_width": 5,
                            "adc": 0,
                            "channel": 0
                        },
                        {
                            "label": "PT_FEED",
                            "color": "#ef3b9e",
                            "units": "psi",
                            "range": [-500, 3000],
                            "calibration_intercept": 92.3,
                            "calibration_slope": -302.4,
                            "adc": 0,
                            "channel": 1
                        }
                    ]
                }
            ],
            "pre_ignite_time": 500,
            "post_ignite_time": 5000,
            "drivers": [
                {
                    "label": "OXI_FILL",
                    "label_actuate": "Open",
                    "label_deactuate": "Close",
                    "pin": 21,
                    "protected": false
                }
            ],
            "ignition_sequence": [
                {
                    "type": "Actuate",
                    "driver_id": 0,
                    "value": true
                },
                {
                    "type": "Sleep",
                    "duration": {
                        "secs": 10,
                        "nanos": 0
                    }
                },
                {
                    "type": "Actuate",
                    "driver_id": 0,
                    "value": false
                }
            ],
            "estop_sequence": [
                {
                    "type": "Actuate",
                    "driver_id": 0,
                    "value": false
                }
            ],
            "spi_mosi": 26,
            "spi_miso": 25,
            "spi_clk": 24,
            "spi_frequency_clk": 50000,
            "adc_cs": [
                20
            ],
            "pin_heartbeat": 0
        }"##;
        let config = Configuration {
            frequency_status: 10,
            log_buffer_size: 256,
            sensor_groups: vec![SensorGroup {
                label: "FAST".into(),
                frequency_standby: 10,
                frequency_ignition: 1000,
                frequency_transmission: 10,
                sensors: vec![
                    Sensor {
                        label: "LC_MAIN".into(),
                        color: "#ef3b9e".into(),
                        units: "lb".into(),
                        range: None,
                        calibration_intercept: 0.34,
                        calibration_slope: 33.2,
                        rolling_average_width: Some(5),
                        adc: 0,
                        channel: 0,
                    },
                    Sensor {
                        label: "PT_FEED".into(),
                        color: "#ef3b9e".into(),
                        units: "psi".into(),
                        range: Some((-500., 3000.)),
                        calibration_intercept: 92.3,
                        calibration_slope: -302.4,
                        rolling_average_width: None,
                        adc: 0,
                        channel: 1,
                    },
                ],
            }],
            pre_ignite_time: 500,
            post_ignite_time: 5000,
            drivers: vec![Driver {
                label: "OXI_FILL".into(),
                label_actuate: "Open".into(),
                label_deactuate: "Close".into(),
                pin: 21,
                protected: false,
            }],
            ignition_sequence: vec![
                Action::Actuate {
                    driver_id: 0,
                    value: true,
                },
                Action::Sleep {
                    duration: Duration::from_secs(10),
                },
                Action::Actuate {
                    driver_id: 0,
                    value: false,
                },
            ],
            estop_sequence: vec![Action::Actuate {
                driver_id: 0,
                value: false,
            }],
            spi_mosi: 26,
            spi_miso: 25,
            spi_clk: 24,
            spi_frequency_clk: 50_000,
            adc_cs: vec![20],
            pin_heartbeat: 0,
        };

        let mut cursor = Cursor::new(config_str);
        assert_eq!(config, Configuration::parse(&mut cursor).unwrap());
    }
}
