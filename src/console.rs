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
pub enum LogLevel {
    /// The lowest log level.
    /// Used exclusively for displaying random garbage to help the developer
    /// debug their problems.
    Debug = 0,
    /// The second-lowest log level.
    /// Used for information which might be useful to have after an event
    /// occurs.
    Info = 1,
    /// The second-highest log level.
    /// Used for notifying the user of potential problems, but which are
    /// nonfatal.
    Warn = 2,
    /// The highest log level.
    /// Used for notifying the user of absolutely critical information which is
    /// fatal to the system.
    /// Critical messages will be written to stderr.
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
    ///
    pub fn new(buf: W) -> UserLog<W> {
        UserLog {
            log_buffer: Mutex::new(buf),
        }
    }

    /// Log some information.
    ///
    /// # Inputs
    ///
    /// * `level`: The level of the log.
    ///     Higher-level logs are more critical.
    ///     If the level `LogLevel::CRITICAL` is passed, this log will be
    ///     written to stderr.
    /// * `string`: The information to log.
    ///     I recommend using `format!()` to construct this string.
    ///
    /// # Errors
    ///
    /// This function will return an `Error` if we are unable to write to the
    ///
    /// # Panics
    ///
    /// This function will panic if the current time is before the UNIX epoch.
    pub fn write(&self, level: LogLevel, string: &str) -> std::io::Result<()> {
        // we trust that this code was run after January 1st, 1970
        let log_time_nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        if level == LogLevel::Critical {
            eprintln!("[{log_time_nanos}] [{level}] {string}");
        } else {
            println!("[{log_time_nanos}] [{level}] {string}");
        }
        writeln!(
            // we trust writing to the log buffer will not cause a panic.
            self.log_buffer.lock().unwrap(),
            "[{log_time_nanos}] [{level}] {string}"
        )?;
        Ok(())
    }
}
