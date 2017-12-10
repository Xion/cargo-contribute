//!
//! cargo-contribute
//!

             extern crate ansi_term;
             extern crate exitcode;
             extern crate futures;
             extern crate hubcaps;
             extern crate isatty;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate maplit;
             extern crate slog_envlogger;
             extern crate slog_stdlog;
             extern crate slog_stream;
             extern crate time;
             extern crate tokio_core;

// `slog` must precede `log` in declarations here, because we want to simultaneously:
// * use the standard `log` macros
// * be able to initialize the slog logger using slog macros like o!()
#[macro_use] extern crate slog;
#[macro_use] extern crate log;


mod logging;


use std::borrow::Cow;
use std::process::exit;

use hubcaps::Github;
use tokio_core::reactor::Core;


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
    logging::init(0).unwrap();
    info!("{} v{}", *NAME, VERSION.unwrap());

    let mut core = Core::new().unwrap_or_else(|e| {
        error!("Failed to initialize Tokio core: {}", e);
        exit(exitcode::TEMPFAIL);
    });
    let github = Github::new(USER_AGENT.to_owned(), None, &core.handle());
    core.run(futures::future::ok::<(), ()>(())).unwrap();
}
