//! Specification of "outbound" parts of the API, which travel from controller
//! to dashboard.

use std::time::SystemTime;

use serde::Serialize;

use crate::config::Configuration;

#[derive(Serialize)]
#[serde(tag = "type")]
/// The set of messages which can be sent from the controller to the dashboard.
pub enum Message<'a> {
    /// A confirmation to the dashboard that the controller is ready.
    Ready,
    /// A configuration message.
    Config {
        /// A reference to the entire configuration object for this controller.
        config: &'a Configuration,
    },
    /// A sensor valuation message.
    /// Each key in the map corresponds to a sensor.
    /// Each value corresponds to a time at which a sensor value was taken and
    /// the ADC value read at that time.
    SensorValue {
        /// The group which generated the readings.
        group_id: u8,
        /// The readings which were created.
        readings: &'a [SensorReading],
    },
    /// A driver state message.
    /// Describes the state of the drivers on the controller.
    DriverValue {
        /// The state of each driver.
        /// Each index corresponds to the driver at the same index in the
        /// original configuration object.
        state: Vec<bool>,
    },
    /// A display message, which will write out a string to the dashboard.
    Display {
        /// The message to display to the user.
        message: &'a str,
    },
    /// An error message for the dashboard to display for the user.
    Error {
        /// The root problem which caused the error to be sent.
        cause: ErrorCause<'a>,
        /// A diagnostic string providing information about the error.
        diagnostic: &'a str,
    },
}

#[derive(Serialize)]
/// An individual reading on a sensor.
pub struct SensorReading {
    /// The ID of the sensor withing the group that created this reading.
    pub sensor_id: u8,
    /// The value read on the sensor.
    pub reading: u16,
    /// The time at which the sensor reading was created.
    pub time: SystemTime,
}

#[derive(Serialize)]
#[serde(tag = "type")]
/// The set of error causes that can be displayed to the dashboard.
pub enum ErrorCause<'a> {
    /// A command from the dashboard was malformed.
    /// Send back a copy of the incorrect command.
    Malformed {
        /// The original message which failed to be parsed.
        original_message: &'a str,
    },
    /// A read from a sensor failed.
    /// Give the ID of the sensor which failed to be read.
    SensorFail {
        /// The ID of the group which contains the failed sensor.
        group_id: u8,
        /// The ID of the sensor within the group which failed to be read.
        sensor_id: u8,
    },
    /// The OS denied permission for some functionality of the controller.
    Permission,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::Value;

    use super::*;

    /// Helper function to test that the serialized result is the same as the
    /// expected result, independent of whitespace or key ordering.
    fn serialize_helper(expected: &str, message: &Message) {
        let message_value = serde_json::to_value(message).unwrap();
        let expected_value = serde_json::from_str::<Value>(expected).unwrap();

        assert_eq!(message_value, expected_value);
    }

    #[test]
    /// Test that a sensor value message is serialized correctly.
    fn serialize_sensor_value() {
        serialize_helper(
            r#"{
                "type": "SensorValue",
                "group_id": 0,
                "readings": [
                    {
                        "sensor_id": 0,
                        "reading": 3456,
                        "time": {
                            "secs_since_epoch": 1651355351,
                            "nanos_since_epoch": 534000000
                        } 
                    }
                ]
            }"#,
            &Message::SensorValue {
                group_id: 0,
                readings: &[SensorReading {
                    sensor_id: 0,
                    reading: 3456,
                    time: SystemTime::UNIX_EPOCH + Duration::from_millis(1_651_355_351_534),
                }],
            },
        );
    }

    #[test]
    /// Test that a driver value message is serialized correctly.
    fn serialize_driver_value() {
        serialize_helper(
            r#"{
                "type": "DriverValue",
                "state": [
                    false,
                    true,
                    false
                ]
            }"#,
            &Message::DriverValue {
                state: vec![false, true, false],
            },
        );
    }

    #[test]
    /// Test that a display message is serialized correctly.
    fn serialize_display() {
        serialize_helper(
            r#"{
                "type": "Display",
                "message": "The weather today is expected to be mostly sunny, with a high of 73 degrees Fahrenheit."
            }"#,
            &Message::Display {
                message: "The weather today is expected to be mostly sunny, with a high of 73 degrees Fahrenheit."
            },
        );
    }

    #[test]
    /// Test that a malformed error message is serialized correctly.
    fn serialize_error_malformed() {
        serialize_helper(
            r#"{
                "type": "Error",
                "diagnostic": "expected key `driver_id` not found",
                "cause": {
                    "type": "Malformed",
                    "original_message": "{\"type\": \"actuate\"}"
                }
            }"#,
            &Message::Error {
                diagnostic: "expected key `driver_id` not found",
                cause: ErrorCause::Malformed {
                    original_message: "{\"type\": \"actuate\"}",
                },
            },
        );
    }

    #[test]
    /// Test that a failed sensor error message is serialized correctly.
    fn serialize_sensor_fail() {
        serialize_helper(
            r#"{
                "type": "Error",
                "diagnostic": "SPI transfer for LC_MAIN failed",
                "cause": {
                    "type": "SensorFail",
                    "group_id": 0,
                    "sensor_id": 0
                }
            }"#,
            &Message::Error {
                diagnostic: "SPI transfer for LC_MAIN failed",
                cause: ErrorCause::SensorFail {
                    group_id: 0,
                    sensor_id: 0,
                },
            },
        );
    }

    #[test]
    /// Test that a failed permission error message is serialized correctly.
    fn serialize_permission() {
        serialize_helper(
            r#"{
                "type": "Error",
                "diagnostic": "could not write to log file `log_LC_MAIN.txt`",
                "cause": {
                    "type": "Permission"
                }
            }"#,
            &Message::Error {
                diagnostic: "could not write to log file `log_LC_MAIN.txt`",
                cause: ErrorCause::Permission,
            },
        );
    }
}
