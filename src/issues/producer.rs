//! Module implementing the suggested issues producer.

use std::error::Error;
use std::fmt;
use std::path::Path;

use futures::{stream, Stream};
use hubcaps::{self, Github};
use hubcaps::search::{IssuesItem, SearchIssuesOptions};
use hyper::client::{Client as HyperClient, Connect};
use tokio_core::reactor::Handle;

use ::USER_AGENT;
use util::{https_client, HttpsConnector};
use super::cargo_toml;
use super::crates_io::Client as CratesIoClient;
use super::model::{Issue, Repository};


// TODO: better error type
pub type IssueStream = Box<Stream<Item=Issue, Error=Box<Error>>>;


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
    pub fn suggest_issues<P: AsRef<Path>>(&self, manifest_path: P) -> IssueStream {
        let manifest_path = manifest_path.as_ref();
        debug!("Suggesting dependency issues for manifest path {}", manifest_path.display());

        // TODO: error handling below
        let deps = cargo_toml::list_dependency_names(manifest_path).unwrap();

        let repos = {
            let crates_io = self.crates_io.clone();
            stream::iter_ok(deps)
                .and_then(move |dep| crates_io.lookup_crate(dep))
                .filter_map(|opt_c| opt_c)
                .filter_map(|crate_| {
                    crate_.metadata.repo_url.as_ref()
                        .and_then(|url| Repository::from_url(url))
                })
        };

        // TODO: limit the number of issues per single repo,
        // or use some version of round-robin or random sampling
        Box::new({
            let github = self.github.clone();
            repos.map(move |repo| repo_issues(&github, &repo)
                    .map_err(|e| Box::new(e) as Box<Error>))
                .flatten()
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
        })
    }
}

impl fmt::Debug for SuggestedIssuesProducer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("SuggestedIssuesProducer")
            .finish()
    }
}


/// Provide suggested issues specifically from given GitHub repo.
fn repo_issues<C: Clone + Connect>(
    github: &Github<C>, repo: &Repository
) -> Box<Stream<Item=IssuesItem, Error=hubcaps::Error>> {
    debug!("Querying for issues in {:?}", repo);

    // TODO: add label filters that signify the issues are "easy"
    let query = format!("repo:{}/{}", repo.owner, repo.name);
    Box::new(
        github.search().issues().iter(query, &SearchIssuesOptions::default())
    )
}
