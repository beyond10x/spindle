//! Deployment targets: where an instance of a component runs, and the port that starts it there.
//!
//! `spindle.placement` (`ess/domains/placement.yaml`) specifies the decision. A [`Target`] is a
//! place an instance can be launched (a host process, a microVM, a container), with the
//! environment it provides and the most resources one instance may take there. [`place`] decides
//! `spindle.placement.PlaceInstance`: an instance is refused when its target is retired, when the
//! target's environment does not admit its component, or when it asks for more than the target's
//! ceiling, and every reason that applies is named.
//!
//! A placed instance is handed to a [`Launcher`] as a [`LaunchPlan`]: its identity, its component,
//! its settings, the artifact to run, the resources it asked for, where it listens and where its
//! peers listen. Every target kind implements the same port, so the rest of Spindle never knows
//! whether an instance is a child process or a virtual machine. [`ProcessLauncher`] is the first
//! implementation and the reference for the others.
//!
//! A launcher enforces some resources and not others. A request for a resource it cannot enforce
//! is refused before anything starts ([`LaunchError::Unenforced`]); a zero request means no limit
//! was asked.

mod launcher;
mod placement;
mod process;
mod sample;

pub use launcher::{
    Artifact, Exit, LaunchError, LaunchPlan, Launcher, Peer, ResourceKind, Resources, Running,
    TargetKind, refuse_unenforced,
};
pub use placement::{
    EnvironmentAdmission, Placement, Refusal, StartError, Target, TargetState, place, start,
};
pub use process::{
    ENV_COMPONENT, ENV_INSTANCE_ID, ENV_LISTEN, ENV_PEER_PREFIX, ENV_SETTING_PREFIX,
    ProcessLauncher, env_name, launch_variables,
};
pub use sample::{HostUsage, ResourceSample, process_group_usage, process_usage};
