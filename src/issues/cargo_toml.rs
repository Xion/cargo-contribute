//! Module for reading the crate manifest, Cargo.toml.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use serde::de::{Deserialize, Error as SerdeDeError};
use toml::{self, Value as Toml};

use model::{Dependency, Package};


/// Read [package] information from given Cargo.toml manifest.
pub fn read_package<P: AsRef<Path>>(manifest_path: P) -> Result<Package, Error> {
    let path = manifest_path.as_ref();
    trace!("Reading [package] from manifest: {}", path.display());

    let mut file = File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let manifest: Toml = toml::from_str(&content)?;
    let package = manifest.get("package")
        .ok_or_else(|| Error::Toml(toml::de::Error::custom(format!(
            "[package] section not found in {}", path.display()))))?;
    Deserialize::deserialize(package.clone()).map_err(Error::Toml)
}


/// List the dependencies of a crate described by given Cargo.toml manifest.
pub fn list_dependencies<P: AsRef<Path>>(manifest_path: P) -> Result<Vec<Dependency>, Error> {
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
            let result: Result<Vec<_>, _> = t.iter()
                .map(|(name, v)| Dependency::from_toml(name, v).map_err(Error::Toml))
                .collect();
            match &result {
                &Ok(ref deps) =>
                    debug!("{} dependencies found in {}",deps.len(), path.display()),
                &Err(ref e) =>
                    error!("Error while parsing dependencies in {}: {}", path.display(), e),
            }
            result
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
