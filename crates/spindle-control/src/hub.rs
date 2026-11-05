//! What the control process knows, shared between the thread that supervises the instances and
//! the server: the deployment answer, the catalogues, the samples of the last [`WINDOW`] and the
//! open streams.
//!
//! [`WINDOW`]: crate::WINDOW

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use hyper::body::Bytes;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::answer::{Deployment, InstanceState, StateChange};
use crate::history::{Clock, History};

/// How many messages a stream may fall behind before further ones are dropped for it.
pub(crate) const STREAM_BACKLOG: usize = 4096;

struct Inner {
    deployment: Deployment,
    history: History,
    streams: Vec<mpsc::Sender<Bytes>>,
}

/// The shared state; every method takes the lock briefly and never waits on a stream.
pub(crate) struct Hub {
    inner: Mutex<Inner>,
    catalogues: BTreeMap<String, String>,
    clock: Arc<dyn Clock>,
}

impl Hub {
    pub(crate) fn new(
        deployment: Deployment,
        catalogues: BTreeMap<String, String>,
        window: Duration,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            inner: Mutex::new(Inner {
                deployment,
                history: History::new(window, Arc::clone(&clock)),
                streams: Vec::new(),
            }),
            catalogues,
            clock,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// The deployment answer as JSON.
    pub(crate) fn deployment(&self) -> String {
        serde_json::to_string(&self.lock().deployment).unwrap_or_default()
    }

    /// The component and state of `instance`.
    pub(crate) fn instance(&self, instance: &str) -> Option<(String, InstanceState)> {
        self.lock()
            .deployment
            .instances
            .iter()
            .find(|candidate| candidate.instance_id == instance)
            .map(|found| (found.component.clone(), found.state))
    }

    /// Whether the deployment declares `component`, and the views it declares.
    pub(crate) fn views(&self, component: &str) -> Option<Vec<String>> {
        self.lock()
            .deployment
            .components
            .iter()
            .find(|candidate| candidate.name == component)
            .map(|found| found.views.clone())
    }

    /// The catalogue of `component` as JSON.
    pub(crate) fn catalogue(&self, component: &str) -> Option<&str> {
        self.catalogues.get(component).map(String::as_str)
    }

    /// A new stream: every held sample first, instance by instance and oldest first, then every
    /// sample and state change as it happens.
    pub(crate) fn subscribe(&self) -> mpsc::Receiver<Bytes> {
        let (sender, receiver) = mpsc::channel(STREAM_BACKLOG);
        let mut inner = self.lock();
        for instance in inner.history.instances() {
            for sample in inner.history.samples(&instance) {
                let _ = sender.try_send(message("sample", &sample));
            }
        }
        inner.streams.push(sender);
        receiver
    }

    /// Keeps `sample` of `instance` and sends it to every stream.
    pub(crate) fn sample(&self, instance: &str, sample: Value) {
        let mut inner = self.lock();
        let bytes = message("sample", &sample);
        inner.history.push(instance, sample);
        broadcast(&mut inner, &bytes);
    }

    /// Moves `instance` to `to` and sends the change to every stream; nothing when it is already
    /// there. `started` records when it was launched.
    pub(crate) fn state(&self, instance: &str, to: InstanceState, started: Option<String>) {
        let at = spindle_telemetry::timestamp(self.clock.now());
        let mut inner = self.lock();
        let Some(found) = inner
            .deployment
            .instances
            .iter_mut()
            .find(|candidate| candidate.instance_id == instance)
        else {
            return;
        };
        if found.state == to {
            return;
        }
        let change = StateChange {
            instance_id: instance.to_owned(),
            from: found.state,
            to,
            at,
        };
        found.state = to;
        if started.is_some() {
            found.started_at = started;
        }
        let value = serde_json::to_value(&change).unwrap_or_default();
        broadcast(&mut inner, &message("state", &value));
    }
}

fn message(event: &str, data: &Value) -> Bytes {
    Bytes::from(format!("event: {event}\ndata: {data}\n\n"))
}

/// Sends `bytes` to every stream; a stream that has closed is forgotten, and one that has fallen
/// [`STREAM_BACKLOG`] messages behind misses this one.
fn broadcast(inner: &mut Inner, bytes: &Bytes) {
    inner
        .streams
        .retain(|stream| match stream.try_send(bytes.clone()) {
            Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => true,
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        });
}
