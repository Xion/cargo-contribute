//! Types representing the relevant parts of a crate manifest.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use semver::VersionReq;
use serde::de::Error;
use toml::{self, Value as Toml};


/// Represents the [package] section of Cargo.toml.
#[derive(Debug, Deserialize)]
pub struct Package {
    /// Human-readable crate name.
    pub name: String,
    /// Crate version.
    pub version: String,
    /// Human-readable crate description.
    #[serde(default)]
    pub description: String,
    /// Author(s) of the crate.
    pub authors: Vec<String>,
    /// Crate license.
    #[serde(default)]
    pub license: Option<String>,
    /// Keywords associated with the crate.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Crate categories.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Crate homepage, if any.
    #[serde(default)]
    pub homepage: Option<String>,
    /// Documentation URL.
    #[serde(default)]
    pub documentation: Option<String>,
    /// Repository URL.
    #[serde(default)]
    pub repository: Option<String>,
}


/// A dependent crate read from Cargo.toml manifest.
pub struct Dependency {
    /// Name of the crate.
    name: String,
    /// Location of crate's sources.
    location: CrateLocation,
}

impl Dependency {
    #[inline]
    pub fn with_version<N, V>(name: N, version: V) -> Self
        where N: ToString, V: AsRef<str>
    {
        let version = version.as_ref();
        Dependency{
            name: name.to_string(),
            location: CrateLocation::Registry{
                version: if version == "*" {
                    VersionReq::any()
                } else {
                    // TODO: some error handling here
                    VersionReq::parse(version.as_ref()).unwrap()
                },
            },
        }
    }

    #[inline]
    pub fn with_path<N, P>(name: N, path: P) -> Self
        where N: ToString, P: AsRef<Path>
    {
        Dependency{
            name: name.to_string(),
            location: CrateLocation::Filesystem{path: path.as_ref().to_owned()},
        }
    }

    #[inline]
    pub fn with_git_url<N, U>(name: N, url: U) -> Self
        where N: ToString, U: ToString
    {
        Dependency{
            name: name.to_string(),
            location: CrateLocation::Git{url: url.to_string()},
        }
    }

    // TODO: consider implementing custom Deserialize instead
    /// Create a `Dependency` struct by interpreting a TOML value from Cargo.toml.
    pub fn from_toml<N: ToString>(name: N, toml: &Toml) -> Result<Self, toml::de::Error> {
        let mut attrs = BTreeMap::new();
        match toml {
            &Toml::String(ref v) => { attrs.insert("version", v.as_str()); }
            &Toml::Table(ref t) => {
                attrs.extend(
                    t.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.as_str(), s)))
                );
            }
            v => {
                return Err(toml::de::Error::custom(format!(
                    "dependency spec must be a string or a table, got {}",
                    v.type_str())));
            }
        }
        match (attrs.get("version"), attrs.get("path"), attrs.get("git")) {
            (Some(v), None, None) => Ok(Dependency::with_version(name, v)),
            (None, Some(p), None) => Ok(Dependency::with_path(name, p)),
            (None, None, Some(u)) => Ok(Dependency::with_git_url(name, u)),
            _ => Err(toml::de::Error::custom(format!(
                "dependency must specify either `version` or `path`"))),
        }
    }
}

impl Dependency {
    #[inline]
    pub fn name(&self) -> &str { &self.name }
    #[inline]
    pub fn location(&self) -> &CrateLocation { &self.location }
}

impl fmt::Debug for Dependency {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut ds = fmt.debug_struct("Dependency");
        ds.field("name", &self.name);
        match self.location {
            CrateLocation::Registry{ref version} =>
                ds.field("version", version),
            CrateLocation::Filesystem{ref path} =>
                ds.field("path", &path.display()),
            CrateLocation::Git{ref url} => ds.field("git", url),
        };
        ds.finish()
    }
}

impl fmt::Display for Dependency {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self.location {
            CrateLocation::Registry{ref version} =>
                write!(fmt, "{} = \"{}\"", self.name, version),
            CrateLocation::Filesystem{ref path} =>
                write!(fmt, "{} = {{ path = \"{}\" }}", self.name, path.display()),
            CrateLocation::Git{ref url} =>
                write!(fmt, "{} = {{ git = \"{}\" }}", self.name, url),
        }
    }
}


/// Describes where is a particular dependent crate located.
#[derive(Debug)]
pub enum CrateLocation {
    /// Crate is hosted on crates.io.
    Registry{ version: VersionReq },
    /// Crate is available under given filesystem path.
    Filesystem{ path: PathBuf },
    /// Crate is kept in a Git repository under given URL.
    Git{ url: String },
}

impl CrateLocation {
    #[inline]
    pub fn is_registry(&self) -> bool {
        match self { &CrateLocation::Registry{..} => true, _ => false }
    }

    #[inline]
    pub fn is_filesystem(&self) -> bool {
        match self { &CrateLocation::Filesystem{..} => true, _ => false }
    }

    #[inline]
    pub fn is_git(&self) -> bool {
        match self { &CrateLocation::Git{..} => true, _ => false }
    }
}
