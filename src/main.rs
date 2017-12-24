//!
//! cargo-contribute
//!

             extern crate ansi_term;
             extern crate chrono;
#[macro_use] extern crate clap;
             extern crate conv;
#[macro_use] extern crate derive_error;
#[macro_use] extern crate enum_derive;
             extern crate exitcode;
             extern crate futures;
             extern crate hubcaps;
             extern crate hyper;
             extern crate hyper_tls;
             extern crate isatty;
             extern crate itertools;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate maplit;
#[macro_use] extern crate macro_attr;
             extern crate rand;
             extern crate regex;
             extern crate serde;
#[macro_use] extern crate serde_derive;
             extern crate serde_json;
             extern crate strfmt;
             extern crate slog_envlogger;
             extern crate slog_stdlog;
             extern crate slog_stream;
             extern crate tokio_core;
             extern crate toml;
             extern crate url;

// `slog` must precede `log` in declarations here, because we want to simultaneously:
// * use the standard `log` macros
// * be able to initialize the slog logger using slog macros like o!()
#[macro_use] extern crate slog;
#[macro_use] extern crate log;


mod args;
mod ext;
mod issues;
mod logging;
mod util;


use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::path::Path;
use std::process::exit;

use futures::Stream;
use log::LogLevel::*;
use strfmt::{FmtError, strfmt};
use tokio_core::reactor::Core;

use args::{ArgsError, Options};
use issues::{Issue, SuggestedIssuesProducer};


lazy_static! {
    /// Application / package name, as filled out by Cargo.
    static ref NAME: &'static str = option_env!("CARGO_PKG_NAME")
        .unwrap_or("cargo-contribute");

    /// Application version, as filled out by Cargo.
    static ref VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
}

lazy_static! {
    /// User-Agent header that the program uses for all outgoing HTTP requests.
    static ref USER_AGENT: Cow<'static, str> = match *VERSION {
        Some(version) => Cow::Owned(format!("{}/{}", *NAME, version)),
        None => Cow::Borrowed(*NAME),
    };
}


fn main() {
    let opts = args::parse().unwrap_or_else(|e| {
        print_args_error(e);
        exit(exitcode::USAGE);
    });

    logging::init(opts.verbosity).unwrap();
    log_signature();

    let mut core = Core::new().unwrap_or_else(|e| {
        error!("Failed to initialize Tokio core: {}", e);
        exit(exitcode::TEMPFAIL);
    });
    suggest_contributions(&mut core, &opts);
}

// Print an error that may occur while parsing arguments.
fn print_args_error(e: ArgsError) {
    match e {
        ArgsError::Parse(ref e) => {
            // In case of generic parse error,
            // message provided by the clap library will be the usage string.
            eprintln!("{}", e.message);
        }
        e => {
            eprintln!("Failed to parse arguments: {}",
                e.cause().map(|c| format!("{}", c)).unwrap_or_else(|| "<unknown error>".into()));
        }
    }
}

/// Log the program name, version, and other metadata.
#[inline]
fn log_signature() {
    if log_enabled!(Info) {
        let version = VERSION.map(|v| format!("v{}", v))
            .unwrap_or_else(|| "<UNKNOWN VERSION>".into());
        info!("{} {}", *NAME, version);
    }
}


/// Actual entry point of the program.
///
/// Suggest issues to contribute to based on given command line options,
/// and print them to stdout.
fn suggest_contributions(core: &mut Core, opts: &Options) -> ! {
    let manifest_path = opts.manifest_path.as_ref()
        .map(|p| p as &Path).unwrap_or(Path::new("./Cargo.toml"));
    if !manifest_path.is_file() {
        error!("Couldn't find crate manifest {}.",
            match opts.manifest_path {
                Some(ref path) => format!("under {}", path.display()),
                None => format!("; make sure you're in the crate root directory."),
            });
        exit(exitcode::DATAERR);
    }

    // TODO: consider doing the OAuth flow via a browser and saving the access token+secret
    // as another mode of authentication
    let producer = match opts.github_token {
        Some(ref t) => SuggestedIssuesProducer::with_github_token(t, &core.handle()),
        None => SuggestedIssuesProducer::new(&core.handle()),
    };
    let mut issues = producer.suggest_issues(manifest_path).unwrap_or_else(|e| {
        error!("Failed to suggest issues: {}", e);
        exit(exitcode::IOERR);
    });
    if let Some(count) = opts.count {
        issues = Box::new(issues.take(count as u64));
    }

    let mut found = false;
    core.run(
        issues.from_err().for_each(|issue| {
            found = true;
            print_issue(opts.format.as_ref().map(|f| f.as_str()), &issue)
        })
    ).unwrap_or_else(|e| {
        error!("Suggesting issues failed with an error: {:?}", e);
        exit(exitcode::TEMPFAIL);
    });
    if !found {
        info!("No suitable issues to contribute to :-(");
    }

    exit(exitcode::OK)
}

/// Print a single issue to standard output.
fn print_issue(fmt: Option<&str>, issue: &Issue) -> Result<(), Box<Error>> {
    match fmt {
        Some(f) => println!("{}", format_issue(f, issue)?),
        None => println!("{} -- {}", issue, issue.url),
    }
    Ok(())
}

/// Format an issue according to user-provided format.
fn format_issue(fmt: &str, issue: &Issue) -> Result<String, FmtError> {
    let params: HashMap<String, _> = ISSUE_FORMATTERS.iter()
        .map(|(&p, &f)| (p.to_owned(), f(issue)))
        .collect();
    let line = strfmt(fmt, &params)?;
    Ok(line)
}

lazy_static! {
    static ref ISSUE_FORMATTERS: HashMap<&'static str, fn(&Issue) -> Cow<str>> = hashmap!{
        "owner" => (|issue| issue.repo.owner.as_str().into()) as fn(&Issue) -> Cow<str>,
        "project" => |issue| issue.repo.name.as_str().into(),
        "repo" => |issue| format!("{}", issue.repo).into(),
        "number" => |issue| format!("{}", issue.number).into(),
        "url" => |issue| issue.url.as_str().into(),
    };
}
