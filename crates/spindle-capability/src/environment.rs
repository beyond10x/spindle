//! The environment document: which implementations provide which capabilities.

use std::collections::BTreeMap;

/// The format an environment document declares.
pub const FORMAT: &str = "spindle-environment/1";

/// Where Spindle runs, and what it can provide there.
///
/// ```toml
/// format = "spindle-environment/1"
/// name = "laptop"
/// os = "linux"
/// arch = "x86_64"
///
/// [provides]
/// "store.durable" = ["sqlite", "memory"]
/// channel = ["in-process"]
/// "external.*" = ["operator-prompt"]
/// ```
///
/// Each list is in order of preference. A key ending in `.*` provides every capability under
/// that prefix, and an exact key wins over it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    /// Always [`FORMAT`].
    pub format: String,
    /// What the environment is called: `laptop`, `microvm-tcp`.
    pub name: String,
    /// The operating system, as Rust's `std::env::consts::OS` spells it.
    pub os: String,
    /// The architecture, as `std::env::consts::ARCH` spells it.
    pub arch: String,
    /// Capability (or `prefix.*`) to implementations, most preferred first.
    #[serde(default)]
    pub provides: BTreeMap<String, Vec<String>>,
}

/// Why an environment document was refused.
#[derive(Debug, thiserror::Error)]
pub enum EnvironmentError {
    /// Not TOML, or not this shape.
    #[error("not a spindle environment: {0}")]
    Parse(#[from] toml::de::Error),
    /// Another format, or none.
    #[error("format is {found:?}, expected {FORMAT:?}")]
    Format {
        /// What the document declared.
        found: String,
    },
    /// A capability listed with no implementation.
    #[error("{capability} lists no implementation; leave it out instead")]
    EmptyList {
        /// The capability.
        capability: String,
    },
}

impl Environment {
    /// Parses and checks an environment document.
    pub fn parse(text: &str) -> Result<Self, EnvironmentError> {
        let environment: Self = toml::from_str(text)?;
        if environment.format != FORMAT {
            return Err(EnvironmentError::Format {
                found: environment.format,
            });
        }
        if let Some((capability, _)) = environment
            .provides
            .iter()
            .find(|(_, implementations)| implementations.is_empty())
        {
            return Err(EnvironmentError::EmptyList {
                capability: capability.clone(),
            });
        }
        Ok(environment)
    }

    /// The implementations that provide `capability`, most preferred first: the exact key, or
    /// else the longest `prefix.*` key that covers it.
    pub fn implementations(&self, capability: &str) -> Option<&[String]> {
        if let Some(exact) = self.provides.get(capability) {
            return Some(exact);
        }
        self.provides
            .iter()
            .filter_map(|(key, implementations)| {
                let prefix = key.strip_suffix('*')?;
                (prefix.ends_with('.') && capability.starts_with(prefix))
                    .then_some((prefix.len(), implementations))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, implementations)| implementations.as_slice())
    }
}
