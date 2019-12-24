//! Module for making GitHub API calls.

use futures::{future, Stream};
use hubcaps::{Error, Github, SortDirection};
use hubcaps::errors::ErrorKind;
use hubcaps::search::{IssuesItem, IssuesSort, SearchIssuesOptions};
use hyper::StatusCode;
use hyper::client::Connect;
use itertools::Itertools;

use model::Repository;


/// Return a stream of all open & unassigned issues in given GitHub repository.
pub fn pending_issues<C: Clone + Connect>(
    github: &Github<C>, repo: Repository
) -> Box<dyn Stream<Item=IssuesItem, Error=Error>> {
    let github = github.clone();

    debug!("Querying for issues in {:?}", repo);
    let query = [
        &format!("repo:{}/{}", repo.owner, repo.name),
        "type:issue",
        "state:open",
        "no:assignee",
    ].iter().join(" ");
    trace!("Search query: {}", query);

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
            // We may encounter some non-fatal HTTP errors when doing the search
            // which we translate to an early stream termination via a .then() + take_while() trick.
            .then(move |res| match res {
                Ok(issue_item) => Ok(Some(issue_item)),
                Err(Error(ErrorKind::RateLimit { reset }, _)) => {
                    // In case of having hit GitHub rate limits,
                    // warn that it happened but don't complain about it as a fatal error
                    // and simply terminate the issue stream instead.
                    warn!("API rate limit hit on repo {}, retry in {} seconds",
                        repo, reset.as_secs());
                    // TODO: consider simply waiting the specified time w/o terminating the stream;
                    // this is usually around 60 secs though so may be best to enable with a --flag
                    Ok(None)
                }
                Err(Error(ErrorKind::Fault{ code, error }, _)) => {
                    debug!("HTTP {} error for repository {}/{}: {:?}",
                        code, repo.owner, repo.name, error);
                    if let Some(ref errors) = error.errors {
                        debug!("HTTP {} error details: {:?}", code, errors.iter().format(", "));
                    }
                    match code {
                        // GitHub returns 422 Unprocessable Entity if the repo doesn't exist at all.
                        // This isn't really an error for us (since crate manifests can list invalid
                        // or outdated repos), so we terminate the stream early if it happens.
                        StatusCode::UnprocessableEntity => {
                            warn!("Cannot access repository {}: {}", repo, code);
                            Ok(None)
                        }
                        // If we hit HTTP 403 outside of rate limiting,
                        // it most likely means the repository exists but is private.
                        // Not much else we can do here, so we just stop poking it any more.
                        StatusCode::Forbidden => {
                            warn!("Access denied when searching repository {}: {}",
                                repo, error.message);
                            Ok(None)
                        }
                        // For other HTTP faults, reconstruct the original error.
                        _ => Err(Error::from_kind(ErrorKind::Fault{code, error})),
                    }
                }
                Err(e) => Err(e),
            })
            .take_while(|opt_ii| future::ok(opt_ii.is_some())).map(Option::unwrap)
    )
}
