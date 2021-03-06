//! Module with the data types related to GitHub.

use std::fmt;

use hubcaps::search::IssuesItem;
use url::{Url, Host};


const GITHUB_HOSTS: &[&str] = &["github.com", "www.github.com"];


/// Represents a GitHub repository.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Repository {
    pub owner: String,
    pub name: String,
}

impl Repository {
    #[inline]
    #[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
    pub fn new<O: ToString, N: ToString>(owner: O, name: N) -> Self {
        Repository {
            owner: owner.to_string(),
            name: name.to_string(),
        }
    }

    /// Determine the repository from given GitHub HTTP URL.
    pub fn from_http_url<U: AsRef<str>>(repo_url: U) -> Option<Self> {
        let parsed = Url::parse(repo_url.as_ref()).ok()?;
        if GITHUB_HOSTS.iter().any(|h| parsed.host() == Some(Host::Domain(h)))  {
            let segs = parsed.path_segments().map(|ps| ps.collect()).unwrap_or_else(Vec::new);
            if segs.len() == 2 {
                // github.com/$OWNER/$NAME (project homepage)
                // or github.com/$OWNER/$NAME.git (direct Git repo URL)
                let owner = segs[0];
                let name = segs[1].trim_end_matches(".git");
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
        let (owner, project) = repo_tuple(&input);
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
/// A fixed version of hubcaps::IssuesItem::repo_tuple,
/// because the original in 0.5.0 doesn't handle URLs GitHub returns
/// (i.e. "https://api.github.com/repos/$OWNER/$REPO").
fn repo_tuple(issues_item: &IssuesItem) -> (String, String) {
    let url = Url::parse(&issues_item.repository_url).unwrap();
    let segs = url.path_segments().map(|ps| ps.collect()).unwrap_or_else(Vec::new);
    let owner = if segs.len() > 1 { segs[segs.len() - 2] } else { "" };
    let project = if segs.len() > 0 { segs[segs.len() - 1] } else { "" };
    (owner.to_owned(), project.to_owned())
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
        let repo = Repository::from_http_url("https://github.com/Xion/gisht").unwrap();
        assert_eq!("Xion", repo.owner);
        assert_eq!("gisht", repo.name);
    }

    #[test]
    fn repository_from_git_url() {
        let repo = Repository::from_http_url("https://github.com/Xion/callee.git").unwrap();
        assert_eq!("Xion", repo.owner);
        assert_eq!("callee", repo.name);
    }
}
