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
             extern crate hyper_tls;
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
             extern crate toml;
             extern crate url;

// `slog` must precede `log` in declarations here, because we want to simultaneously:
// * use the standard `log` macros
// * be able to initialize the slog logger using slog macros like o!()
#[macro_use] extern crate slog;
#[macro_use] extern crate log;


mod args;
mod cargo_toml;
mod crates_io;
mod ext;
mod logging;
mod util;


use std::borrow::Cow;
use std::error::Error;
use std::process::exit;

use futures::Stream;
use hubcaps::Github;
use hubcaps::search::{IssuesItem, SearchIssuesOptions};
use hyper::client::Connect;
use log::LogLevel::*;
use tokio_core::reactor::{Core, Handle};

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
    let handle = core.handle();

    let github = Github::new(USER_AGENT.to_owned(), None, &handle);
    core.run(futures::future::ok::<(), ()>(())).unwrap();

    let deps = cargo_toml::list_dependency_names("./Cargo.toml").unwrap();
    core.run(
        crate_repositories(&handle, deps.iter().map(|d| d.as_str()))
            // TODO: limit the number of issues per single repo,
            // or use some version of round-robin or random sampling
            .map(|repo| repo_issues(&github, &repo)
                .map_err(|e| Box::new(e) as Box<Error>))
            .flatten().take(10)
            .for_each(|issue| {
                let (owner, project) = issue.repo_tuple();
                println!("[{}/{}] #{}: {}", owner, project, issue.number, issue.title);
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


#[derive(Debug)]
pub struct GithubRepo {
    owner: String,
    name: String,
}

impl GithubRepo {
    #[inline]
    pub fn new<O: ToString, N: ToString>(owner: O, name: N) -> Self {
        GithubRepo {
            owner: owner.to_string(),
            name: name.to_string(),
        }
    }

    pub fn from_url(repo_url: &str) -> Option<Self> {
        let parsed = url::Url::parse(repo_url).ok()?;
        if parsed.host() == Some(url::Host::Domain("github.com")) {
            let segs = parsed.path_segments().map(|ps| ps.collect()).unwrap_or_else(Vec::new);
            if segs.len() == 2 {
                // github.com/$OWNER/$REPO
                return Some(GithubRepo::new(segs[0], segs[1]));
            }
        }
        None
    }
}

fn crate_repositories<'c, I>(
    handle: &Handle, crates: I
) -> Box<futures::Stream<Item=GithubRepo, Error=crates_io::Error> + 'c>
    where I: IntoIterator<Item=&'c str> + 'c
{
    let client = crates_io::Client::new_tls(handle);
    Box::new(
        futures::stream::iter_ok(crates)
            .and_then(move |crate_| client.lookup_crate(crate_))
            .filter_map(|opt_c| opt_c)
            .filter_map(|c| {
                c.metadata.repo_url.as_ref().and_then(|url| GithubRepo::from_url(url))
            })
    )
}

fn repo_issues<C: Clone + Connect>(
    github: &Github<C>, repo: &GithubRepo
) -> Box<futures::Stream<Item=IssuesItem, Error=hubcaps::Error>> {
    debug!("Querying for issues in {:?}", repo);

    // TODO: add label filters that signify the issues are "easy"
    let query = format!("repo:{}/{}", repo.owner, repo.name);
    Box::new(
        github.search().issues().iter(query, &SearchIssuesOptions::default())
    )
}
