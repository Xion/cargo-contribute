//! Utility module.

use hyper;
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use tokio_core::reactor::Handle;


const HTTPS_DNS_THREADS: usize = 4;

/// Type of a TLS-capable asynchronous Hyper client.
pub type HttpsClient = hyper::Client<HttpsConnector<HttpConnector>>;

/// Create an asynchronous, TLS-capable HTTP Hyper client
/// working with given Tokio Handle.
pub fn https_client(handle: &Handle) -> HttpsClient {
    let connector = HttpsConnector::new(HTTPS_DNS_THREADS, handle).unwrap();
    hyper::client::Config::default()
        .connector(connector)
        .build(handle)
}
