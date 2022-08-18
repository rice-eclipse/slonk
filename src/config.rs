use std::{io::Read, time::Duration};

use serde::{Deserialize, Serialize};

use crate::hardware::Mcp3208;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
/// A configuration for the entire engine controller.
/// Contains all necessary data for both the controller and dashboard to
/// operate correctly.
pub struct Configuration {
    /// The frequency at which driver and system status updates should be sent
    /// to the dashboard.
    pub frequency_status: u32,
    /// The families of sensors, each having their own frequencies and manager
    /// threads.
    pub sensor_groups: Vec<SensorGroup>,
    /// The drivers, which actuate external digital pins.
    pub drivers: Vec<Driver>,
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
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
/// Information required to define a driver.
pub struct Driver {
    /// The human-readable name of the driver.
    pub label: String,
    /// The pin actuated by the driver.
    pub pin: u8,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "type")]
/// The set of actions that can be taken in an ignition or shutoff sequence.
pub enum Action {
    /// Actuate a driver to a state.
    Actuate {
        /// The identifier (i.e. index) of the driver to be actuated.
        driver_id: u8,
        /// The state that the driver will be actuated to.
        state: bool,
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
    /// The frequency at which data should be transmitted to the dashboard from
    /// this sensor group.
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
    /// The units of the sensor's calibrated value.
    pub units: String,
    /// The minimum and maximum allowable range of values that the calibrated
    /// value of this sensor can take be allowed to take on until an estop is triggered.
    pub range: Option<(f64, f64)>,
    /// The intercept of the linear calibration function for this sensor.
    /// In the expression of the calibration function `y = mx + b`, this would
    /// be `b`.
    pub calibration_intercept: f64,
    /// The slope of the linear calibration function for this sensor.
    /// In the expression of the calibration function `y = mx + b`, this would
    /// be `m`.
    pub calibration_slope: f64,
    /// The width of a rolling average for this device, used to filter data on
    /// the controller side.
    pub rolling_average_width: u32,
    /// The ID of the ADC used by this device.
    /// This maps to the field `adc_cs` in `Configuration`.
    /// For instance, if the value of `adc` is 2, and `adc[2]` is 33, then this
    /// sensor uses the ADC with chip select pin 33.
    pub adc: u8,
    /// The channel on the ADC to use for
    pub channel: u8,
}

#[derive(Debug, PartialEq, Eq)]
/// The set of errors that can occur when validating a configuration.
pub enum Error {
    /// The configuration was malformed and could not be parsed into a
    /// `Configuration`object.
    Malformed,
    /// A sensor's definition referred to an ADC which did not exist.
    NoSuchAdc,
    /// A sensor's definition referred to a channel which is out of bounds on an
    /// ADC.
    BadChannel,
    /// The SPI clock frequency was set too slow.
    ClockTooSlow,
}

impl Configuration {
    /// Construct a new `Configuration` by parsing some readable source.
    /// Will also check the configuration to determine that there are no logical
    /// inconsistencies in its definition.
    ///
    /// # Errors
    ///
    /// This function will return errors in line with the definition of `Error`
    /// in this module.
    pub fn parse(source: &mut impl Read) -> Result<Configuration, Error> {
        // deserialize the configuration
        let config: Configuration =
            serde_json::from_reader(source).map_err(|_| Error::Malformed)?;

        // now validate it

        if u64::from(config.spi_frequency_clk) < Mcp3208::SPI_MIN_FREQUENCY {
            return Err(Error::ClockTooSlow);
        }

        for group in &config.sensor_groups {
            for sensor in &group.sensors {
                if usize::from(sensor.adc) >= config.adc_cs.len() {
                    return Err(Error::NoSuchAdc);
                }

                if sensor.channel >= 8 {
                    return Err(Error::BadChannel);
                }
            }
        }

        // all validation steps passed
        Ok(config)
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
        let config_str = r#"{
            "frequency_status": 10,
            "sensor_groups": [
                {
                    "label": "FAST",
                    "frequency_standby": 10,
                    "frequency_ignition": 1000,
                    "frequency_transmission": 10,
                    "sensors": [
                        {
                            "label": "LC_MAIN",
                            "units": "lb",
                            "calibration_intercept": 0.34,
                            "calibration_slope": 33.2,
                            "rolling_average_width": 5,
                            "adc": 0,
                            "channel": 0
                        },
                        {
                            "label": "PT_FEED",
                            "units": "psi",
                            "range": [-500, 3000],
                            "calibration_intercept": 92.3,
                            "calibration_slope": -302.4,
                            "rolling_average_width": 4,
                            "adc": 0,
                            "channel": 1
                        }
                    ]
                }
            ],
            "drivers": [
                {
                    "label": "OXI_FILL",
                    "pin": 33
                }
            ],
            "ignition_sequence": [
                {
                    "type": "Actuate",
                    "driver_id": 0,
                    "state": true
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
                    "state": false
                }
            ],
            "estop_sequence": [
                {
                    "type": "Actuate",
                    "driver_id": 0,
                    "state": false
                }
            ],
            "spi_mosi": 26,
            "spi_miso": 27,
            "spi_clk": 28,
            "spi_frequency_clk": 50000,
            "adc_cs": [
                37
            ]
        }"#;
        let config = Configuration {
            frequency_status: 10,
            sensor_groups: vec![SensorGroup {
                label: "FAST".into(),
                frequency_standby: 10,
                frequency_ignition: 1000,
                frequency_transmission: 10,
                sensors: vec![
                    Sensor {
                        label: "LC_MAIN".into(),
                        units: "lb".into(),
                        range: None,
                        calibration_intercept: 0.34,
                        calibration_slope: 33.2,
                        rolling_average_width: 5,
                        adc: 0,
                        channel: 0,
                    },
                    Sensor {
                        label: "PT_FEED".into(),
                        units: "psi".into(),
                        range: Some((-500., 3000.)),
                        calibration_intercept: 92.3,
                        calibration_slope: -302.4,
                        rolling_average_width: 4,
                        adc: 0,
                        channel: 1,
                    },
                ],
            }],
            drivers: vec![Driver {
                label: "OXI_FILL".into(),
                pin: 33,
            }],
            ignition_sequence: vec![
                Action::Actuate {
                    driver_id: 0,
                    state: true,
                },
                Action::Sleep {
                    duration: Duration::from_secs(10),
                },
                Action::Actuate {
                    driver_id: 0,
                    state: false,
                },
            ],
            estop_sequence: vec![Action::Actuate {
                driver_id: 0,
                state: false,
            }],
            spi_mosi: 26,
            spi_miso: 27,
            spi_clk: 28,
            spi_frequency_clk: 50_000,
            adc_cs: vec![37],
        };

        let mut cursor = Cursor::new(config_str);
        assert!(Ok(config).eq(&Configuration::parse(&mut cursor)));
    }
}
