//! Module implementing logging for the application.
//!
//! This includes setting up log filtering given a verbosity value,
//! as well as defining how the logs are being formatted to stderr.

use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::io;

use ansi_term::{Colour, Style};
use isatty;
use log::SetLoggerError;
use slog::{self, DrainExt, FilterLevel, Level};
use slog_envlogger::LogBuilder;
use slog_stdlog;
use slog_stream;
use time;


// Default logging level defined using the two enums used by slog.
// Both values must correspond to the same level. (This is checked by a test).
const DEFAULT_LEVEL: Level = Level::Info;
const DEFAULT_FILTER_LEVEL: FilterLevel = FilterLevel::Info;

// Arrays of log levels, indexed by verbosity.
const POSITIVE_VERBOSITY_LEVELS: &'static [FilterLevel] = &[
    DEFAULT_FILTER_LEVEL,
    FilterLevel::Debug,
    FilterLevel::Trace,
];
const NEGATIVE_VERBOSITY_LEVELS: &'static [FilterLevel] = &[
    DEFAULT_FILTER_LEVEL,
    FilterLevel::Warning,
    FilterLevel::Error,
    FilterLevel::Critical,
    FilterLevel::Off,
];


/// Initialize logging with given verbosity.
/// The verbosity value has the same meaning as in args::Options::verbosity.
pub fn init(verbosity: isize) -> Result<(), SetLoggerError> {
    let istty = cfg!(unix) && isatty::stderr_isatty();
    let stderr = slog_stream::stream(io::stderr(), LogFormat{tty: istty});

    // Determine the log filtering level based on verbosity.
    // If the argument is excessive, log that but clamp to the highest/lowest log level.
    let mut verbosity = verbosity;
    let mut excessive = false;
    let level = if verbosity >= 0 {
        if verbosity >= POSITIVE_VERBOSITY_LEVELS.len() as isize {
            excessive = true;
            verbosity = POSITIVE_VERBOSITY_LEVELS.len() as isize - 1;
        }
        POSITIVE_VERBOSITY_LEVELS[verbosity as usize]
    } else {
        verbosity = -verbosity;
        if verbosity >= NEGATIVE_VERBOSITY_LEVELS.len() as isize {
            excessive = true;
            verbosity = NEGATIVE_VERBOSITY_LEVELS.len() as isize - 1;
        }
        NEGATIVE_VERBOSITY_LEVELS[verbosity as usize]
    };

    // Include universal logger options, like the level.
    let mut builder = LogBuilder::new(stderr);
    builder = builder.filter(None, level);

    // Make some of the libraries less chatty
    // by raising the minimum logging level for them
    // (e.g. Info means that Debug and Trace level logs are filtered).
    builder = builder
        .filter(Some("hyper"), FilterLevel::Info)
        .filter(Some("tokio"), FilterLevel::Info);

    // Include any additional config from environmental variables.
    // This will override the options above if necessary,
    // so e.g. it is still possible to get full debug output from hyper/tokio.
    if let Ok(ref conf) = env::var("RUST_LOG") {
        builder = builder.parse(conf);
    }

    // Initialize the logger, possibly logging the excessive verbosity option.
    let env_logger_drain = builder.build();
    let logger = slog::Logger::root(env_logger_drain.fuse(), o!());
    try!(slog_stdlog::set_logger(logger));
    if excessive {
        warn!("-v/-q flag passed too many times, logging level {:?} assumed", level);
    }
    Ok(())
}


// Log formatting

/// Token type that's only uses to tell slog-stream how to format our log entries.
struct LogFormat {
    pub tty: bool,
}

impl slog_stream::Format for LogFormat {
    /// Format a single log Record and write it to given output.
    fn format(&self, output: &mut io::Write,
              record: &slog::Record,
              _logger_kvp: &slog::OwnedKeyValueList) -> io::Result<()> {
        // Format the higher level (more fine-grained) messages with greater detail,
        // as they are only visible when user explicitly enables verbose logging.
        let msg = if record.level() > DEFAULT_LEVEL {
            let logtime = format_log_time();
            let level: String = {
                let first_char = record.level().as_str().chars().next().unwrap();
                first_char.to_uppercase().collect()
            };
            let module = {
                let module = record.module();
                match module.find("::") {
                    Some(idx) => Cow::Borrowed(&module[idx + 2..]),
                    None => "main".into(),
                }
            };
            // Dim the prefix (everything that's not a message) if we're outputting to a TTY.
            let prefix_style = if self.tty { *TTY_FINE_PREFIX_STYLE } else { Style::default() };
            let prefix = format!("{}{} {}#{}]", level, logtime, module, record.line());
            format!("{} {}\n", prefix_style.paint(prefix), record.msg())
        } else {
            // Colorize the level label if we're outputting to a TTY.
            let level: Cow<str> = if self.tty {
                let style = TTY_LEVEL_STYLES.get(&record.level().as_usize())
                    .cloned()
                    .unwrap_or_else(Style::default);
                format!("{}", style.paint(record.level().as_str())).into()
            } else {
                record.level().as_str().into()
            };
            format!("{}: {}\n", level, record.msg())
        };

        try!(output.write_all(msg.as_bytes()));
        Ok(())
    }
}

/// Format the timestamp part of a detailed log entry.
fn format_log_time() -> String {
    let utc_now = time::now().to_utc();
    let mut logtime = format!("{}", utc_now.rfc3339());  // E.g.: 2012-02-22T14:53:18Z

    // Insert millisecond count before the Z.
    let millis = utc_now.tm_nsec / NANOS_IN_MILLISEC;
    logtime.pop();
    format!("{}.{:04}Z", logtime, millis)
}

const NANOS_IN_MILLISEC: i32 = 1000000;

lazy_static! {
    /// Map of log levels to their ANSI terminal styles.
    // (Level doesn't implement Hash so it has to be usize).
    static ref TTY_LEVEL_STYLES: HashMap<usize, Style> = hashmap!{
        Level::Info.as_usize() => Colour::Green.normal(),
        Level::Warning.as_usize() => Colour::Yellow.normal(),
        Level::Error.as_usize() => Colour::Red.normal(),
        Level::Critical.as_usize() => Colour::Purple.normal(),
    };

    /// ANSI terminal style for the prefix (timestamp etc.) of a fine log message.
    static ref TTY_FINE_PREFIX_STYLE: Style = Style::new().dimmed();
}


#[cfg(test)]
mod tests {
    use slog::FilterLevel;
    use super::{DEFAULT_LEVEL, DEFAULT_FILTER_LEVEL,
                NEGATIVE_VERBOSITY_LEVELS, POSITIVE_VERBOSITY_LEVELS};

    /// Check that default logging level is defined consistently.
    #[test]
    fn default_level() {
        let level = DEFAULT_LEVEL.as_usize();
        let filter_level = DEFAULT_FILTER_LEVEL.as_usize();
        assert_eq!(level, filter_level,
            "Default logging level is defined inconsistently: Level::{:?} vs. FilterLevel::{:?}",
            DEFAULT_LEVEL, DEFAULT_FILTER_LEVEL);
    }

    #[test]
    fn verbosity_levels() {
        assert_eq!(NEGATIVE_VERBOSITY_LEVELS[0], POSITIVE_VERBOSITY_LEVELS[0]);
        assert!(NEGATIVE_VERBOSITY_LEVELS.contains(&FilterLevel::Off),
            "Verbosity levels don't allow to turn logging off completely");
    }
}
