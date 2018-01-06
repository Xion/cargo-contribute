//! Module for displaying the suggested issues that were found.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use strfmt::{FmtError, strfmt};

use model::Issue;


lazy_static! {
   pub static ref ISSUE_FORMATTERS: HashMap<&'static str, Fmt> = hashmap!{
        "owner" => Fmt::new(
            |issue| issue.repo.owner.as_str().into(),
            "Owner of the project where the issue comes from",
        ),
        "project" => Fmt::new(
            |issue| issue.repo.name.as_str().into(),
            "Issue's project name",
        ),
        "repo" => Fmt::new(
            |issue| format!("{}", issue.repo).into(),
            "Issue's repository, equivalent to {owner}/{project}",
        ),
        "number" => Fmt::new(
            |issue| format!("{}", issue.number).into(),
            "GitHub issue number (ID)",
        ),
        "url" => Fmt::new(
            |issue| issue.url.as_str().into(),
            "URL to issue's HTML page on GitHub",
        ),
        "title" => Fmt::new(
            |issue| issue.title.as_str().into(),
            "Issue title",
        ),
        "body" => Fmt::new(
            |issue| issue.body.as_str().into(),
            "Issue body, i.e. the text of its first comment by the creator",
        ),
        "comments" => Fmt::new(
            |issue| format!("{}", issue.comment_count).into(),
            "Number of comments the issue has",
        ),
    };
}


/// Format an issue according to user-provided format.
pub fn format_issue(fmt: &str, issue: &Issue) -> Result<String, FmtError> {
    let params: HashMap<String, _> = ISSUE_FORMATTERS.iter()
        .map(|(&p, ref f)| (p.to_owned(), f.apply(issue)))
        .collect();
    let line = strfmt(fmt, &params)?;
    Ok(line)
}


/// Formatter for a particular piece of data in an issue.
pub struct Fmt {
    func: fn(&Issue) -> Cow<str>,
    desc: String,
}

impl Fmt {
    #[inline]
    pub fn new<D: ToString>(func: fn(&Issue) -> Cow<str>, desc: D) -> Self {
        Fmt{func, desc: desc.to_string()}
    }
}

impl Fmt {
    #[inline]
    pub fn description(&self) -> &str { self.desc.as_str() }

    #[inline]
    pub fn apply<'i>(&self, issue: &'i Issue) -> Cow<'i, str> {
        (self.func)(issue)
    }
}

impl fmt::Debug for Fmt {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let func_addr = self.func as *const ();
        fmt.debug_struct("Fmt")
            .field("func", &format_args!("0x{:x}", func_addr as usize))
            .field("desc", &self.desc)
            .finish()
    }
}
