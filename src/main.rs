//!
//! cargo-contribute
//!

             extern crate exitcode;
             extern crate futures;
             extern crate hubcaps;
#[macro_use] extern crate lazy_static;
             extern crate tokio_core;


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
    eprintln!("{} v{}", *NAME, VERSION.unwrap());

    let mut core = Core::new().unwrap_or_else(|e| {
        eprintln!("Failed to initialize Tokio core: {}", e);
        exit(exitcode::TEMPFAIL);
    });
    let github = Github::new(USER_AGENT.to_owned(), None, &core.handle());
    core.run(futures::future::ok::<(), ()>(())).unwrap();
}
