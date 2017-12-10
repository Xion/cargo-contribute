//!
//! cargo-contribute
//!

             extern crate ansi_term;
             extern crate chrono;
#[macro_use] extern crate clap;
             extern crate conv;
#[macro_use] extern crate derive_error;
             extern crate exitcode;
             extern crate futures;
             extern crate hubcaps;
             extern crate hyper;
             extern crate isatty;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate maplit;
             extern crate serde;
#[macro_use] extern crate serde_derive;
             extern crate serde_json;
             extern crate slog_envlogger;
             extern crate slog_stdlog;
             extern crate slog_stream;
             extern crate tokio_core;

// `slog` must precede `log` in declarations here, because we want to simultaneously:
// * use the standard `log` macros
// * be able to initialize the slog logger using slog macros like o!()
#[macro_use] extern crate slog;
#[macro_use] extern crate log;


mod args;
mod crates_io;
mod ext;
mod logging;


use std::borrow::Cow;
use std::process::exit;

use hubcaps::Github;
use log::LogLevel::*;
use tokio_core::reactor::Core;

use args::ArgsError;


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
    let github = Github::new(USER_AGENT.to_owned(), None, &core.handle());
    core.run(futures::future::ok::<(), ()>(())).unwrap();
}

// Print an error that may occur while parsing arguments.
fn print_args_error(e: ArgsError) {
    match e {
        ArgsError::Parse(ref e) => {
            // In case of generic parse error,
            // message provided by the clap library will be the usage string.
            eprintln!("{}", e.message);
        }
        // e => {
        //     eprintln!("Failed to parse arguments: {}", e);
        // }
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
