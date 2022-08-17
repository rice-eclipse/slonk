use std::{collections::HashMap, time::Instant};

/// The set of messages which can be sent from the controller to the dashboard.
pub enum Message<'a> {
    /// A configuration message.
    /// The contained value is a JSON string containing the full configuration
    /// file.
    Config(&'a str),
    /// A sensor valuation message.
    /// Each key in the map corresponds to a sensor.
    /// Each value corresponds to a time at which a sensor value was taken and
    /// the ADC value read at that time.
    SensorValue(&'a HashMap<String, (Instant, u16)>),
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
    Malformed(&'a [u8]),
    /// A read from a sensor failed.
    /// Give the ID of the sensor which failed to be read.
    SensorFail(&'a str),
    /// The OS denied permission for some functionality of the controller.
    Permission,
}
