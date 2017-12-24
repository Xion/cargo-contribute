//! Module implementing the suggested issues producer.

use std::fmt;
use std::path::Path;

use futures::{future, Future, stream, Stream as StdStream};
use hubcaps::{self, Credentials, Error as HubcapsError, Github, SortDirection};
use hubcaps::errors::ErrorKind;
use hubcaps::search::{IssuesItem, IssuesSort, SearchIssuesOptions};
use hyper::StatusCode;
use hyper::client::{Client as HyperClient, Connect};
use itertools::Itertools;
use log::LogLevel::*;
use rand::{Rng, thread_rng};
use regex::Regex;
use tokio_core::reactor::Handle;

use ::USER_AGENT;
use util::{https_client, HttpsConnector};
use super::cargo_toml::{self, CrateLocation, Dependency};
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

        let mut deps = cargo_toml::list_dependencies(manifest_path)?.into_iter()
            .filter(|d| d.location().is_registry())  // retain only crates.io deps
            .collect_vec();
        thread_rng().shuffle(&mut deps);

        // Determine the GitHub repositories corresponding to dependent crates.
        // In most cases, this means read the package/repository entries
        // from the manifests of those crates by talking to crates.io.
        let repos = {
            // TODO: instead of querying crates.io immediately, look through the local Cargo cache first
            // (~/.cargo/registry/src/**/*) which most likely contains the dep sources already
            let crates_io = self.crates_io.clone();
            stream::iter_ok(deps)
                .and_then(move |dep| {
                    repo_for_dependency(&crates_io, &dep).map_err(Error::CratesIo)
                })
                .filter_map(|opt_repo| opt_repo)
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


fn repo_for_dependency<C: Clone + Connect>(
    crates_io: &CratesIoClient<C>, dep: &Dependency
) -> Box<Future<Item=Option<Repository>, Error=crates_io::Error>> {
    lazy_static! {
        static ref GITHUB_GIT_URL_RE: Regex = Regex::new(
            r#"https://github.com/(?P<owner>\w+)/(?P<name>[^.]+).git"#
        ).unwrap();
    }

    match dep.location() {
        &CrateLocation::Registry{..} => Box::new(
            // TODO: consider looking up only the particular version of the dep
            // that was specified in the manifest (if it indeed was)
            crates_io.lookup_crate(dep.name().to_owned()).map(|opt_c| {
                opt_c.and_then(|crate_| {
                    crate_.metadata.repo_url.as_ref()
                        .and_then(|url| Repository::from_url(url))
                })
            })
        ),
        &CrateLocation::Git{ref url} => Box::new(future::ok(
            GITHUB_GIT_URL_RE.captures(url)
                .map(|caps| Repository::new(&caps["owner"], &caps["name"]))
        )),
        _ => panic!("repo_for_dependency() encountered unexpected {:?}", dep.location()),
    }
}


const GITHUB_API_ROOT: &'static str = "https://api.github.com";

/// Issue labels that we're looking for when suggesting issues.
/// At least one of these must be present.
const ISSUE_LABELS: &'static [&'static str] = &[
    "help wanted",
    "good first issue",
    "easy",
    "beginner",
];

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
    ].iter().join(" ");
    trace!("Search query: {}", query);
    if log_enabled!(Trace) {
        trace!("Accepted issue labels: {}", ISSUE_LABELS.iter().format(", "));
    }

    let options = SearchIssuesOptions::builder()
        // Return the maximum number of results possible
        // (as per https://developer.github.com/v3/search/#search-issues).
        .per_page(100)
        // Surface most recently updated issues first.
        .sort(IssuesSort::Updated)
        .order(SortDirection::Desc)
        .build();

    Box::new(
        github.search().issues().iter(query, &options)
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
            // Filter issues to match one of the labels we're looking for.
            .filter(|ii| ii.labels.iter().any(|l| {
                let label = canonicalize_label(&l.name);
                ISSUE_LABELS.contains(&label.as_str())
            }))
    )
}

/// Convert a GitHub label to its "canonical" form for comparison purposes.
fn canonicalize_label(label: &str) -> String {
    // Strip punctuation, sanitize whitespace, and remove freestanding capital letters
    // (which are often used in labels to keep them sorted).
    label.split(|c: char| c.is_whitespace()).map(|w| w.trim())
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|w| !(w.len() == 1 && w.chars().all(|c| c.is_uppercase())))
        .map(|w| w.to_lowercase())
        .join(" ")
}


#[cfg(test)]
mod tests {
    use tokio_core::reactor::Core;
    use issues::cargo_toml::Dependency;
    use issues::crates_io::Client as CratesIoClient;
    use issues::model::Repository;
    use super::{canonicalize_label, ISSUE_LABELS, repo_for_dependency};

    #[test]
    fn issue_labels_are_canonical() {
        for &label in ISSUE_LABELS.iter() {
            assert!(label == &canonicalize_label(label));
        }
    }

    #[test]
    fn repo_for_github_git_dependency() {
        let mut core = Core::new().unwrap();
        let crates_io = CratesIoClient::new(&core.handle());

        let dep = Dependency::with_git_url("unused", "https://github.com/Xion/gisht.git");
        let repo = core.run(repo_for_dependency(&crates_io, &dep)).unwrap();
        assert_eq!(Some(Repository{owner: "Xion".into(), name: "gisht".into()}), repo);
    }
}
