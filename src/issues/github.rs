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
) -> Box<Stream<Item=IssuesItem, Error=Error>> {
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
            // We may encounter some HTTP errors when doing the search
            // which we translate to an early stream termination via a .then() + take_while() trick.
            .then(move |res| match res {
                Ok(issue_item) => Ok(Some(issue_item)),
                Err(Error(ErrorKind::Fault{ code, error }, _)) => {
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
                        _ => Err(Error::from_kind(ErrorKind::Fault{code, error})),
                    }
                }
                Err(e) => Err(e),
            })
            .take_while(|opt_ii| future::ok(opt_ii.is_some())).map(Option::unwrap)
    )
}
