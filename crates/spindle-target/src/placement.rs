//! Placement: the decision `spindle.placement.PlaceInstance` specifies.

use std::fmt;

use spindle_capability::{Admission, Capability, Environment, Requirements, admit};

use crate::{LaunchError, LaunchPlan, Launcher, ResourceKind, Resources, Running, TargetKind};

/// Where a target is in its lifecycle: `spindle.placement.Target.State`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TargetState {
    /// It takes instances.
    Available,
    /// It takes no further instance.
    Retired,
}

/// A place instances are launched: `spindle.placement.Target`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Target {
    /// `spindle.placement.TargetId`.
    pub id: String,
    /// What kind of target it is.
    pub kind: TargetKind,
    /// The environment it provides: `spindle.deployment.EnvironmentId`, the environment
    /// document's `name`.
    pub environment: String,
    /// The most one instance may take.
    pub ceiling: Resources,
    /// Its lifecycle state.
    pub state: TargetState,
}

/// An admission and the environment it was decided against.
///
/// Placement reads an admission only for a target that provides the same environment, so an
/// admission decided on a laptop never places an instance in a microVM.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EnvironmentAdmission {
    /// The environment document's `name`.
    pub environment: String,
    /// What admission decided there.
    pub admission: Admission,
}

impl EnvironmentAdmission {
    /// Admits `requirements` against `environment` and records which environment decided it.
    pub fn decide(requirements: &Requirements, environment: &Environment) -> Self {
        Self {
            environment: environment.name.clone(),
            admission: admit(requirements, environment),
        }
    }
}

/// One reason an instance was refused.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Refusal {
    /// The target is retired.
    TargetRetired,
    /// The admission was decided for another environment than the one the target provides.
    EnvironmentMismatch {
        /// The environment the target provides.
        target: String,
        /// The environment the admission was decided for.
        admitted: String,
    },
    /// The target's environment does not provide what the component needs.
    NotAdmitted {
        /// Every capability it is missing.
        missing: Vec<Capability>,
    },
    /// The request exceeds the target's ceiling for one resource.
    AboveCeiling {
        /// The resource.
        resource: ResourceKind,
        /// What the instance asked for.
        requested: u64,
        /// The most the target allows.
        ceiling: u64,
    },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetRetired => f.write_str("the target is retired"),
            Self::EnvironmentMismatch { target, admitted } => write!(
                f,
                "the admission was decided for environment {admitted}, and the target provides \
                 {target}"
            ),
            Self::NotAdmitted { missing } => {
                f.write_str("the target's environment does not provide")?;
                for (index, capability) in missing.iter().enumerate() {
                    f.write_str(if index == 0 { " " } else { ", " })?;
                    f.write_str(capability.as_str())?;
                }
                Ok(())
            }
            Self::AboveCeiling {
                resource,
                requested,
                ceiling,
            } => write!(
                f,
                "{resource} {requested} exceeds the target's ceiling of {ceiling}"
            ),
        }
    }
}

/// The outcome of placement.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Placement {
    /// The instance may be launched on the target.
    Placed {
        /// The request the launcher holds the instance to: what it asked for, with every
        /// resource it left at zero raised to the target's ceiling.
        request: Resources,
    },
    /// The instance is refused and never launched.
    Refused {
        /// Every reason that applies: the target's state, then the admission's environment, then
        /// admission, then each resource over the ceiling in [`ResourceKind::ALL`] order.
        reasons: Vec<Refusal>,
    },
}

impl Placement {
    /// Whether the instance may be launched.
    pub fn is_placed(&self) -> bool {
        matches!(self, Self::Placed { .. })
    }
}

/// Places an instance asking for `request` on `target`, with the admission its component got.
pub fn place(target: &Target, request: &Resources, admission: &EnvironmentAdmission) -> Placement {
    let mut reasons = Vec::new();
    if target.state == TargetState::Retired {
        reasons.push(Refusal::TargetRetired);
    }
    if admission.environment != target.environment {
        reasons.push(Refusal::EnvironmentMismatch {
            target: target.environment.clone(),
            admitted: admission.environment.clone(),
        });
    } else if let Admission::Refused { missing } = &admission.admission {
        reasons.push(Refusal::NotAdmitted {
            missing: missing.clone(),
        });
    }
    for resource in ResourceKind::ALL {
        let requested = request.get(resource);
        let ceiling = target.ceiling.get(resource);
        if requested > ceiling {
            reasons.push(Refusal::AboveCeiling {
                resource,
                requested,
                ceiling,
            });
        }
    }
    if reasons.is_empty() {
        Placement::Placed {
            request: resolve(request, &target.ceiling),
        }
    } else {
        Placement::Refused { reasons }
    }
}

/// `request` with each zero raised to `ceiling`, so a placed instance never runs unbounded on a
/// target that bounds it.
fn resolve(request: &Resources, ceiling: &Resources) -> Resources {
    fn or<T: PartialEq + Default>(asked: T, most: T) -> T {
        if asked == T::default() { most } else { asked }
    }
    Resources {
        vcpus: or(request.vcpus, ceiling.vcpus),
        memory_mib: or(request.memory_mib, ceiling.memory_mib),
        disk_mib: or(request.disk_mib, ceiling.disk_mib),
    }
}

/// Why [`start`] did not start an instance.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// Placement refused the instance; nothing was launched.
    #[error("refused: {}", .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
    Refused(Vec<Refusal>),
    /// The launcher is for another kind of target.
    #[error("the target is a {target} target and the launcher a {launcher} launcher")]
    WrongLauncher {
        /// The target's kind.
        target: TargetKind,
        /// The launcher's kind.
        launcher: TargetKind,
    },
    /// The launcher refused or failed.
    #[error(transparent)]
    Launch(#[from] LaunchError),
}

/// Places the instance `plan` describes on `target` and, only when it is placed, launches it
/// with the placed request.
pub fn start(
    launcher: &mut dyn Launcher,
    target: &Target,
    admission: &EnvironmentAdmission,
    plan: &LaunchPlan,
) -> Result<Box<dyn Running>, StartError> {
    if launcher.kind() != target.kind {
        return Err(StartError::WrongLauncher {
            target: target.kind,
            launcher: launcher.kind(),
        });
    }
    match place(target, &plan.request, admission) {
        Placement::Placed { request } => {
            let placed = LaunchPlan {
                request,
                ..plan.clone()
            };
            Ok(launcher.launch(&placed)?)
        }
        Placement::Refused { reasons } => Err(StartError::Refused(reasons)),
    }
}
