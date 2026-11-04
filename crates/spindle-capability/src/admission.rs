//! Admission: bind every requirement, or refuse and name what is missing.
//!
//! The decision `spindle.deployment.DecideAdmission` specifies: `admitted` carries the bindings,
//! `refused` carries every missing capability, never only the first.

use crate::{Capability, Environment, Requirements};

/// One capability bound to the implementation that will provide it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Binding {
    /// The capability.
    pub capability: Capability,
    /// The implementation chosen for it.
    pub implementation: String,
}

/// The outcome of admission.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Admission {
    /// Every requirement has an implementation.
    Admitted {
        /// One per required capability, in capability order.
        bindings: Vec<Binding>,
    },
    /// At least one requirement has none.
    Refused {
        /// Every required capability the environment does not provide, in capability order.
        missing: Vec<Capability>,
    },
}

impl Admission {
    /// Whether the deployment may start.
    pub fn is_admitted(&self) -> bool {
        matches!(self, Self::Admitted { .. })
    }
}

/// Admits `requirements` against `environment`, taking each capability's most preferred
/// implementation.
pub fn admit(requirements: &Requirements, environment: &Environment) -> Admission {
    let mut bindings = Vec::with_capacity(requirements.len());
    let mut missing = Vec::new();
    for capability in requirements.keys() {
        match environment
            .implementations(capability.as_str())
            .and_then(<[String]>::first)
        {
            Some(implementation) => bindings.push(Binding {
                capability: capability.clone(),
                implementation: implementation.clone(),
            }),
            None => missing.push(capability.clone()),
        }
    }
    if missing.is_empty() {
        Admission::Admitted { bindings }
    } else {
        Admission::Refused { missing }
    }
}
