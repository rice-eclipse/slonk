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

use std::{fmt::Display, io::Write, sync::Mutex, time::SystemTime};

/// A log for data displayed to the user.
/// The data sent to the user log need not be machine-readable.
/// The user log will handle saving this data and annotating it with timestamps.
pub struct UserLog<W: Write> {
    /// The buffer to which user log information will be written.
    log_buffer: Mutex<W>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// The levels for logging.
enum LogLevel {
    /// The lowest log level.
    /// Used exclusively for displaying random garbage to help the developer debug their problems.
    Debug = 0,
    /// The second-lowest log level.
    /// Used for information which might be useful to have after an event occurs.
    Info = 1,
    /// The second-highest log level.
    /// Used for notifying the user of potential problems, but which are nonfatal.
    Warn = 2,
    /// The highest log level.
    /// Used for notifying the user of absolutely critical information which is fatal to the system.
    Critical = 3,
}

impl Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LogLevel::Debug => "DEBUG",
                LogLevel::Info => "INFO",
                LogLevel::Warn => "WARN",
                LogLevel::Critical => "CRITICAL",
            }
        )
    }
}

impl<W: Write> UserLog<W> {
    /// Construct a new `UserLog`.
    ///
    /// Information written to the log will be copied over to `buf` as well.
    pub fn new(buf: W) -> UserLog<W> {
        UserLog {
            log_buffer: Mutex::new(buf),
        }
    }

    #[allow(clippy::missing_errors_doc)]
    /// Log some debug information for the user.
    ///
    /// This information should be unimportant for most users.
    pub fn debug(&self, string: &str) -> std::io::Result<()> {
        self.write(LogLevel::Debug, string)
    }

    #[allow(clippy::missing_errors_doc)]
    /// Log some information for the user.
    pub fn info(&self, string: &str) -> std::io::Result<()> {
        self.write(LogLevel::Info, string)
    }

    #[allow(clippy::missing_errors_doc)]
    /// Write a warning for the user.
    ///
    /// Warnings ought to be non-fatal, but could cause an error in the future.
    pub fn warn(&self, string: &str) -> std::io::Result<()> {
        self.write(LogLevel::Warn, string)
    }

    #[allow(clippy::missing_errors_doc)]
    /// Log critical information to the user.
    pub fn critical(&self, string: &str) -> std::io::Result<()> {
        self.write(LogLevel::Critical, string)
    }

    /// Log some information.
    ///
    /// # Inputs
    ///
    /// * `level`: The level of the log.
    ///     Higher-level logs are more critical.
    /// * `string`: The information to log.
    ///     I recommend using `format!()` to construct this string.
    ///
    /// # Errors
    ///
    /// This function will return an `Error` if we are unable to write to the log buffer.
    ///
    /// # Panics
    ///
    /// This function will panic if the current time is before the UNIX epoch.
    fn write(&self, level: LogLevel, string: &str) -> std::io::Result<()> {
        // we trust that this code was run after January 1st, 1970
        let log_time_nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        // use terminal text control characters to change colors
        match level {
            LogLevel::Critical => print!("\x1b[31m"), // red
            LogLevel::Warn => print!("\x1b[33m"),     // yellow
            LogLevel::Info => (),
            LogLevel::Debug => print!("\x1b[90m"), // faded
        };
        println!("[{log_time_nanos}] [{level}] {string}");

        // wipe previous coloring
        print!("\x1b[0m");
        writeln!(
            // we trust writing to the log buffer will not cause a panic.
            self.log_buffer.lock().unwrap(),
            "[{log_time_nanos}] [{level}] {string}"
        )?;
        Ok(())
    }
}
