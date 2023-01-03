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

//! Functions for handling incoming messages to the controller from the dashboard.

use serde::Deserialize;
use std::{fmt::Display, io::Read};

#[non_exhaustive]
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type")]
/// A parsed command received from the controller, which is now ready to be executed.
pub enum Command {
    /// The dashboard requested that the driver be actuated to a logic level.
    Actuate {
        /// A string identifying the driver.
        /// The controller must verify that this string is a real driver.
        driver_id: u8,
        /// The logic level that the driver must be actuated to.
        /// `true` corresponds to powered (i.e. connected to 12V), while `false` corresponds to
        /// unpowered (high-Z connection or grounding; hardware-decided).
        value: bool,
    },
    /// The dashboard requested to begin an ignition procedure immediately.
    Ignition,
    /// The dashboard requested to begin an emergency stop immediately.
    EmergencyStop,
}

#[non_exhaustive]
#[derive(Debug)]
/// The ways in which parsing an incoming command can fail.
pub enum Error {
    /// The message was malformed or illegal JSON.
    /// The value inside this variant is the sequence of bytes which contained the malformed
    /// message.
    Malformed(Vec<u8>),
    /// There was an I/O error in parsing the message.
    Io(std::io::Error),
}

impl From<std::io::Error> for Error {
    /// Construct an `Io` variant of `ParseError`.
    /// This allows convenient use of the question mark operator `?` for bubbling up errors.
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

impl Command {
    /// Parse an incoming stream and extract the next command.
    /// In the `Ok()` case, this will return a pair containing the command and the instant that the
    /// command was sent.
    ///
    /// # Errors
    ///
    /// This function will return an `Err` in the cases described in `ParseError`.
    ///
    /// # Panics
    ///
    /// This function will only panic in case of an internal logic error.
    pub fn parse(src: &mut dyn Read) -> Result<Command, Error> {
        let mut buffer = Vec::new();
        let mut bytes = src.bytes();
        let mut depth = 0;
        // whether we are inside of a string literal
        let mut in_string = false;
        // whether the previous character was the escape character `\`
        let mut escaped = false;
        loop {
            let c = bytes.next().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "require more characters to fill out JSON body",
                )
            })??;
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
                            // prevent underflow in the case of a message starting with closing
                            // brace
                            return Err(Error::Malformed(buffer));
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

        let result = serde_json::from_slice(&buffer);
        let cmd = result.map_err(|_| Error::Malformed(buffer))?;

        Ok(cmd)
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Actuate { driver_id, value } => write!(f, "actuate {driver_id} {value}"),
            Command::Ignition => write!(f, "ignition"),
            Command::EmergencyStop => write!(f, "estop"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Helper function to construct cursors and save some boilerplate on other tests.
    /// Creates a cursor of `message` and uses it to call `Command::parse`.
    /// Ignores the extracted time from the parser.
    fn parse_helper(message: &str) -> Result<Command, Error> {
        let mut cursor = Cursor::new(message);
        Command::parse(&mut cursor)
    }

    #[test]
    /// Test that a command with a bad identifier cannot be parsed.
    fn bad_command() {
        let message = r#"{
            "type": "GARBAGE"
        }"#;

        let Err(Error::Malformed(s)) = parse_helper(message) else {panic!()};
        let slice: &[u8] = message.as_ref();

        assert_eq!(&s, slice);
    }

    #[test]
    /// Test that an `actuate` command is parsed correctly.
    fn actuate() {
        let message = r#"{
            "type": "Actuate",
            "driver_id": 0,
            "value": true
        }"#;
        assert_eq!(
            parse_helper(message).unwrap(),
            Command::Actuate {
                driver_id: 0,
                value: true
            }
        );
    }

    #[test]
    /// Test that an ignition command is parsed correctly.
    fn ignition() {
        let message = r#"{
            "type": "Ignition"
        }"#;
        assert_eq!(parse_helper(message).unwrap(), Command::Ignition);
    }

    #[test]
    /// Test that an emergency stop command is parsed correctly.
    fn estop() {
        let message = r#"{
            "type": "EmergencyStop"
        }"#;
        assert_eq!(parse_helper(message).unwrap(), Command::EmergencyStop);
    }
}
