//! Module with the data types used by issue producer.

use std::fmt;

use hubcaps::search::IssuesItem;
use url::{Url, Host};


const GITHUB_HOST: &'static str = "github.com";


#[derive(Debug)]
pub struct Issue {
    /// GitHub repository where this issue comes from.
    pub repo: Repository,
    /// Issue number.
    pub number: usize,
    /// Issue title.
    pub title: String,
    /// URL to the HTML page of the issue.
    pub url: String,
}

impl From<IssuesItem> for Issue {
    fn from(input: IssuesItem) -> Self {
        // TODO: reverse order of tuple (again) when this PR is merged:
        // https://github.com/softprops/hubcaps/pull/100
        let (project, owner) = input.repo_tuple();
        Issue{
            repo: Repository::new(owner, project),
            number: input.number as usize,
            title: input.title,
            url: input.html_url,
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "[{}] #{}: {}", self.repo, self.number, self.title)
    }
}



/// Represents a GitHub repository.
#[derive(Debug, Eq, PartialEq)]
pub struct Repository {
    pub owner: String,
    pub name: String,
}

impl Repository {
    #[inline]
    pub fn new<O: ToString, N: ToString>(owner: O, name: N) -> Self {
        Repository {
            owner: owner.to_string(),
            name: name.to_string(),
        }
    }

    pub fn from_url(repo_url: &str) -> Option<Self> {
        let parsed = Url::parse(repo_url).ok()?;
        if parsed.host() == Some(Host::Domain(GITHUB_HOST)) {
            let segs = parsed.path_segments().map(|ps| ps.collect()).unwrap_or_else(Vec::new);
            if segs.len() == 2 {
                // github.com/$OWNER/$REPO
                return Some(Repository::new(segs[0], segs[1]));
            }
        }
        None
    }
}

impl fmt::Display for Repository {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}/{}", self.owner, self.name)
    }
}
