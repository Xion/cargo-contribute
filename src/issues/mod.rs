//! Module for producing suggested issues for crate dependencies.

mod cargo_toml;
mod crates_io;
mod model;
mod producer;

pub use self::model::{Issue, Repository};
pub use self::producer::{Error, SuggestedIssuesProducer};
