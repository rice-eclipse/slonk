//! Specification of "outbound" parts of the API, which travel from controller
//! to dashboard.

use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{ser::SerializeMap, Serialize, Serializer};

/// The set of messages which can be sent from the controller to the dashboard.
pub enum Message<'a> {
    /// A configuration message.
    /// The contained value is a JSON string containing the full configuration
    /// file.
    ///
    /// TODO: Do not use a string here, and instead use a configuration struct
    /// we make ourselves.
    Config(&'a str),
    /// A sensor valuation message.
    /// Each key in the map corresponds to a sensor.
    /// Each value corresponds to a time at which a sensor value was taken and
    /// the ADC value read at that time.
    SensorValue(&'a HashMap<String, (SystemTime, u16)>),
    /// A driver state message.
    /// Describes the state of the drivers on the controller.
    /// Each key in the map corresponds to a unique driver, and each value
    /// corresponds to its state.
    DriverValue(&'a HashMap<String, bool>),
    /// A display message, which will write out a string to the dashboard.
    Display(&'a str),
    /// An error message for the dashboard to display for the user.
    Error {
        /// The root problem which caused the error to be sent.
        cause: ErrorCause<'a>,
        /// A diagnostic string providing information about the error.
        diagnostic: &'a str,
    },
}

/// The set of error causes that can be displayed to the dashboard.
pub enum ErrorCause<'a> {
    /// A command from the dashboard was malformed.
    /// Send back a copy of the incorrect command.
    Malformed(&'a str),
    /// A read from a sensor failed.
    /// Give the ID of the sensor which failed to be read.
    SensorFail(&'a str),
    /// The OS denied permission for some functionality of the controller.
    Permission,
}

/// Hacky wrapper struct to change the way that serde will serialize our system
/// times.
struct TimedDatum(SystemTime, u16);

impl Serialize for TimedDatum {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        #[allow(clippy::cast_possible_truncation)]
        map.serialize_entry(
            "time",
            &(self
                .0
                .duration_since(UNIX_EPOCH)
                .expect("have we traveled back in time?")
                .as_millis() as u64),
        )?;
        map.serialize_entry("adc", &self.1)?;
        map.end()
    }
}

impl Serialize for Message<'_> {
    /// Serialize a message for the dasboard.
    ///
    /// # Panics
    ///
    /// This function will panic if the system time is before the Unix epoch.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let n_fields = match self {
            Message::Error {
                cause,
                diagnostic: _,
            } => {
                (match cause {
                    // different causes will add different numbers of keys
                    ErrorCause::Malformed(_) | ErrorCause::SensorFail(_) => 1,
                    ErrorCause::Permission => 0,
                }) + 1
            }
            _ => 1,
        };
        // add 2 to number of fields so that we can add message type and
        // timestamp
        let mut obj = serializer.serialize_map(Some(n_fields + 2))?;

        // Add data fields
        match self {
            Message::Config(_) => todo!(),
            Message::SensorValue(data) => {
                let new_data = data
                    .iter()
                    .map(|(key, &(time, adc))| (key, TimedDatum(time, adc)))
                    .collect::<HashMap<&String, _>>();
                obj.serialize_entry("data", &new_data)?;
            }
            Message::DriverValue(state) => obj.serialize_entry("state", state)?,
            Message::Display(message) => obj.serialize_entry("message", message)?,
            Message::Error { cause, diagnostic } => {
                obj.serialize_entry("diagnostic", diagnostic)?;
                let cause_name = match cause {
                    ErrorCause::Malformed(_) => "malformed",
                    ErrorCause::SensorFail(_) => "sensor_fail",
                    ErrorCause::Permission => "permission",
                };
                obj.serialize_entry("cause", cause_name)?;
                // now serialize specialized fields of this cause
                match cause {
                    ErrorCause::Malformed(bad_msg) => {
                        obj.serialize_entry("original_message", bad_msg)?;
                    }
                    ErrorCause::SensorFail(sensor_id) => {
                        obj.serialize_entry("sensor_id", sensor_id)?;
                    }
                    ErrorCause::Permission => (),
                };
            }
        };

        // Add an entry for the message type
        obj.serialize_entry(
            "message_type",
            match self {
                Message::Config(_) => "configuration",
                Message::SensorValue(_) => "sensor_value",
                Message::DriverValue(_) => "driver_value",
                Message::Display(_) => "display",
                Message::Error {
                    cause: _,
                    diagnostic: _,
                } => "error",
            },
        )?;

        // Time of sending can be time of serialization, I suppose
        let send_time = SystemTime::now();
        #[allow(clippy::cast_possible_truncation)]
        obj.serialize_entry(
            "send_time",
            &(send_time
                .duration_since(UNIX_EPOCH)
                .expect("have we traveled back in time?")
                .as_millis() as u64),
        )?;
        obj.end()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::Value;

    use super::*;

    /// Helper function to test that the serialized result is the same as the
    /// expected result, independent of whitespace or key ordering.
    fn serialize_helper(expected: &str, message: &Message) {
        let mut message_value = serde_json::to_value(message).unwrap();
        let mut expected_value = serde_json::from_str::<Value>(expected).unwrap();

        // verify that a timestamp was created, then remove it because we cannot
        // test it for equality against our reference
        let message_obj = message_value.as_object_mut().unwrap();
        assert!(message_obj.remove("send_time").unwrap().is_number());

        let expected_obj = expected_value.as_object_mut().unwrap();
        expected_obj.remove("send_time");

        assert_eq!(message_obj, expected_obj);
    }

    #[test]
    /// Test that a sensor value message is serialized correctly.
    fn serialize_sensor_value() {
        let mut map = HashMap::new();
        map.insert(
            "PT_FEED".into(),
            (UNIX_EPOCH + Duration::from_millis(1_651_355_351_534), 3456),
        );
        map.insert(
            "LC_MAIN".into(),
            (UNIX_EPOCH + Duration::from_millis(1_651_355_351_462), 125),
        );

        serialize_helper(
            r#"{
            "message_type": "sensor_value",
            "send_time": 1651355351791,
            "data": {
                "PT_FEED": {
                    "time": 1651355351534,
                    "adc": 3456
                },
                "LC_MAIN": {
                    "time": 1651355351462,
                    "adc": 125
                }
            }
        }"#,
            &Message::SensorValue(&map),
        );
    }

    #[test]
    /// Test that a driver value message is serialized correctly.
    fn serialize_driver_value() {
        let mut map = HashMap::new();
        map.insert("OXI_FILL".into(), false);
        map.insert("ENGI_VENT".into(), true);
        map.insert("IGNITION".into(), false);

        serialize_helper(
            r#"{
                "message_type": "driver_value",
                "send_time": 1651355351791,
                "state": {
                    "OXI_FILL": false,
                    "ENGI_VENT": true,
                    "IGNITION": false
                }
            }"#,
            &Message::DriverValue(&map),
        );
    }

    #[test]
    /// Test that a Display message is correctly serialized.
    fn serialize_display() {
        serialize_helper(
            r#"{
            "message_type": "display",
            "send_time": 3133675200,
            "message": "The weather today is expected to be mostly sunny, with a high of 73 degrees Fahrenheit."
        }"#,
            &Message::Display("The weather today is expected to be mostly sunny, with a high of 73 degrees Fahrenheit."),
        );
    }

    #[test]
    /// Test that a malformed error message is correctly serialized.
    fn serialize_malformed() {
        serialize_helper(
            r#"{
                "message_type": "error",
                "send_time": 1651355351791,
                "cause": "malformed",
                "diagnostic": "expected key `driver_id` not found",
                "original_message": "{\"message_type\": \"actuate\",\"send_time\": 165135535000}"
            }"#,
            &Message::Error {
                cause: ErrorCause::Malformed(
                    "{\"message_type\": \"actuate\",\"send_time\": 165135535000}",
                ),
                diagnostic: "expected key `driver_id` not found",
            },
        );
    }

    #[test]
    /// Test that a failed sensor read error message is serialized correctly.
    fn serialize_error_sensor_fail() {
        serialize_helper(
            r#"{
            "message_type": "error",
            "send_time": 1651355351791,
            "cause": "sensor_fail",
            "diagnostic": "SPI transfer for LC_MAIN failed",
            "sensor_id": "LC_MAIN"
        }"#,
            &Message::Error {
                cause: ErrorCause::SensorFail("LC_MAIN"),
                diagnostic: "SPI transfer for LC_MAIN failed",
            },
        );
    }

    #[test]
    /// Test that a permissions-based error is serialized correctly.
    fn serialize_error_permission() {
        serialize_helper(
            r#"{
                "message_type": "error",
                "send_time": 1651355351791,
                "cause": "permission",
                "diagnostic": "could not write to log file `log_LC_MAIN.txt`"
            }"#,
            &Message::Error {
                cause: ErrorCause::Permission,
                diagnostic: "could not write to log file `log_LC_MAIN.txt`",
            },
        );
    }
}
