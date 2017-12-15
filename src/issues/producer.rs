//! Module implementing the suggested issues producer.

use std::fmt;
use std::path::Path;

use futures::{future, Future, stream, Stream as StdStream};
use hubcaps::{self, Github};
use hubcaps::search::{IssuesItem, SearchIssuesOptions};
use hyper::client::{Client as HyperClient, Connect};
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
    pub fn with_http(http: HyperClient<HttpsConnector>) -> Self {
        const API_ROOT: &'static str = "https://api.github.com";
        SuggestedIssuesProducer {
            crates_io: CratesIoClient::with_http(http.clone()),
            github: Github::custom(
                API_ROOT, USER_AGENT.to_owned(), /* credentials */ None, http.clone()),
        }
    }
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
                    let (owner, project) = issue_item.repo_tuple();
                    let issue = Issue{
                        repo: Repository::new(owner, project),
                        number: issue_item.number as usize,
                        title: issue_item.title,
                    };
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


/// Provide suggested issues specifically from given GitHub repo.
fn repo_issues<C: Clone + Connect>(
    github: &Github<C>, repo: Repository
) -> Box<StdStream<Item=IssuesItem, Error=hubcaps::Error>> {
    debug!("Querying for issues in {:?}", repo);
    let github = github.clone();

    // Check if the repo exists first and return an empty stream if it doesn't.
    // (otherwise we would panic on an unwrap inside the hubcaps crate).
    let empty = Box::new(stream::empty()) as Box<StdStream<Item=IssuesItem, Error=hubcaps::Error>>;
    Box::new(
        github.repo(repo.owner.clone(), repo.name.clone()).get()
            .then(move |result| Ok(match result {
                Ok(gh_repo) => {
                    if !gh_repo.has_issues {
                        return Ok(empty);
                    }
                    // TODO: add label filters that signify the issues are "easy"
                    let query = format!("repo:{}/{}", repo.owner, repo.name);
                    Box::new(
                        github.search().issues().iter(query, &SearchIssuesOptions::default())
                    )
                }
                Err(e) => {
                    warn!("Couldn't find repo {}/{} on GitHub: {}", repo.owner, repo.name, e);
                    empty
                }
            }))
        .flatten_stream()
    )
}
