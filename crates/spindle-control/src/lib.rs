//! The control process of a deployment (`docs/design/observe.md` § 5): it places and launches the
//! instances, collects what they send, and serves the read-only observe API and the UI.
//!
//! # Launch
//!
//! [`Control::start`] takes a compiled specification, the environment its targets provide and a
//! [`PlacementDocument`]. Each instance is placed on its target as `spindle.placement.PlaceInstance`
//! decides ([`spindle_target::place`]: retired target, admission, ceiling) and, once placed,
//! launched through its target's [`Launcher`](spindle_target::Launcher) with the placed request,
//! its own loopback address to listen on and every other component's as its peers. This build
//! launches `Process` targets ([`spindle_target::ProcessLauncher`]); an instance on another kind is
//! not run and is named by [`Control::not_run`]. An instance of a component reached over the
//! network (one that requires `ingress.network`) is given a loopback address for its ingress as
//! the setting [`INGRESS_SETTING`].
//!
//! # Collect
//!
//! Every launched instance is given a Unix socket as its telemetry endpoint. Each
//! `spindle-telemetry/1` frame it sends becomes one `spindle.observe.Sample`, its resources
//! replaced by what the launcher reads of the instance on the host where it reads them
//! (`docs/design/observe.md` § 4). The last [`WINDOW`] of samples is kept per instance
//! ([`History`]). An instance that ends moves to `Exited`.
//!
//! # Serve
//!
//! On `127.0.0.1` only, every request checked for its `Host` header and a random token:
//!
//! | path | answer |
//! |---|---|
//! | `GET /api/deployment` | `spindle.observe.Deployment`: components and bindings from the IR, the instances of the placement with their target, request and state |
//! | `GET /api/catalogue?component=<c>` | `spindle.observe.Catalogue`, derived from the IR |
//! | `GET /api/stream` | Server-Sent Events: the held samples, then one `sample` per frame and one `state` per state change |
//! | `GET /api/views/<component>/<view>` | the view's answer from the ingress of a running instance of the component, as it answers |
//! | `GET /` | the UI, embedded at compile time from `ui/dist` |

mod answer;
mod control;
mod history;
mod hub;
mod placement;
mod server;
mod ui;

pub use answer::{
    Binding, BindingKind, Component, DeployedInstance, Deployment, InstanceState, StateChange,
};
pub use control::{Artifacts, Control, ControlError, INGRESS_SETTING, NotRun, Options};
pub use history::{Clock, History, SystemClock, WINDOW};
pub use placement::{FORMAT, InstanceEntry, PlacementDocument, PlacementError, TargetEntry};
pub use ui::embedded as ui_embedded;
