//! Module for handling command line arguments.

use std::env;
use std::error::Error;
use std::fmt;
use std::ffi::OsString;
use std::iter::IntoIterator;
use std::mem;
use std::num::ParseIntError;
use std::path::PathBuf;
use std::slice;
use std::str;

use clap::{self, AppSettings, Arg, ArgMatches};
use conv::TryFrom;
use itertools::Itertools;
use strfmt::FmtError;

use model::{Issue, Repository};
use super::{ISSUE_FORMATTERS, NAME, VERSION, format_issue};


// Parse command line arguments and return `Options` object.
#[inline]
pub fn parse() -> Result<Options, ArgsError> {
    parse_from_argv(env::args_os())
}

/// Parse application options from given array of arguments
/// (*all* arguments, including binary name).
#[inline]
pub fn parse_from_argv<I, T>(argv: I) -> Result<Options, ArgsError>
    where I: IntoIterator<Item=T>, T: Clone + Into<OsString> + PartialEq<str>
{
    // Detect `cargo contribute` invocation, and remove the subcommand name.
    let mut argv: Vec<_> = argv.into_iter().collect();
    if argv.len() >= 2 && &argv[1] == "contribute" {
        argv.remove(1);
    }

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
    /// Optional format string to use when printing issues.
    pub format: Option<String>,
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
        let format = matches.value_of(OPT_FORMAT).map(String::from);

        Ok(Options{verbosity, manifest_path, count, github_token, format})
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
const OPT_FORMAT: &'static str = "format";
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
        .author(crate_authors!(", "))
        .about(*ABOUT)
        .long_about(concat!(
            "Look at this crate's [dependencies] and suggest some of their open issues\n",
            "as potential avenues for making contributions (pull requests)."))

        .setting(AppSettings::StrictUtf8)

        .setting(AppSettings::UnifiedHelpMessage)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::ColorNever)

        .arg(Arg::with_name(OPT_MANIFEST_PATH)
            .long("manifest-path")
            .takes_value(true)
            .empty_values(false)
            .multiple(false)
            .value_name("PATH")
            .help("Path to a crate manifest to look through"))

        .arg(Arg::with_name(OPT_COUNT)
            .long("count").short("n")
            .takes_value(true)
            .empty_values(false)
            .validator(validate_count)
            .multiple(false)
            .value_name("N")
            .help("Maximum number of suggested issues to yield")
            .long_help(concat!(
                "How many issues to print in total.\n\n",
                "If omitted, the program will look for all matching issues\n",
                "(which may easily lead to hitting GitHub's rate limits).\n")))

        .arg(Arg::with_name(OPT_GITHUB_TOKEN)
            .long("github-token").alias("token")
            .takes_value(true)
            .empty_values(false)
            .multiple(false)
            .value_name("TOKEN")
            .help("GitHub's personal access token to use")
            .long_help(concat!(
                "Access token to use when querying GitHub API.\n\n",
                "You can provide a personal access token generated using\n",
                "https://github.com/settings/tokens.\n",
                "This helps avoiding rate limit problems when searching for ",
                "issues to contribute to.\n")))

        .arg(Arg::with_name(OPT_FORMAT)
            .long("format")
            .visible_alias("template").short("T")  // inspired by `hg log`
            .takes_value(true)
            .empty_values(true)
            .allow_hyphen_values(true)
            .validator(validate_format)
            .multiple(false)
            .value_name("FORMAT")
            .help("Custom formatting string for printing suggested issues")
            .long_help(leak(format!(concat!(
                "Specify your own formatting string to use when printing suggested issues.\n\n",
                "This string follows the normal Rust syntax from format!() et al.\n",
                "The following issue placeholders are available for use:\n",
                "{}"), ISSUE_FORMATTERS.keys().format_with(", ", |key, f| {
                    f(&format_args!("{{{}}}", key))  // {key}
                })))))

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

/// Validator for the --count flag value.
fn validate_count(count: String) -> Result<(), String> {
    count.parse::<usize>().map(|_| ()).map_err(|e| format!("{}", e))
}

/// Validator for the --format flag value.
fn validate_format(format: String) -> Result<(), String> {
    lazy_static! {
        static ref EXAMPLE_ISSUE: Issue = Issue{
            repo: Repository::new("Octocat", "hello-world"),
            number: 42,
            url: "http://example.com/42".into(),
            title: "Optimize reticulating spines".into(),
            body: "...".into(),
            comment_count: 0,
        };
    }
    format_issue(&format, &*EXAMPLE_ISSUE).map(|_| ()).map_err(|e| match e {
        FmtError::Invalid(msg) => msg,
        FmtError::KeyError(msg) => msg,
        // Other errors shouldn't happen because they would indicate problems
        // with formatting arguments, i.e. a bug in our code.
        e => panic!("Unexpected error when validating --format: {}", e),
    })
}


/// Convert a value to a &'static str by leaking the memory of an owned String.
fn leak<T: ToString>(v: T) -> &'static str {
    let s = v.to_string();
    unsafe {
        let (ptr, len) = (s.as_ptr(), s.len());
        mem::forget(s);
        let bytes: &'static [u8] = slice::from_raw_parts(ptr, len);
        str::from_utf8_unchecked(bytes)
    }
}
