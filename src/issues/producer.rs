//! Module implementing the suggested issues producer.

use std::collections::HashSet;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

use futures::{future, Future, stream, Stream as StdStream};
use glob::glob;
use hubcaps::{self, Credentials, Error as HubcapsError, Github};
use hubcaps::search::IssuesItem;
use hyper::client::{Client as HyperClient, Connect};
use itertools::Itertools;
use log::LogLevel::*;
use rand::{Rng, thread_rng};
use regex::Regex;
use semver::{Version, VersionReq};
use tokio_core::reactor::Handle;

use ::USER_AGENT;
use model::{CrateLocation, Dependency, Issue, Package, Repository};
use util::{https_client, HttpsConnector};
use super::cargo_toml;
use super::crates_io::{self, Client as CratesIoClient};
use super::github::pending_issues;


type Stream<T> = Box<StdStream<Item=T, Error=Error>>;

/// Type of the Stream returned by `SuggestedIssuesProducer`.
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
    #[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
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

        let mut deps = cargo_toml::list_dependencies(manifest_path)?;
        thread_rng().shuffle(&mut deps);

        // Determine the GitHub repositories corresponding to dependent crates.
        // In most cases, this means read the package/repository entries
        // from the manifests of those crates by looking at Cargo cache or talking to crates.io.
        let mut repo_set = HashSet::new();
        let repos = {
            let manifest_path = manifest_path.to_owned();
            let crates_io = self.crates_io.clone();
            stream::iter_ok(deps)
                .and_then(move |dep| {
                    repo_for_dependency(&manifest_path, &crates_io, &dep).map_err(Error::CratesIo)
                })
                .filter_map(move |opt_repo| {
                    if let Some(repo) = opt_repo {
                        // Check if we've reported on this repo already.
                        if repo_set.contains(&repo) { None }
                        else {
                            repo_set.insert(repo.clone()); Some(repo)
                        }
                    } else { None }
                })
        };

        // For each repo, search for suitable issues and stream them in a round-robin fashion
        // (via this hideous amalgamation of fold() + flatten_stream()).
        Ok(Box::new({
            let github = self.github.clone();
            repos.map(move |repo| suggest_repo_issues(&github, repo).map_err(Error::GitHub))
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


// Finding repositories of crate dependencies

lazy_static! {
    static ref GITHUB_GIT_HTTPS_URL_RE: Regex = Regex::new(
        r#"https?://(www\.)?github\.com/(?P<owner>\w+)/(?P<name>[^.]+)(\.git)?"#
    ).unwrap();
    static ref GITHUB_GIT_SSH_URL_RE: Regex = Regex::new(
        r#"git@github\.com:(?P<owner>\w+)/(?P<name>[^.]+)\.git"#
    ).unwrap();
}

lazy_static! {
    // TODO: a dot-dir in $HOME probably doesn't work on Windows,
    // so we likely need to look in AppData or similar instead
    static ref CARGO_REGISTRY_CACHE_DIR: Option<PathBuf> = env::home_dir()
        .map(|home| home.join(".cargo/registry/src"));
}

fn repo_for_dependency<P: AsRef<Path>, C: Clone + Connect>(
    manifest_path: P, crates_io: &CratesIoClient<C>, dep: &Dependency
) -> Box<Future<Item=Option<Repository>, Error=crates_io::Error>> {
    match *dep.location() {
        CrateLocation::Registry{ref version} => {
            // Check the local Cargo cache first for the dependent crate's manifest.
            // Otherwise, fall back to querying crates.io.
            if let Some(package) = read_cached_manifest(dep.name(), version) {
                return Box::new(future::ok(
                    package.repository.as_ref().and_then(Repository::from_http_url)
                ));
            }
            debug!("Dependency {}={} not found in local Cargo cache", dep.name(), version);
            Box::new(
                crates_io.lookup_crate(dep.name().to_owned()).map(|opt_c| {
                    // Some crates list their GitHub URLs only as "homepage" in the manifest,
                    // so we'll try that in addition to the more appropriate "repository".
                    let crate_ = opt_c?;
                    crate_.metadata.repo_url.as_ref().and_then(Repository::from_http_url)
                        .or_else(|| Repository::from_http_url(crate_.metadata.homepage_url.as_ref()?))
                })
            )
        }
        CrateLocation::Filesystem{ref path} => Box::new(future::ok({
            manifest_path.as_ref().parent()
                .and_then(|manifest_dir| manifest_dir.join(path).canonicalize().map_err(|e| {
                    warn!("Error resolving path=... dependency `{}`: {}", dep.name(), e); e
                }).ok())
                .and_then(|manifest_dir| {
                    let dep_manifest_path = manifest_dir.join(path).join("Cargo.toml");
                    cargo_toml::read_package(dep_manifest_path)
                        .map_err(|e| {
                            warn!("Error loading manifest of local dependency `{}`: {}",
                                dep.name(), e); e
                        }).ok()
                })
                .and_then(|p| {
                    // Like above, try `repository` followed by `homepage`.
                    p.repository.as_ref().and_then(Repository::from_http_url)
                        .or_else(|| p.homepage.as_ref().and_then(Repository::from_http_url))
                })
        })),
        CrateLocation::Git{ref url} => Box::new(future::ok(
            GITHUB_GIT_HTTPS_URL_RE.captures(url)
                .or_else(|| GITHUB_GIT_SSH_URL_RE.captures(url))
                .map(|caps| Repository::new(&caps["owner"], &caps["name"]))
        )),
    }
}

fn read_cached_manifest<N>(crate_: N, version: &VersionReq) -> Option<Package>
    where N: AsRef<str>
{
    let crate_ = crate_.as_ref();
    trace!("Trying to find cached manifest of crate {}={}", crate_, version);

    let cache_root = match CARGO_REGISTRY_CACHE_DIR.as_ref() {
        Some(cr) => cr,
        None => {
            warn!("Cannot find Cargo's registry cache directory.");
            return None;
        }
    };

    // Find all cached versions of the crate and pick the newest matching one.
    let pattern = format!("{}/*/{}-*", cache_root.display(), crate_);
    trace!("Globbing with pattern: {}", pattern);
    let manifest_path = glob(&pattern).unwrap()
        .filter_map(|res| {
            if let Err(ref e) = res { trace!("Error while globbing: {}", e); }
            res.ok()
        })
        .filter_map(|dir| {
            // Extract the cached crate version and match it with the dependency requirement.
            let version_suffix = dir.file_name().unwrap().to_str().unwrap()
                .rsplit('-').next().unwrap();
            let cached_version = Version::parse(version_suffix).unwrap_or_else(|e| {
                panic!("Failed to parse crate version `{}` from cached path {}: {}",
                    version_suffix, dir.display(), e);
            });
            if version.matches(&cached_version) {
                Some((cached_version, dir.to_owned()))
            } else {
                None
            }
        })
        .sorted_by_key(|&(ref v, _)| v.clone()).map(|(_, d)| d)
        .next().or_else(|| {
            debug!("Crate {}-{} not found in Cargo cache", crate_, version); None
        })?
        .join("Cargo.toml");

    if manifest_path.exists() {
        debug!("Cached manifest found at {}", manifest_path.display());
    } else {
        warn!("Found cached crate {}={} but it's missing its manifest", crate_, version);
        return None;
    }

    cargo_toml::read_package(manifest_path).map_err(|e| {
        warn!("Error while reading cached manifest of {}={}: {}", crate_, version, e);
    }).ok()
}


// Searching suitable issues on GitHub

const GITHUB_API_ROOT: &str = "https://api.github.com";

/// Issue labels that we're looking for when suggesting issues.
/// At least one of these must be present.
const ISSUE_LABELS: &[&str] = &[
    "help wanted",
    "good first issue",
    "easy",
    "beginner",
];

/// Provide suggested issues specifically from given GitHub repo.
fn suggest_repo_issues<C: Clone + Connect>(
    github: &Github<C>, repo: Repository
) -> Box<StdStream<Item=IssuesItem, Error=HubcapsError>> {
    let result = Box::new(
        // Filter pending issues to match one of the labels we're looking for.
        pending_issues(github, repo).filter(|ii| ii.labels.iter().any(|l| {
            let label = canonicalize_label(&l.name);
            ISSUE_LABELS.contains(&label.as_str())
        }))
    );
    if log_enabled!(Trace) {
        trace!("Accepted issue labels: {}", ISSUE_LABELS.iter().format(", "));
    }
    result
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
    use issues::crates_io::Client as CratesIoClient;
    use model::{Dependency, Repository};
    use super::{canonicalize_label, ISSUE_LABELS, repo_for_dependency};

    #[test]
    fn issue_labels_are_canonical() {
        for &label in ISSUE_LABELS.iter() {
            assert!(label == &canonicalize_label(label));
        }
    }

    #[test]
    fn repo_for_github_http_git_dependency() {
        let mut core = Core::new().unwrap();
        let crates_io = CratesIoClient::new(&core.handle());

        const REPO_URLS: &[&str] = &[
            "https://github.com/Xion/gisht.git",
            "http://github.com/Xion/gisht.git",
            "http://www.github.com/Xion/gisht.git",
            "https://www.github.com/Xion/gisht.git",
            "https://github.com/Xion/gisht",
            "http://github.com/Xion/gisht",
            "http://www.github.com/Xion/gisht",
            "https://www.github.com/Xion/gisht",
        ];
        let expected_repo = Repository{owner: "Xion".into(), name: "gisht".into()};
        for &repo_url in REPO_URLS {
            let dep = Dependency::with_git_url("unused", repo_url);
            let repo = core.run(repo_for_dependency("unused", &crates_io, &dep)).unwrap();
            assert_eq!(Some(&expected_repo), repo.as_ref());
        }
    }

    #[test]
    fn repo_for_github_ssh_git_dependency() {
        let mut core = Core::new().unwrap();
        let crates_io = CratesIoClient::new(&core.handle());

        let dep = Dependency::with_git_url("unused", "git@github.com:Xion/gisht.git");
        let repo = core.run(repo_for_dependency("unused", &crates_io, &dep)).unwrap();
        assert_eq!(Some(Repository{owner: "Xion".into(), name: "gisht".into()}), repo);
    }
}
