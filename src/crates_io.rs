//! Module for communicating with crates.io API.

use chrono::{DateTime, Utc};


const API_ROOT: &'static str = "https://crates.io/api/v1/";


/// Structure holding information about a single crate.
#[derive(Debug, Deserialize)]
pub struct Crate {
    #[serde(rename = "crate")]
    metadata: Metadata,
}

/// Basic crate metadata.
///
/// This structure omits some fields that crates.io returns in the JSON
/// but which are not too useful for us.
#[derive(Debug, Deserialize)]
pub struct Metadata {
    /// Crate identifier.
    id: String,
    /// Human-readable crate name.
    name: String,
    /// Human-readable crate description.
    description: String,
    /// When was the crate created.
    created_at: DateTime<Utc>,
    /// When was the crate last updated.
    updated_at: DateTime<Utc>,
    /// Keywords associated with the crate.
    #[serde(default)]
    keywords: Vec<String>,
    /// Crate categories.
    #[serde(default)]
    categories: Vec<String>,
    /// Crate homepage, if any.
    #[serde(rename = "homepage")]
    #[serde(default)]
    homepage_url: Option<String>,
    /// Documentation URL.
    #[serde(rename = "documentation")]
    #[serde(default)]
    docs_url: Option<String>,
    /// Repository URL.
    #[serde(rename = "repository")]
    #[serde(default)]
    repo_url: Option<String>,
}
