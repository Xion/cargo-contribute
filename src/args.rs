//! Module for handling command line arguments.

use std::env;
use std::error::Error;
use std::fmt;
use std::ffi::OsString;
use std::iter::IntoIterator;
use std::num::ParseIntError;
use std::path::PathBuf;

use clap::{self, AppSettings, Arg, ArgMatches};
use conv::TryFrom;

use super::{NAME, VERSION};


// Parse command line arguments and return `Options` object.
#[inline]
pub fn parse() -> Result<Options, ArgsError> {
    parse_from_argv(env::args_os())
}

/// Parse application options from given array of arguments
/// (*all* arguments, including binary name).
#[inline]
pub fn parse_from_argv<I, T>(argv: I) -> Result<Options, ArgsError>
    where I: IntoIterator<Item=T>, T: Clone + Into<OsString>
{
    let parser = create_parser();
    let matches = try!(parser.get_matches_from_safe(argv));
    Options::try_from(matches)
}


/// Structure to hold options received from the command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Verbosity of the logging output.
    ///
    /// Corresponds to the number of times the -v flag has been passed.
    /// If -q has been used instead, this will be negative.
    pub verbosity: isize,

    /// Path to a crate manifest (Cargo.toml) to look at for [dependencies].
    /// If omitted, we'll try to use one in the current directory.
    pub manifest_path: Option<PathBuf>,
    /// Maximum number of issues to yield.
    /// If omitted, we'll keep searching for more indefinitely.
    pub count: Option<usize>,
    /// Optional GitHub personal access token to use for authentication.
    pub github_token: Option<String>,
}

#[allow(dead_code)]
impl Options {
    #[inline]
    pub fn verbose(&self) -> bool { self.verbosity > 0 }
    #[inline]
    pub fn quiet(&self) -> bool { self.verbosity < 0 }
}

impl<'a> TryFrom<ArgMatches<'a>> for Options {
    type Err = ArgsError;

    fn try_from(matches: ArgMatches<'a>) -> Result<Self, Self::Err> {
        let verbose_count = matches.occurrences_of(OPT_VERBOSE) as isize;
        let quiet_count = matches.occurrences_of(OPT_QUIET) as isize;
        let verbosity = verbose_count - quiet_count;

        let manifest_path = matches.value_of(OPT_MANIFEST_PATH).map(PathBuf::from);
        let count = match matches.value_of(OPT_COUNT) {
            Some(c) => Some(c.parse()?),
            None => None,
        };
        let github_token = matches.value_of(OPT_GITHUB_TOKEN).map(String::from);

        Ok(Options{verbosity, manifest_path, count, github_token})
    }
}

macro_attr! {
    /// Error that can occur while parsing of command line arguments.
    #[derive(Debug, EnumFromInner!)]
    pub enum ArgsError {
        /// General when parsing the arguments.
        Parse(clap::Error),
        /// Error when parsing --count flag.
        Count(ParseIntError),
    }
}
impl Error for ArgsError {
    fn description(&self) -> &str { "failed to parse argv" }
    fn cause(&self) -> Option<&Error> {
        match self {
            &ArgsError::Parse(ref e) => Some(e),
            &ArgsError::Count(ref e) => Some(e),
        }
    }
}
impl fmt::Display for ArgsError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &ArgsError::Parse(ref e) => write!(fmt, "parse error: {}", e),
            &ArgsError::Count(ref e) => write!(fmt, "invalid --count value: {}", e),
        }
    }
}


// Parser configuration

/// Type of the argument parser object
/// (which is called an "App" in clap's silly nomenclature).
type Parser<'p> = clap::App<'p, 'p>;


lazy_static! {
    static ref ABOUT: &'static str = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("");
}

const OPT_MANIFEST_PATH: &'static str = "manifest-path";
const OPT_COUNT: &'static str = "count";
const OPT_GITHUB_TOKEN: &'static str = "github-token";
const OPT_VERBOSE: &'static str = "verbose";
const OPT_QUIET: &'static str = "quiet";

/// Create the parser for application's command line.
fn create_parser<'p>() -> Parser<'p> {
    let mut parser = Parser::new(*NAME);
    if let Some(version) = *VERSION {
        parser = parser.version(version);
    }
    parser
        .bin_name("cargo contribute")
        .about(*ABOUT)
        .author(crate_authors!(", "))

        .setting(AppSettings::StrictUtf8)

        .setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::ColorNever)

        .arg(Arg::with_name(OPT_MANIFEST_PATH)
            .long("manifest-path")
            .takes_value(true)
            .multiple(false)
            .value_name("PATH")
            .help("Path to a crate manifest to look through"))

        // TODO: make the default more conservative because GitHub rate limits hard
        .arg(Arg::with_name(OPT_COUNT)
            .long("count").short("n")
            .takes_value(true)
            .multiple(false)
            .value_name("N")
            .help("Maximum number of suggested issues to yield"))

        .arg(Arg::with_name(OPT_GITHUB_TOKEN)
            .long("token").long("github-token")
            .takes_value(true)
            .multiple(false)
            .value_name("TOKEN")
            .help("GitHub's personal access token to use")
            .long_help(concat!(
                "You can provide a personal access token generated using ",
                "https://github.com/settings/tokens.",
                "This helps avoiding rate limit problems when searching for ",
                "issues to contribute to.")))

        // Verbosity flags.
        .arg(Arg::with_name(OPT_VERBOSE)
            .long("verbose").short("v")
            .multiple(true)
            .conflicts_with(OPT_QUIET)
            .help("Increase logging verbosity"))
        .arg(Arg::with_name(OPT_QUIET)
            .long("quiet").short("q")
            .multiple(true)
            .conflicts_with(OPT_VERBOSE)
            .help("Decrease logging verbosity"))

        .help_short("H")
        .version_short("V")
}
