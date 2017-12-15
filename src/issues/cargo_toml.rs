//! Module for reading the crate manifest, Cargo.toml.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use serde::de::Error as SerdeDeError;
use toml::{self, Value as Toml};


/// List the names of crate dependencies from given Cargo.toml path.
pub fn list_dependency_names<P: AsRef<Path>>(manifest_path: P) -> Result<Vec<String>, Error> {
    let path = manifest_path.as_ref();
    trace!("Reading dependencies from manifest: {}", path.display());

    let mut file = File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let manifest: Toml = toml::from_str(&content)?;
    match manifest.get("dependencies") {
        None => {
            debug!("No dependencies found in {}", path.display());
            Ok(vec![])
        }
        Some(&Toml::Table(ref t)) => {
            // TODO: eliminate non-crates.io dependencies, like path="..." ones
            let result: Vec<_> = t.keys().cloned().collect();
            debug!("{} dependencies found in {}", result.len(), path.display());
            Ok(result)
        }
        Some(v) => Err(Error::Toml(toml::de::Error::custom(format!(
            "[dependencies] must be a table, got {}", v.type_str())))),
    }
}


/// Error while reading Cargo.toml manifest.
#[derive(Debug, Error)]
pub enum Error {
    Io(io::Error),
    Toml(toml::de::Error),
}
