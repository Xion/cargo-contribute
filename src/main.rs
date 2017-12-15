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
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate maplit;
#[macro_use] extern crate macro_attr;
             extern crate rand;
             extern crate serde;
#[macro_use] extern crate serde_derive;
             extern crate serde_json;
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
use std::error::Error;
use std::path::Path;
use std::process::exit;

use futures::Stream;
use log::LogLevel::*;
use tokio_core::reactor::Core;

use args::ArgsError;
use issues::SuggestedIssuesProducer;


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

    let manifest_path = opts.manifest_path.as_ref()
        .map(|p| p as &Path).unwrap_or(Path::new("./Cargo.toml"));
    if !manifest_path.is_file() {
        error!("Couldn't find crate manifest {}.",
            match opts.manifest_path {
                Some(ref path) => format!("under {}", path.display()),
                None => format!("; make sure you're in the crate root directory."),
            });
        exit(exitcode::USAGE);
    }

    let mut core = Core::new().unwrap_or_else(|e| {
        error!("Failed to initialize Tokio core: {}", e);
        exit(exitcode::TEMPFAIL);
    });

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

    core.run(
        issues.for_each(|issue| {
            println!("[{}/{}] #{}: {}",
                issue.repo.owner, issue.repo.name, issue.number, issue.title);
            Ok(())
        })
    ).unwrap();
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
