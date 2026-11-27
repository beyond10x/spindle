//! External causes the environment binds to an HTTP call: an `external.<command>.<outcome>`
//! capability whose admitted implementation names an `[http.<name>]` table of the environment
//! document is answered by one request to that call each time the kernel asks it.
//!
//! The request is a `spindle-external/1` JSON body ([`FORMAT`]): the capability, the command, the
//! outcome, the cause as the specification states it and the command's input. ESS 0.90 declares
//! no construct for an outbound request's payload: `external:` carries one phrase, `ess-http/1`
//! binds inbound routes and `ess-transport` binds events. A `2xx` reply whose body is
//! `{"holds": true | false}` answers the cause; a timeout, a transport failure, any other status
//! or any other body answers no cause, and the command fails as undecided and changes nothing. A
//! reply body is read up to [`REPLY_LIMIT`] bytes; a longer one is a failed call.
//!
//! A call blocks the thread that asks it, the one thread that runs every command of a served run,
//! until its reply or its timeout, which the environment document bounds at 30 s.
//!
//! The secret is read from the environment variable `secret_env` names, once, when the run
//! starts, and sent as `Authorization: Bearer <value>`; no document holds it and no message
//! prints it.
//!
//! A call leaves the host only over verified TLS. An `https://` URL is called over TLS (rustls,
//! `ring`), the server's certificate verified against the platform's trust roots, or against only
//! the PEM certificates of the table's `ca_file`; a certificate that does not verify fails the
//! call before a byte of the request is written. A plain `http://` URL is called only where its
//! host is loopback ([`spindle_capability::loopback_host`]); the environment document refuses any
//! other, and so does [`Calls::bind`].

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE, HOST, HeaderValue};
use hyper::{Method, Request, Uri};
use hyper_util::rt::TokioIo;
use rustls::RootCertStore;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use spindle_capability::{Binding, Capability, Environment, HttpCall, loopback_host};
use spindle_kernel::{Decider, External};
use tokio::io::{AsyncRead, AsyncWrite};

/// The format of the request body a call is sent.
pub(crate) const FORMAT: &str = "spindle-external/1";

/// The most bytes of a reply body a call reads: `{"holds": true}` is 15.
pub(crate) const REPLY_LIMIT: usize = 64 * 1024;

/// Each external cause admission bound to an `[http.<name>]` call, by capability.
#[derive(Clone, Default)]
pub(crate) struct Calls {
    bound: BTreeMap<String, Arc<Call>>,
    /// The runtime every call is sent on; `None` where nothing is bound.
    runtime: Option<Arc<tokio::runtime::Runtime>>,
}

/// Each bound capability with the name of its call; never a URL or a secret.
impl std::fmt::Debug for Calls {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_map()
            .entries(
                self.bound
                    .iter()
                    .map(|(capability, call)| (capability, &call.name)),
            )
            .finish()
    }
}

/// One `[http.<name>]` table, resolved: its URL parsed and its secret read. An external cause
/// asks it ([`Calls`]); an egress posts events to it ([`crate::egress`]).
pub(crate) struct Call {
    name: String,
    method: Method,
    /// Where to connect: `host:port`.
    address: String,
    /// The `Host` header: `host[:port]`, never userinfo.
    host: String,
    /// The request target: path and query.
    target: Uri,
    timeout: Duration,
    /// `Bearer <secret>`, where the table names a secret.
    authorization: Option<HeaderValue>,
    /// How an `https://` call verifies its server; `None` for a loopback `http://` call.
    tls: Option<Tls>,
}

/// An `https://` call's TLS client and the name the server's certificate must carry.
struct Tls {
    connector: tokio_rustls::TlsConnector,
    server: ServerName<'static>,
}

impl Calls {
    /// The calls `bindings` bind an external cause to on `environment`, each URL checked and
    /// each secret read now.
    ///
    /// # Errors
    /// A call's method or URL is not one this build sends (a plain `http://` URL off loopback
    /// among them), its secret's variable is not set, or its `ca_file` cannot be read or holds no
    /// certificate. No message repeats the URL, which may carry what the document should not.
    pub(crate) fn bind(environment: &Environment, bindings: &[Binding]) -> Result<Self> {
        let mut resolved: BTreeMap<&str, Arc<Call>> = BTreeMap::new();
        let mut bound = BTreeMap::new();
        let mut platform = None;
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
                    let call = Arc::new(Call::new(name, declared, &mut platform)?);
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
    /// `declared`, resolved: its URL parsed and checked, its secret read and, for an `https://`
    /// URL, its TLS client built. `platform` is the client verifying against the platform's
    /// roots, read on first need and shared by every call without a `ca_file`.
    pub(crate) fn new(
        name: &str,
        declared: &HttpCall,
        platform: &mut Option<Arc<rustls::ClientConfig>>,
    ) -> Result<Self> {
        let method = Method::from_bytes(declared.method.as_bytes()).with_context(|| {
            format!(
                "[http.{name}] method {:?} is not an HTTP method",
                declared.method
            )
        })?;
        let Ok(uri) = declared.url.parse::<Uri>() else {
            bail!("[http.{name}] url is not a URL");
        };
        let secure = match uri.scheme_str() {
            Some(scheme) if scheme.eq_ignore_ascii_case("https") => true,
            Some(scheme) if scheme.eq_ignore_ascii_case("http") => false,
            _ => {
                bail!("[http.{name}] url: this build calls https:// URLs, and http:// on loopback")
            }
        };
        let Some(authority) = uri.authority() else {
            bail!("[http.{name}] url names no host");
        };
        if authority.as_str().contains('@') {
            bail!("[http.{name}] url carries userinfo; a secret is named by secret_env");
        }
        if !secure && !loopback_host(authority.host()) {
            bail!(
                "[http.{name}] url is http:// to a host that is not loopback (localhost, \
                 127.0.0.0/8, [::1]): a call that leaves the host is made over https://"
            );
        }
        if !secure && declared.ca_file.is_some() {
            bail!("[http.{name}] names a ca_file, but its url is not https://");
        }
        let host = match authority.port() {
            Some(port) => format!("{}:{port}", authority.host()),
            None => authority.host().to_owned(),
        };
        let address = format!(
            "{}:{}",
            authority.host(),
            authority
                .port_u16()
                .unwrap_or(if secure { 443 } else { 80 })
        );
        let Ok(target) = uri
            .path_and_query()
            .map_or("/", |path| path.as_str())
            .parse::<Uri>()
        else {
            bail!("[http.{name}] url has no request path");
        };
        let tls = if secure {
            let bare = authority
                .host()
                .trim_start_matches('[')
                .trim_end_matches(']');
            let Ok(server) = ServerName::try_from(bare.to_owned()) else {
                bail!("[http.{name}] url's host is not a name a certificate can carry");
            };
            let verifying = match &declared.ca_file {
                Some(path) => client(file_roots(name, path)?)?,
                None => match platform {
                    Some(shared) => Arc::clone(shared),
                    None => {
                        let roots = platform_roots();
                        if roots.is_empty() {
                            bail!(
                                "[http.{name}] url is https://, and no trust root of the platform \
                                 could be read: name the CA that issued the server's certificate \
                                 in ca_file"
                            );
                        }
                        Arc::clone(platform.insert(client(roots)?))
                    }
                },
            };
            Some(Tls {
                connector: tokio_rustls::TlsConnector::from(verifying),
                server,
            })
        } else {
            None
        };
        let authorization = match declared.secret_env.as_deref() {
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
            host,
            target,
            timeout: Duration::from_millis(declared.timeout_ms),
            authorization,
            tls,
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
            .block_on(async {
                tokio::time::timeout(self.timeout, self.send(body, Reply::Read)).await
            })
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

    /// Sends `body`, a JSON text, within the call's timeout: `Ok` for a `2xx` status, the reason
    /// otherwise (a timeout, a failure, or the status answered). Decided on the status alone: the
    /// reply body is not read, however long or slow it is, and the connection is closed once the
    /// status arrived.
    pub(crate) fn post(
        &self,
        runtime: &tokio::runtime::Runtime,
        body: String,
    ) -> Result<(), String> {
        let (status, _) = runtime
            .block_on(async {
                tokio::time::timeout(self.timeout, self.send(body, Reply::Status)).await
            })
            .map_err(|_| {
                format!(
                    "the HTTP call `{}` did not reply within {} ms",
                    self.name,
                    self.timeout.as_millis()
                )
            })?
            .map_err(|why| format!("the HTTP call `{}` failed: {why}", self.name))?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(format!("the HTTP call `{}` answered {status}", self.name))
        }
    }

    /// One request on its own connection: the status and, as `reply` asks, the body of the reply.
    async fn send(&self, body: String, reply: Reply) -> Result<(u16, Bytes), String> {
        let stream = tokio::net::TcpStream::connect(&self.address)
            .await
            .map_err(|error| format!("connecting to {}: {error}", self.address))?;
        let Some(tls) = &self.tls else {
            return self.exchange(stream, body, reply).await;
        };
        let stream = tls
            .connector
            .connect(tls.server.clone(), stream)
            .await
            .map_err(|error| format!("TLS with {}: {error}", self.address))?;
        self.exchange(stream, body, reply).await
    }

    /// The request written on `stream`, once it is connected (and, for `https://`, verified).
    async fn exchange<S>(
        &self,
        stream: S,
        body: String,
        reply: Reply,
    ) -> Result<(u16, Bytes), String>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .map_err(|error| error.to_string())?;
        let connection = tokio::spawn(connection);
        let mut request = Request::builder()
            .method(self.method.clone())
            .uri(self.target.clone())
            .header(HOST, self.host.as_str())
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
        if reply == Reply::Status {
            // The body is never read: the connection goes with it, so a slow or endless body
            // holds nothing open.
            drop(response);
            connection.abort();
            return Ok((status, Bytes::new()));
        }
        let reply = Limited::new(response.into_body(), REPLY_LIMIT)
            .collect()
            .await
            .map_err(|error| {
                if error.is::<http_body_util::LengthLimitError>() {
                    format!("its reply is longer than {REPLY_LIMIT} bytes")
                } else {
                    error.to_string()
                }
            })?
            .to_bytes();
        Ok((status, reply))
    }
}

/// What a call reads of its reply.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reply {
    /// The status and the body, up to [`REPLY_LIMIT`]: what an external cause is answered from.
    Read,
    /// The status alone: what decides an egress post.
    Status,
}

/// A TLS client on `ring` verifying its server against `roots` only.
fn client(roots: RootCertStore) -> Result<Arc<rustls::ClientConfig>> {
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .context("choosing the TLS versions an HTTP call offers")?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok(Arc::new(config))
}

/// Every certificate of the PEM file `path`, the `ca_file` of the call `name`.
fn file_roots(name: &str, path: &str) -> Result<RootCertStore> {
    let unreadable = |error: &dyn std::fmt::Display| {
        anyhow::anyhow!("[http.{name}] ca_file {path} is not a PEM certificate file: {error}")
    };
    let mut roots = RootCertStore::empty();
    for certificate in CertificateDer::pem_file_iter(path).map_err(|error| unreadable(&error))? {
        let certificate = certificate.map_err(|error| unreadable(&error))?;
        roots.add(certificate).map_err(|error| unreadable(&error))?;
    }
    if roots.is_empty() {
        bail!("[http.{name}] ca_file {path} holds no certificate");
    }
    Ok(roots)
}

/// The platform's trust roots, each one rustls can read; one the platform holds but cannot be
/// read is left out, so a server it issued for is not verified.
fn platform_roots() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    roots
}
