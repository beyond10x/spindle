//! The placement document `deploy --placement` reads: `spindle-placement/1`, TOML.
//!
//! It declares the targets of one deployment and the instances planned onto them, the nouns of
//! `spindle.placement` (`ess/domains/placement.yaml`): a target with its id, kind, environment and
//! ceiling, and an instance with its id, component, target, request and settings.
//!
//! ```toml
//! format = "spindle-placement/1"
//!
//! [[targets]]
//! id = "laptop"
//! kind = "Process"
//! environment = "laptop"
//!
//! [[instances]]
//! id = "rally-table-0"
//! component = "rally-table"
//! target = "laptop"
//! ```
//!
//! A ceiling or request left out is zero in every resource; settings are strings.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use spindle_target::{Resources, TargetKind};

/// The value of the document's `format`.
pub const FORMAT: &str = "spindle-placement/1";

/// A `spindle-placement/1` document.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementDocument {
    /// Always [`FORMAT`].
    pub format: String,
    /// The targets, in document order.
    #[serde(default)]
    pub targets: Vec<TargetEntry>,
    /// The instances, in document order.
    #[serde(default)]
    pub instances: Vec<InstanceEntry>,
}

/// One target: `spindle.placement.Target`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetEntry {
    /// `spindle.placement.TargetId`.
    pub id: String,
    /// Its kind, as the specification names it (`Process`, `QemuMicrovm`, ...).
    pub kind: TargetKind,
    /// The environment it provides: the environment document's `name`.
    pub environment: String,
    /// The most one instance may take there.
    #[serde(default)]
    pub ceiling: Resources,
}

/// One instance: `spindle.placement.Instance`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceEntry {
    /// `spindle.placement.InstanceId`.
    pub id: String,
    /// The component it runs.
    pub component: String,
    /// The target it is planned onto.
    pub target: String,
    /// What it asks for.
    #[serde(default)]
    pub request: Resources,
    /// Its settings, by name.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

/// Why a placement document is refused.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlacementError {
    /// The text is not a placement document.
    #[error("not a {FORMAT} document: {0}")]
    Malformed(String),
    /// The document names another format.
    #[error("format {0:?} is not {FORMAT}")]
    Format(String),
    /// Two targets or two instances share an id.
    #[error("{kind} id {id:?} is declared twice")]
    Duplicate {
        /// `target` or `instance`.
        kind: &'static str,
        /// The id.
        id: String,
    },
    /// An instance names a target the document does not declare.
    #[error("instance {instance:?} names target {target:?}, which is not declared")]
    UnknownTarget {
        /// The instance.
        instance: String,
        /// The target it names.
        target: String,
    },
}

impl PlacementDocument {
    /// Reads `text`, refusing an unknown field, another format, a duplicate id or an instance on an
    /// undeclared target.
    pub fn parse(text: &str) -> Result<Self, PlacementError> {
        let document: Self =
            toml::from_str(text).map_err(|error| PlacementError::Malformed(error.to_string()))?;
        if document.format != FORMAT {
            return Err(PlacementError::Format(document.format));
        }
        let mut targets = BTreeSet::new();
        for target in &document.targets {
            if !targets.insert(target.id.as_str()) {
                return Err(PlacementError::Duplicate {
                    kind: "target",
                    id: target.id.clone(),
                });
            }
        }
        let mut instances = BTreeSet::new();
        for instance in &document.instances {
            if !instances.insert(instance.id.as_str()) {
                return Err(PlacementError::Duplicate {
                    kind: "instance",
                    id: instance.id.clone(),
                });
            }
            if !targets.contains(instance.target.as_str()) {
                return Err(PlacementError::UnknownTarget {
                    instance: instance.id.clone(),
                    target: instance.target.clone(),
                });
            }
        }
        Ok(document)
    }

    /// The target `id`.
    pub fn target(&self, id: &str) -> Option<&TargetEntry> {
        self.targets.iter().find(|target| target.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instance_on_an_undeclared_target_is_refused() {
        let text = "format = \"spindle-placement/1\"\n[[instances]]\nid = \"a-0\"\ncomponent = \"a\"\ntarget = \"t\"\n";
        assert_eq!(
            PlacementDocument::parse(text),
            Err(PlacementError::UnknownTarget {
                instance: "a-0".to_owned(),
                target: "t".to_owned()
            })
        );
    }

    #[test]
    fn another_format_is_refused() {
        assert_eq!(
            PlacementDocument::parse("format = \"spindle-placement/2\"\n"),
            Err(PlacementError::Format("spindle-placement/2".to_owned()))
        );
    }
}
