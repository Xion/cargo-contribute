//! Module implementing the suggested issues producer.

use std::fmt;
use std::path::Path;

use futures::{future, Future, stream, Stream as StdStream};
use hubcaps::{self, Credentials, Error as HubcapsError, Github};
use hubcaps::errors::ErrorKind;
use hubcaps::search::{IssuesItem, SearchIssuesOptions};
use hyper::StatusCode;
use hyper::client::{Client as HyperClient, Connect};
use itertools::Itertools;
use rand::{Rng, thread_rng};
use tokio_core::reactor::Handle;

use ::USER_AGENT;
use util::{https_client, HttpsConnector};
use super::cargo_toml;
use super::crates_io::{self, Client as CratesIoClient};
use super::model::{Issue, Repository};


type Stream<T> = Box<StdStream<Item=T, Error=Error>>;

/// Type of the Stream returned by SuggestedIssuesProducer.
pub type IssueStream = Stream<Issue>;


/// Structure wrapping all the state necessary to produce suggested issues
/// for given crate manifest.
pub struct SuggestedIssuesProducer {
    crates_io: CratesIoClient<HttpsConnector>,
    github: Github<HttpsConnector>,
}

impl SuggestedIssuesProducer {
    /// Create a new SuggestedIssuesProducer.
    pub fn new(handle: &Handle) -> Self {
        Self::with_http(https_client(handle))
    }

    #[inline]
    pub fn with_github_token(token: &str, handle: &Handle) -> Self {
        let http = https_client(handle);
        SuggestedIssuesProducer {
            crates_io: CratesIoClient::with_http(http.clone()),
            github: Github::custom(
                GITHUB_API_ROOT, USER_AGENT.to_owned(),
                Some(Credentials::Token(token.to_owned())), http.clone()),
        }
    }

    #[inline]
    pub fn with_http(http: HyperClient<HttpsConnector>) -> Self {
        SuggestedIssuesProducer {
            crates_io: CratesIoClient::with_http(http.clone()),
            github: Github::custom(
                GITHUB_API_ROOT, USER_AGENT.to_owned(), /* credentials */ None, http.clone()),
        }
    }

    // TODO: consider providing a builder
}

impl SuggestedIssuesProducer {
    /// Suggest issues for a crate with given Cargo.toml manifest.
    pub fn suggest_issues<P: AsRef<Path>>(&self, manifest_path: P) -> Result<IssueStream, Error> {
        let manifest_path = manifest_path.as_ref();
        debug!("Suggesting dependency issues for manifest path {}", manifest_path.display());

        let mut deps = cargo_toml::list_dependency_names(manifest_path)?;
        thread_rng().shuffle(&mut deps);

        // Read the package/repository entries from the manifests of dependent crates
        // by talking to crates.io.
        let repos = {
            let crates_io = self.crates_io.clone();
            stream::iter_ok(deps)
                .and_then(move |dep| crates_io.lookup_crate(dep).map_err(Error::CratesIo))
                .filter_map(|opt_c| opt_c)
                .filter_map(|crate_| {
                    crate_.metadata.repo_url.as_ref()
                        .and_then(|url| Repository::from_url(url))
                })
        };

        // For each repo, search for suitable issues and stream them in a round-robin fashion
        // (via this hideous amalgamation of fold() + flatten_stream()).
        Ok(Box::new({
            let github = self.github.clone();
            repos.map(move |repo| repo_issues(&github, repo).map_err(Error::GitHub))
                // Yes, each cast and each turbofish is necessary here -_-
                .fold(Box::new(stream::empty()) as Stream<IssuesItem>,
                    |acc, x| future::ok::<_, Error>(
                        Box::new(acc.select(x)) as Stream<IssuesItem>,
                    ))
                .flatten_stream()
                .map(|issue_item| {
                    let issue = issue_item.into();
                    trace!("Found issue: {}", issue);
                    issue
                })
        }))
    }
}

impl fmt::Debug for SuggestedIssuesProducer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("SuggestedIssuesProducer")
            .finish()
    }
}


/// Error that can occur while producing suggested issues.
#[derive(Debug, Error)]
pub enum Error {
    #[error(msg = "error reading crate manifest")]
    Manifest(cargo_toml::Error),
    #[error(msg = "error contacting crates.io")]
    CratesIo(crates_io::Error),
    #[error(msg = "error contacting github.com")]
    GitHub(hubcaps::Error),
}


const GITHUB_API_ROOT: &'static str = "https://api.github.com";

/// Provide suggested issues specifically from given GitHub repo.
fn repo_issues<C: Clone + Connect>(
    github: &Github<C>, repo: Repository
) -> Box<StdStream<Item=IssuesItem, Error=HubcapsError>> {
    let github = github.clone();

    debug!("Querying for issues in {:?}", repo);
    let query = [
        &format!("repo:{}/{}", repo.owner, repo.name),
        "type:issue",
        "state:open",
        "no:assignee",
        // TODO: find some way for specifying multiple labels linked with OR
        // (GitHub only seems to support AND)
        r#"label:"help wanted""#,
        // Surface most recently updated issues first.
        "sort:updated-desc",
    ].iter().join(" ");
    trace!("Search query: {}", query);

    Box::new(
        github.search().issues().iter(query, &SearchIssuesOptions::default())
            // We may encounter some HTTP errors when doing the search
            // which we translate to an early stream termination via a .then() + take_while() trick.
            .then(move |res| match res {
                Ok(issue_item) => Ok(Some(issue_item)),
                Err(HubcapsError(ErrorKind::Fault{ code, error }, _)) => {
                    debug!("HTTP {} error for repository {}/{}: {:?}",
                        code, repo.owner, repo.name, error);
                    match code {
                        // GitHub returns 422 Unprocessable Entity if the repo doesn't exist at all.
                        // This isn't really an error for us (since crate manifests can list invalid
                        // or outdated repos), so we terminate the stream early if it happens.
                        StatusCode::UnprocessableEntity => {
                            warn!("Cannot access repository {}: {}", repo, code);
                            Ok(None)
                        }
                        // If we hit HTTP 403, that probably means we've reached the GitHub rate limit.
                        // Not much else we can do here, so we just stop poking it any more.
                        StatusCode::Forbidden => {
                            warn!("Possible rate limit hit when searching repository {}: {}",
                                repo, error.message);
                            if let Some(ref errors) = error.errors {
                                debug!("HTTP 403 error details: {:?}", errors.iter().format(", "));
                            }
                            Ok(None)
                        }
                        // For other HTTP faults, reconstruct the original error.
                        _ => Err(HubcapsError::from_kind(ErrorKind::Fault{code, error})),
                    }
                }
                Err(e) => Err(e),
            })
            .take_while(|opt_ii| future::ok(opt_ii.is_some())).map(Option::unwrap)
    )
}
