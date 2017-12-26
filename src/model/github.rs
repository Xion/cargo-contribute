//! Module with the data types related to GitHub.

use std::fmt;

use hubcaps::search::IssuesItem;
use url::{Url, Host};


const GITHUB_HOST: &'static str = "github.com";


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

    /// Determine the repository from given GitHub URL.
    pub fn from_url<U: AsRef<str>>(repo_url: U) -> Option<Self> {
        let parsed = Url::parse(repo_url.as_ref()).ok()?;
        if parsed.host() == Some(Host::Domain(GITHUB_HOST)) {
            let segs = parsed.path_segments().map(|ps| ps.collect()).unwrap_or_else(Vec::new);
            if segs.len() == 2 {
                // github.com/$OWNER/$NAME (project homepage)
                // or github.com/$OWNER/$NAME.git (direct Git repo URL)
                let owner = segs[0];
                let name = segs[1].trim_right_matches(".git");
                let repo = Repository::new(owner, name);
                trace!("URL {} identified as GitHub repo {}", parsed, repo);
                return Some(repo);
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


#[derive(Debug)]
pub struct Issue {
    /// GitHub repository where this issue comes from.
    pub repo: Repository,
    /// Issue number.
    pub number: u64,
    /// URL to the HTML page of the issue.
    pub url: String,
    /// Issue title.
    pub title: String,
    /// Issue text (body of the first comment).
    pub body: String,
    /// Number of comments on the issue.
    pub comment_count: usize,

}

impl From<IssuesItem> for Issue {
    fn from(input: IssuesItem) -> Self {
        // TODO: reverse order of tuple (again) when this PR is merged:
        // https://github.com/softprops/hubcaps/pull/100
        let (project, owner) = input.repo_tuple();
        Issue{
            repo: Repository::new(owner, project),
            number: input.number,
            url: input.html_url,
            title: input.title,
            body: input.body.unwrap_or_else(String::new),
            comment_count: input.comments as usize,
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "[{}] #{}: {}", self.repo, self.number, self.title)
    }
}


#[cfg(test)]
mod tests {
    use super::Repository;

    #[test]
    fn repository_from_project_url() {
        let repo = Repository::from_url("https://github.com/Xion/gisht").unwrap();
        assert_eq!("Xion", repo.owner);
        assert_eq!("gisht", repo.name);
    }

    #[test]
    fn repository_from_git_url() {
        let repo = Repository::from_url("https://github.com/Xion/callee.git").unwrap();
        assert_eq!("Xion", repo.owner);
        assert_eq!("callee", repo.name);
    }
}
