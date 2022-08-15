//! Functions for handling incoming messages to the controller from the
//! dashboard.
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A parsed command received from the controller, which is now ready to be
/// executed.
pub enum Command {
    /// The dashboard requested to know if the controller is ready to begin.
    /// controller is ready to begin
    Ready,
    /// controller to standby
    Standby,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The ways in which parsing an incoming command can fail.
pub enum ParseError {
    /// The source channel closed unexpectedly.
    SourceClosed,
    /// The message was malformed or illegal JSON.
    Malformed,
    /// Unknown command received
    UnknownCommand,
    /// Other
    Other,
    /// There was an I/O error in parsing the message.
    Io(std::io::ErrorKind),
}
#[derive(Serialize, Deserialize)]
struct DriverCommand {
    driver_cmd: String,
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
    /// This function will return an `Err` if the incoming message cannot be
    /// parsed.
    ///
    /// # Panics
    ///
    /// This function will only panic in case of an internal logic error.
    pub fn parse(src: &mut dyn Read) -> Result<Command, ParseError> {
        let mut buffer = Vec::new(); // size?
        let mut _bytes = src.bytes();
        let mut depth = 0;
        // whether we are inside of a string literal
        let mut in_string = false;
        // whether the previous character was the escape character `\`
        let mut escaped = false;
        loop {
            let c = _bytes.next().ok_or(ParseError::SourceClosed)??;
            buffer.push(c);
            match c {
                b'{' => {
                    if !in_string {
                        depth += 1
                    }
                }
                b'}' => {
                    if !in_string {
                        depth -= 1;
                        // check if this is the end of the outermost object
                        if depth == 0 {
                            break;
                        }
                    }
                }
                // if we encounter an unescaped backslash, toggle whether we are in a string
                b'"' => in_string ^= !escaped,
                _ => (),
            };
            escaped = c == b'\\' && !escaped;
        }

        let data = String::from_utf8_lossy(&buffer);
        let cmd: Result<String, _> = serde_json::from_str(&data);
        match cmd {
            Ok(s) => match s.as_str() {
                "Ready" => return Ok(Command::Ready),
                "Standby" => return Ok(Command::Standby),
                _ => return Err(ParseError::UnknownCommand),
            },
            Err(_) => return Err(ParseError::UnknownCommand),
        }
    }
}
#[test]
fn test_ready_command() {
    let mut cursor = Cursor::new(
        r#"{
    "message_type": "Ready",
    "send_time": 1651355351791
}"#,
    );
    assert_eq!(Command::parse(&mut cursor), Ok(Command::Ready));
}
