//! External causes the environment binds to an HTTP call: an `external.<command>.<outcome>`
//! capability whose admitted implementation names an `[http.<name>]` table of the environment
//! document is answered by one request to that call each time the kernel asks it.
//!
//! The request is a `spindle-external/1` JSON body ([`FORMAT`]): the capability, the command, the
//! outcome, the cause as the specification states it and the command's input. ESS 0.90 declares
//! no construct for an outbound request's payload: `external:` carries one phrase, `ess-http/1`
//! binds inbound routes and `ess-transport` binds events. A `2xx` reply whose body is
//! `{"holds": true | false}` answers the cause; a timeout, a transport failure, any other status
//! or any other body answers no cause, and the command fails as undecided and changes nothing.
//!
//! The secret is read from the environment variable `secret_env` names, once, when the run
//! starts, and sent as `Authorization: Bearer <value>`; no document holds it and no message
//! prints it. This build calls `http://` URLs only.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE, HOST, HeaderValue};
use hyper::{Method, Request, Uri};
use hyper_util::rt::TokioIo;
use spindle_capability::{Binding, Capability, Environment};
use spindle_kernel::{Decider, External};

/// The format of the request body a call is sent.
pub(crate) const FORMAT: &str = "spindle-external/1";

/// Each external cause admission bound to an `[http.<name>]` call, by capability.
#[derive(Clone, Default)]
pub(crate) struct Calls {
    bound: BTreeMap<String, Arc<Call>>,
    /// The runtime every call is sent on; `None` where nothing is bound.
    runtime: Option<Arc<tokio::runtime::Runtime>>,
}

/// One `[http.<name>]` table, resolved: its URL parsed and its secret read.
struct Call {
    name: String,
    method: Method,
    /// Where to connect: `host:port`.
    address: String,
    /// The `Host` header.
    authority: String,
    /// The request target: path and query.
    target: Uri,
    timeout: Duration,
    /// `Bearer <secret>`, where the table names a secret.
    authorization: Option<HeaderValue>,
}

impl Calls {
    /// The calls `bindings` bind an external cause to on `environment`, each URL checked and
    /// each secret read now.
    ///
    /// # Errors
    /// A call's method or URL is not one this build sends, or its secret's variable is not set.
    pub(crate) fn bind(environment: &Environment, bindings: &[Binding]) -> Result<Self> {
        let mut resolved: BTreeMap<&str, Arc<Call>> = BTreeMap::new();
        let mut bound = BTreeMap::new();
        for binding in bindings {
            let capability = binding.capability.as_str();
            if !capability.starts_with(Capability::EXTERNAL_PREFIX) {
                continue;
            }
            let name = binding.implementation.as_str();
            let Some(declared) = environment.http.get(name) else {
                continue;
            };
            let call = match resolved.get(name) {
                Some(call) => Arc::clone(call),
                None => {
                    let call = Arc::new(Call::new(
                        name,
                        &declared.method,
                        &declared.url,
                        declared.timeout_ms,
                        declared.secret_env.as_deref(),
                    )?);
                    resolved.insert(name, Arc::clone(&call));
                    call
                }
            };
            bound.insert(capability.to_owned(), call);
        }
        let runtime = if bound.is_empty() {
            None
        } else {
            Some(Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .enable_time()
                    .build()
                    .context("starting the runtime HTTP calls are sent on")?,
            ))
        };
        Ok(Self { bound, runtime })
    }

    /// Whether `bindings` bind at least one external cause, and each of them to a call.
    pub(crate) fn answer_every(&self, bindings: &[Binding]) -> bool {
        let mut external = bindings
            .iter()
            .map(|binding| binding.capability.as_str())
            .filter(|capability| capability.starts_with(Capability::EXTERNAL_PREFIX))
            .peekable();
        external.peek().is_some() && external.all(|capability| self.bound.contains_key(capability))
    }

    /// The answer of the call `question`'s capability is bound to; `None` where none is.
    pub(crate) fn answer(&self, question: &External<'_>) -> Option<Result<bool, String>> {
        let call = self.bound.get(&question.capability)?;
        let runtime = self.runtime.as_ref()?;
        Some(call.ask(runtime, question))
    }

    /// `decider`, with every cause bound to a call answered by that call first.
    pub(crate) fn over(&self, mut decider: Box<dyn Decider + Send>) -> Box<dyn Decider + Send> {
        if self.bound.is_empty() {
            return decider;
        }
        let calls = self.clone();
        Box::new(move |question: &External<'_>| {
            calls
                .answer(question)
                .unwrap_or_else(|| decider.decide(question))
        })
    }
}

impl Call {
    fn new(
        name: &str,
        method: &str,
        url: &str,
        timeout_ms: u64,
        secret_env: Option<&str>,
    ) -> Result<Self> {
        let method = Method::from_bytes(method.as_bytes())
            .with_context(|| format!("[http.{name}] method {method:?} is not an HTTP method"))?;
        let uri: Uri = url
            .parse()
            .with_context(|| format!("[http.{name}] url {url:?} is not a URL"))?;
        if uri.scheme_str() != Some("http") {
            bail!("[http.{name}] url {url:?}: this build calls http:// URLs only");
        }
        let Some(authority) = uri.authority() else {
            bail!("[http.{name}] url {url:?} names no host");
        };
        let address = format!(
            "{}:{}",
            authority.host(),
            authority.port_u16().unwrap_or(80)
        );
        let target = uri
            .path_and_query()
            .map_or("/", |path| path.as_str())
            .parse()
            .with_context(|| format!("[http.{name}] url {url:?} has no request path"))?;
        let authorization = match secret_env {
            None => None,
            Some(variable) => {
                let secret = std::env::var(variable).map_err(|_| {
                    anyhow::anyhow!(
                        "[http.{name}] names its secret by the environment variable {variable}, \
                         which is not set (or not UTF-8)"
                    )
                })?;
                let mut value =
                    HeaderValue::from_str(&format!("Bearer {secret}")).map_err(|_| {
                        anyhow::anyhow!(
                            "[http.{name}]: the value of {variable} cannot be sent in a header"
                        )
                    })?;
                value.set_sensitive(true);
                Some(value)
            }
        };
        Ok(Self {
            name: name.to_owned(),
            method,
            address,
            authority: authority.as_str().to_owned(),
            target,
            timeout: Duration::from_millis(timeout_ms),
            authorization,
        })
    }

    /// Sends `question` and reads whether the cause holds from the reply.
    fn ask(
        &self,
        runtime: &tokio::runtime::Runtime,
        question: &External<'_>,
    ) -> Result<bool, String> {
        let body = serde_json::json!({
            "format": FORMAT,
            "capability": question.capability,
            "command": question.command,
            "outcome": question.outcome,
            "cause": question.cause,
            "input": question.input,
        })
        .to_string();
        let called = format!(
            "`{}` is bound to the HTTP call `{}`",
            question.capability, self.name
        );
        let (status, reply) = runtime
            .block_on(async { tokio::time::timeout(self.timeout, self.send(body)).await })
            .map_err(|_| {
                format!(
                    "{called}, which did not reply within {} ms",
                    self.timeout.as_millis()
                )
            })?
            .map_err(|why| format!("{called}, which failed: {why}"))?;
        if !(200..300).contains(&status) {
            return Err(format!("{called}, which answered {status}"));
        }
        serde_json::from_slice::<serde_json::Value>(&reply)
            .ok()
            .and_then(|reply| reply.get("holds").and_then(serde_json::Value::as_bool))
            .ok_or_else(|| format!("{called}, whose reply is not {{\"holds\": true | false}}"))
    }

    /// One request on its own connection: the status and the body of the reply.
    async fn send(&self, body: String) -> Result<(u16, Bytes), String> {
        let stream = tokio::net::TcpStream::connect(&self.address)
            .await
            .map_err(|error| format!("connecting to {}: {error}", self.address))?;
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .map_err(|error| error.to_string())?;
        tokio::spawn(connection);
        let mut request = Request::builder()
            .method(self.method.clone())
            .uri(self.target.clone())
            .header(HOST, self.authority.as_str())
            .header(CONTENT_TYPE, "application/json");
        if let Some(authorization) = &self.authorization {
            request = request.header(AUTHORIZATION, authorization.clone());
        }
        let request = request
            .body(Full::new(Bytes::from(body)))
            .map_err(|error| error.to_string())?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let reply = response
            .into_body()
            .collect()
            .await
            .map_err(|error| error.to_string())?
            .to_bytes();
        Ok((status, reply))
    }
}
