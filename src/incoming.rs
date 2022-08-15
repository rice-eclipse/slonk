//! Functions for handling incoming messages to the controller from the
//! dashboard.
use serde_json::Value;
use std::io::Read;

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
    Malformed,
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
    ///
    /// # Errors
    ///
    /// This function will return an `Err` in the cases described in
    /// `ParseError`.
    ///
    /// # Panics
    ///
    /// This function will only panic in case of an internal logic error.
    pub fn parse(src: &mut dyn Read) -> Result<Command, ParseError> {
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
                            return Err(ParseError::Malformed);
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
        let data = String::from_utf8(buffer).map_err(|_| ParseError::Malformed)?;
        if let Ok(Value::Object(obj)) = serde_json::from_str(&data) {
            // we successfully extracted an object
            // now try to extract the name of the command being requested
            if let Some(Value::String(cmd_name)) = obj.get("message_type") {
                match cmd_name.as_str() {
                    "ready" => Ok(Command::Ready),
                    // TODO handle cases of other commands here
                    _ => Err(ParseError::UnknownCommand(cmd_name.clone())),
                }
            } else {
                Err(ParseError::Malformed)
            }
        } else {
            // whatever we parsed, it was not a JSON object. must have been
            // malformed
            Err(ParseError::Malformed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Helper function to construct cursors and save some boilerplate on other
    /// tests.
    /// Creates a cursor of `message` and uses it to call `Command::parse`.
    fn parse_helper(message: &str) -> Result<Command, ParseError> {
        let mut cursor = Cursor::new(message);
        Command::parse(&mut cursor)
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
        assert_eq!(parse_helper("}{}"), Err(ParseError::Malformed));
    }
}
