//! Functions for handling incoming messages to the controller from the
//! dashboard.
use serde_json::Value;
use std::{
    io::Read,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A parsed command received from the controller, which is now ready to be
/// executed.
pub enum Command {
    /// The dashboard requested to know if the controller is ready to begin.
    Ready,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
/// The ways in which parsing an incoming command can fail.
pub enum ParseError {
    /// The source channel closed unexpectedly.
    SourceClosed,
    /// The message was malformed or illegal JSON.
    /// The value inside this variant is the sequence of bytes which contained
    /// the malformed message.
    Malformed(Vec<u8>),
    /// We received an unknown or unsupported command.
    /// The string field is the name of the command we were asked to handle.
    UnknownCommand(String),
    /// Other
    Other,
    /// There was an I/O error in parsing the message.
    Io(std::io::ErrorKind),
}

impl From<std::io::Error> for ParseError {
    /// Construct an `Io` variant of `ParseError`. This allows convenient use of
    /// the question mark operator `?` for bubbling up errors.
    fn from(err: std::io::Error) -> Self {
        ParseError::Io(err.kind())
    }
}

impl Command {
    /// Parse an incoming stream and extract the next command.
    /// In the `Ok()` case, this will return a pair containing the command and
    /// the instant that the command was sent.
    ///
    /// # Errors
    ///
    /// This function will return an `Err` in the cases described in
    /// `ParseError`.
    ///
    /// # Panics
    ///
    /// This function will only panic in case of an internal logic error.
    pub fn parse(src: &mut dyn Read) -> Result<(Command, SystemTime), ParseError> {
        let mut buffer = Vec::new();
        let mut bytes = src.bytes();
        let mut depth = 0;
        // whether we are inside of a string literal
        let mut in_string = false;
        // whether the previous character was the escape character `\`
        let mut escaped = false;
        loop {
            let c = bytes.next().ok_or(ParseError::SourceClosed)??;
            buffer.push(c);
            match c {
                b'{' => {
                    if !in_string {
                        depth += 1;
                    }
                }
                b'}' => {
                    if !in_string {
                        if depth == 0 {
                            // prevent underflow in the case of a message
                            // starting with closing brace
                            return Err(ParseError::Malformed(buffer.clone()));
                        }
                        depth -= 1;
                        // check if this is the end of the outermost object
                        if depth == 0 {
                            break;
                        }
                    }
                }
                // if we encounter an unescaped quote, toggle whether we are in a string
                b'"' => in_string ^= !escaped,
                _ => (),
            };
            escaped = c == b'\\' && !escaped;
        }

        // convert our byte buffer to a UTF-8 string, returning an error if we
        // fail
        let data =
            std::str::from_utf8(&buffer).map_err(|_| ParseError::Malformed(buffer.clone()))?;

        // closure which will return an error and the buffer, used for error
        // handling
        let ret_malformed_opt = || ParseError::Malformed(buffer.clone());
        let serde_value = serde_json::from_str::<Value>(data)
            .map_err(|_| ParseError::Malformed(buffer.clone()))?;
        let json_obj = serde_value.as_object().ok_or_else(ret_malformed_opt)?;
        // we successfully extracted an object
        // retrieve send time of command
        let send_time_ms = json_obj
            .get("send_time")
            .ok_or_else(ret_malformed_opt)?
            .as_u64()
            .ok_or_else(ret_malformed_opt)?;
        let send_time = UNIX_EPOCH + Duration::from_micros(send_time_ms * 1000);

        // now try to extract the name of the command being requested
        let cmd_name = json_obj
            .get("message_type")
            .ok_or_else(ret_malformed_opt)?
            .as_str()
            .ok_or_else(ret_malformed_opt)?;

        let cmd = match cmd_name {
            "ready" => Command::Ready,
            // TODO handle cases of other commands here
            _ => return Err(ParseError::UnknownCommand(cmd_name.into())),
        };

        Ok((cmd, send_time))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Helper function to construct cursors and save some boilerplate on other
    /// tests.
    /// Creates a cursor of `message` and uses it to call `Command::parse`.
    /// Ignores the extracted time from the parser.
    fn parse_helper(message: &str) -> Result<Command, ParseError> {
        let mut cursor = Cursor::new(message);
        Command::parse(&mut cursor).map(|(cmd, _)| cmd)
    }

    #[test]
    /// Test that a `Ready` command is correctly parsed.
    fn ready() {
        let message = r#"{
            "message_type": "ready",
            "send_time": 1651355351791
        }"#;
        assert_eq!(parse_helper(message), Ok(Command::Ready));
    }

    #[test]
    /// Test that a command with a bad identifier cannot be parsed.
    fn bad_command() {
        let message = r#"{
            "message_type": "GARBAGE",
            "send_time": 1651355351791
        }"#;
        assert_eq!(
            parse_helper(message),
            Err(ParseError::UnknownCommand("GARBAGE".into()))
        );
    }

    #[test]
    /// Test that a loose closing brace will cause an error.
    fn extraneous_closing_brace() {
        assert_eq!(parse_helper("}{}"), Err(ParseError::Malformed(vec![b'}'])));
    }
}
