//! Admission per component: every component of the system is admitted on its own.

use std::collections::BTreeMap;
use std::fmt;

use ess_compiler::EssIr;
use spindle_capability::{Admission, Binding, Capability, Environment};

/// What each component was admitted with, by component name.
pub type Placements = BTreeMap<String, Vec<Binding>>;

/// An environment refused at least one component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The environment's name.
    pub environment: String,
    /// Every refused component, with every capability it lacks, in capability order.
    pub missing: BTreeMap<String, Vec<Capability>>,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "refused on {}: no implementation for", self.environment)?;
        for (component, missing) in &self.missing {
            let names = missing
                .iter()
                .map(Capability::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            write!(f, "\n  {component}: {names}")?;
        }
        Ok(())
    }
}

/// Admits every component of `ir` against `environment`, each with the capabilities it needs
/// when it runs on its own ([`spindle_capability::derive`] with the component).
///
/// # Errors
/// A [`Refusal`] naming every component the environment cannot run and everything each lacks.
pub fn admit(ir: &EssIr, environment: &Environment) -> Result<Placements, Refusal> {
    let mut placements = Placements::new();
    let mut missing = BTreeMap::new();
    for component in ir.components().values() {
        let name = component.name.as_str();
        let Some(requirements) = spindle_capability::derive(ir, Some(name)) else {
            continue;
        };
        match spindle_capability::admit(&requirements, environment) {
            Admission::Admitted { bindings } => {
                placements.insert(name.to_owned(), bindings);
            }
            Admission::Refused { missing: lacks } => {
                missing.insert(name.to_owned(), lacks);
            }
        }
    }
    if missing.is_empty() {
        Ok(placements)
    } else {
        Err(Refusal {
            environment: environment.name.clone(),
            missing,
        })
    }
}
