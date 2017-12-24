//! Module for reading the crate manifest, Cargo.toml.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use itertools::Itertools;
use serde::de::Error as SerdeDeError;
use toml::{self, Value as Toml};


/// A dependent crate read from Cargo.toml manifest.
pub struct Dependency {
    /// Name of the crate.
    name: String,
    /// Crate version (which may be absent or equal to "*").
    version: Option<String>,
    /// Local path to the crate (if it's indeed local and not from crates.io).
    path: Option<PathBuf>,
}

impl Dependency {
    #[inline]
    pub fn with_version<N, V>(name: N, version: V) -> Self
        where N: ToString, V: ToString
    {
        Dependency{
            name: name.to_string(),
            version: Some(version.to_string()),
            path: None,
        }
    }

    #[inline]
    pub fn with_path<N, P>(name: N, path: P) -> Self
        where N: ToString, P: AsRef<Path>
    {
        Dependency{
            name: name.to_string(),
            version: None,
            path: Some(path.as_ref().to_owned()),
        }
    }

    /// Create a `Dependency` struct by interpreting a TOML value from Cargo.toml.
    pub fn from_toml<N: ToString>(name: N, toml: &Toml) -> Result<Self, Error> {
        let mut attrs = BTreeMap::new();
        match toml {
            &Toml::String(ref v) => { attrs.insert("version", v.as_str()); }
            &Toml::Table(ref t) => {
                attrs.extend(
                    t.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.as_str(), s)))
                );
            }
            v => {
                return Err(Error::Toml(toml::de::Error::custom(format!(
                    "dependency spec must be a string or a table, got {}",
                    v.type_str()))));
            }
        }
        match (attrs.get("version"), attrs.get("path")) {
            (Some(v), None) => Ok(Dependency::with_version(name, v)),
            (None, Some(p)) => Ok(Dependency::with_path(name, p)),
            _ => Err(Error::Toml(toml::de::Error::custom(format!(
                "dependency must specify either `version` or `path`")))),
        }
    }
}

impl Dependency {
    #[inline]
    pub fn name(&self) -> &str { &self.name }
    #[inline]
    pub fn is_local(&self) -> bool { self.path.is_some() }
}

impl fmt::Debug for Dependency {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut ds = fmt.debug_struct("Dependency");
        ds.field("name", &self.name);
        if let Some(ref version) = self.version {
            ds.field("version", version);
        }
        if let Some(ref path) = self.path {
            ds.field("path", &path.display());
        }
        ds.finish()
    }
}

impl fmt::Display for Dependency {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut attrs: HashMap<&'static str, Cow<str>> = HashMap::new();
        if let Some(ref version) = self.version {
            attrs.insert("version", version.as_str().into());
        }
        if let Some(ref path) = self.path {
            attrs.insert("path", format!("{}", path.display()).into());
        }

        match attrs.len() {
            0 => write!(fmt, "[dependencies.{}]", self.name),
            1 => {
                if let Some(version) = attrs.get("version") {
                    write!(fmt, "{} = \"{}\"", self.name, version)
                } else {
                    let (ref k, ref v) = attrs.iter().next().unwrap();
                    write!(fmt, "{} = {{ {} = \"{}\" }}", self.name, k, v)
                }
            }
            _ => {
                write!(fmt, "{} = {{ {} }}", self.name, attrs.iter()
                    .format_with(", ", |(k, v), f| {
                        f(&format_args!("{} = \"{}\"", k, v))
                    }))
            }
        }
    }
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
                .map(|(name, v)| Dependency::from_toml(name, v)).collect();
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
