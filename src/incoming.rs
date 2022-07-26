//! Functions for handling incoming messages to the controller from the
//! dashboard.

use std::io::Read;

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A parsed command received from the controller, which is now ready to be
/// executed.
pub enum Command {
    /// The dashboard requested to know if the controller is ready to begin.
    Ready,
    /* add other commands below..... */
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The ways in which parsing an incoming command can fail.
pub enum ParseError {
    /// The source channel closed unexpectedly.
    SourceClosed,
    /// The message was malformed or illegal JSON.
    Malformed,
}

impl Command {
    /// Parse an incoming stream and extract the next command.
    ///
    /// # Errors
    ///
    /// This function will return an `Err` if the incoming message cannot be
    /// parsed.
    ///
    /// # Panics
    ///
    /// This function will only panic in case of an internal logic error.
    pub fn parse(src: &mut dyn Read) -> Result<Command, ParseError> {
        let mut _bytes = src.bytes();
        todo!();
    }
}
