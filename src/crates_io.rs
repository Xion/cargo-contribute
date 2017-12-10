//! Module for communicating with crates.io API.

use std::borrow::Cow;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use futures::{future, Future as StdFuture, IntoFuture};
use hyper::{self, StatusCode, Uri};
use hyper::client::{Connect, HttpConnector};
use hyper_tls::HttpsConnector;
use serde_json;
use tokio_core::reactor::Handle;

use ext::futures::{BoxFuture, FutureExt};
use ext::hyper::BodyExt;
use util::https_client;


const API_ROOT: &'static str = "https://crates.io/api/v1/";


/// Structure holding information about a single crate.
#[derive(Debug, Deserialize)]
pub struct Crate {
    #[serde(rename = "crate")]
    pub metadata: Metadata,
}

/// Basic crate metadata.
///
/// This structure omits some fields that crates.io returns in the JSON
/// but which are not too useful for us.
#[derive(Debug, Deserialize)]
pub struct Metadata {
    /// Crate identifier.
    pub id: String,
    /// Human-readable crate name.
    pub name: String,
    /// Human-readable crate description.
    pub description: String,
    /// When was the crate created.
    pub created_at: DateTime<Utc>,
    /// When was the crate last updated.
    pub updated_at: DateTime<Utc>,
    /// Keywords associated with the crate.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Crate categories.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Crate homepage, if any.
    #[serde(rename = "homepage")]
    #[serde(default)]
    pub homepage_url: Option<String>,
    /// Documentation URL.
    #[serde(rename = "documentation")]
    #[serde(default)]
    pub docs_url: Option<String>,
    /// Repository URL.
    #[serde(rename = "repository")]
    #[serde(default)]
    pub repo_url: Option<String>,
}


/// Client for the crates.io API.
#[derive(Debug)]
pub struct Client<'c, C: Clone + 'c> {
    http: Cow<'c, hyper::Client<C>>,
}

impl<'c> Client<'c, HttpConnector> {
    #[inline]
    pub fn new(handle: &Handle) -> Self {
        Client{
            http: Cow::Owned(hyper::Client::new(handle)),
        }
    }
}
impl<'c> Client<'c, HttpsConnector<HttpConnector>> {
    #[inline]
    pub fn new_tls(handle: &Handle) -> Self {
        Client{
            http: Cow::Owned(https_client(handle))
        }
    }
}
impl<'c, C: Clone> Client<'c, C> {
    #[inline]
    pub fn with_http(http: &'c hyper::Client<C>) -> Self {
        Client{
            http: Cow::Borrowed(http),
        }
    }
}

impl<'c, C: Clone + Connect> Client<'c, C> {
    /// Lookup a crate by name, returning its metadata.
    /// Returns None if the crate couldn't be found
    pub fn lookup_crate<'id>(&self, id: &'id str) -> Future<'id, Option<Crate>> {
        trace!("Looking up crate `{}` on crates.io...", id);
        let url = Uri::from_str(&format!("{}/crates/{}", API_ROOT, id)).unwrap();
        self.http.get(url).map_err(Error::Http).and_then(move |resp| {
            let status = resp.status();
            if status.is_success() {
                debug!("Successful response from crates.io for `{}`", id);
                resp.body().into_bytes().map_err(Error::Http)
                    .and_then(|bytes| {
                        serde_json::from_reader(&bytes[..])
                            .map(Some).map_err(Error::Json)
                            .into_future()
                    }).into_box()
            } else if status == StatusCode::NotFound {
                warn!("Crate `{}` not found on crates.io", id);
                future::ok(None).into_box()
            } else {
                error!(
                    "Unexpected response code from crates.io while looking up crate `{}`: {}",
                    id, status);
                future::err(Error::Http(hyper::Error::Status)).into_box()
            }
        }).into_box()
    }
}


/// Future type returned by Client methods.
pub type Future<'f, T> = BoxFuture<'f, T, Error>;

/// Error when talking to crates.io.
#[derive(Debug, Error)]
pub enum Error {
    /// HTTP error.
    Http(hyper::Error),
    /// JSON error.
    Json(serde_json::Error),
}
