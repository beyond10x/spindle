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
    /// sample and state change as it happens. The channel holds every held sample and
    /// [`STREAM_BACKLOG`] more, so the replay loses none.
    pub(crate) fn subscribe(&self) -> mpsc::Receiver<Bytes> {
        let mut inner = self.lock();
        let held = inner
            .history
            .instances()
            .into_iter()
            .map(|instance| inner.history.samples(&instance))
            .collect::<Vec<_>>();
        let count = held.iter().map(Vec::len).sum::<usize>();
        let (sender, receiver) = mpsc::channel(count + STREAM_BACKLOG);
        for sample in held.iter().flatten() {
            let _ = sender.try_send(message("sample", sample));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{SystemClock, WINDOW};

    #[test]
    fn a_late_stream_is_replayed_every_held_sample() {
        let hub = Hub::new(
            Deployment {
                system: "s".to_owned(),
                components: Vec::new(),
                bindings: Vec::new(),
                instances: Vec::new(),
            },
            BTreeMap::new(),
            WINDOW,
            Arc::new(SystemClock),
        );
        let held = STREAM_BACKLOG + 1_000;
        for sequence in 0..held {
            let instance = if sequence % 2 == 0 { "a-0" } else { "b-0" };
            hub.sample(
                instance,
                serde_json::json!({ "instance_id": instance, "sequence": sequence }),
            );
        }
        let mut stream = hub.subscribe();
        let mut replayed = 0;
        while stream.try_recv().is_ok() {
            replayed += 1;
        }
        assert_eq!(
            replayed, held,
            "a stream opened late is told every held sample"
        );
        hub.sample(
            "a-0",
            serde_json::json!({ "instance_id": "a-0", "sequence": held }),
        );
        assert!(stream.try_recv().is_ok(), "and then the live ones");
    }
}
