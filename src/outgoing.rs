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

//! Specification of "outbound" parts of the API, which travel from controller
//! to dashboard.

use std::{
    io::Write,
    sync::{Arc, Mutex, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::SystemTime,
};

use serde::Serialize;

use crate::config::Configuration;

#[derive(Serialize)]
#[serde(tag = "type")]
/// The set of messages which can be sent from the controller to the dashboard.
pub enum Message<'a> {
    /// A configuration message.
    Config {
        /// A reference to the entire configuration object for this controller.
        config: &'a Configuration,
    },
    /// A sensor valuation message.
    /// Each key in the map corresponds to a sensor.
    /// Each value corresponds to a time at which a sensor value was taken and the ADC value read at
    /// that time.
    SensorValue {
        /// The group which generated the readings.
        group_id: u8,
        /// The readings which were created.
        readings: &'a [SensorReading],
    },
    /// A driver values message.
    /// Describes the logic levels of the drivers on the controller.
    DriverValue {
        /// The logic level of each driver.
        /// Each index corresponds to the driver at the same index in the
        /// original configuration object.
        values: &'a [bool],
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

/// A channel which can write to the dashboard.
/// It contains a writer for a channel to the dashboard and to a message log.
///
/// # Types
///
/// * `C`: the type of the channel to the dashboard.
/// * `M`: the type of the log file to be written to.
pub struct DashChannel<C: Write, M: Write> {
    /// A channel for the dashboard.
    /// If writing to this channel fails, it will be immediately overwritten with `None`.
    /// When `dash_channel` is `None`, nothing will be written.
    pub dash_channel: Arc<RwLock<Option<C>>>,
    /// The log file for all messages that are sent.
    message_log: Mutex<M>,
}

#[derive(Debug)]
/// The errors which can occur when writing an outgoing message.
pub enum Error {
    /// We failed to write to the message log (possibly because it was deleted?)
    LogFile(std::io::Error),
    /// A lock was poisoned.
    Poison,
}

impl<C: Write, M: Write> DashChannel<C, M> {
    /// Construct a new `DashChannel` with no outgoing channel.
    pub fn new(message_log: M) -> DashChannel<C, M> {
        DashChannel {
            dash_channel: Arc::new(RwLock::new(None)),
            message_log: Mutex::new(message_log),
        }
    }

    /// Write a message to the dashboard.
    /// After writing the message, log that the message was written.
    ///
    /// If writing the message to the dashboard
    ///     
    /// # Errors
    ///
    /// This function will return an `Err` if we are unable to write to the message log.
    ///
    /// # Panics
    ///
    /// This function will panic if the current time is before the UNIX epoch.
    pub fn send(&self, message: &Message) -> Result<(), Error> {
        let mut channel_guard = self.dash_channel.write().map_err(|_| Error::Poison)?;
        let mut message_log_guard = self.message_log.lock().map_err(|_| Error::Poison)?;
        if let Some(ref mut writer) = *channel_guard {
            if serde_json::to_writer(&mut *writer, message).is_ok() {
                // log that we sent this message to the dashboard
                // first, mark the time
                write!(
                    message_log_guard,
                    "{},",
                    SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                )
                .map_err(Error::LogFile)?;
                // then, the message
                serde_json::to_writer(&mut *message_log_guard, message)
                    .map_err(|e| Error::LogFile(std::io::Error::from(e)))?;
                // then a trailing newline
                writeln!(message_log_guard).map_err(Error::LogFile)?;
            } else {
                *channel_guard = None;
            }
        }

        Ok(())
    }

    /// Determine whether this channel actually has a target to send messages to.
    ///
    /// # Errors
    ///
    /// This function may return an `Err` if an internal lock is poisoned.
    pub fn has_target(&self) -> Result<bool, PoisonError<RwLockReadGuard<Option<C>>>> {
        Ok(self.dash_channel.read()?.is_some())
    }

    /// Set the outgoing channel for this stream to be `channel`.
    ///
    /// # Errors
    ///
    /// This function may return an `Err` if an internal lock is poisoned.
    pub fn set_channel(
        &self,
        channel: Option<C>,
    ) -> Result<(), PoisonError<RwLockWriteGuard<Option<C>>>> {
        *self.dash_channel.write()? = channel;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::Value;

    use super::*;

    /// Helper function to test that the serialized result is the same as the expected result,
    /// independent of whitespace or key ordering.
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
                "values": [
                    false,
                    true,
                    false
                ]
            }"#,
            &Message::DriverValue {
                values: &[false, true, false],
            },
        );
    }
}
