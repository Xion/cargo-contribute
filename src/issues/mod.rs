//! Module for producing suggested issues for crate dependencies.

mod cargo_toml;
mod crates_io;
mod producer;

pub use self::producer::{Error, SuggestedIssuesProducer};
