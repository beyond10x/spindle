//! `run --caller-token <FILE>` (`story:verified-caller-token`): each request served proves its
//! caller by a signed token, `Authorization: Bearer <JWT>`, which the run verifies itself.
//!
//! The file is a TOML mapping: the token's `issuer` and `audience`; `jwks`, the URL of the
//! issuer's key set (JWKS), `https://`, or `http://` to a loopback host only, as an HTTP call of
//! the environment is (`calls.rs`), with an optional `ca_file` of PEM roots for an `https://` one;
//! `algorithms`, the signature algorithms a token may name, asymmetric only (`RS256` by default);
//! `leeway_seconds`, the clock skew allowed on `exp` and `nbf` (60 by default, at most 300); and
//! `refresh_seconds`, how often the key set is fetched again (300 by default, 1 to 86400). Then
//! `[actor]`: `claim`, the claim naming the caller's role, and `[actor.values]`, each value of it
//! mapped to a qualified actor of the model; and `[attributes."<actor>"]`, each attribute of that
//! actor mapped to the top-level claim it is read from.
//!
//! A token verifies where its header names an algorithm of the mapping and the id (`kid`) of a
//! key of the set of that algorithm's family (an RSA key for `RS*` and `PS*`, an EC key on the
//! algorithm's curve for `ES*`, an Ed25519 key for `EdDSA`), its signature verifies with that key,
//! its `iss` and `aud` are the mapping's and its `exp` has not passed. `alg: none`, a symmetric
//! algorithm and a key the set lists for encryption verify nothing. The token's claims then map
//! to a caller ([`Verifier::mapped`]); one whose role claim the mapping does not name maps to
//! none, and the listener answers it as a request authenticated as no caller.
//!
//! The mapping is checked against the model first: an actor or attribute the specification does
//! not declare stops the run. The key set is fetched before the run listens; one that cannot be
//! fetched, or lists no key a token could be verified with, stops the run. A thread of its own
//! fetches it again every `refresh_seconds`: a set it fetches replaces the keys, one listing no
//! usable key too, which then refuses every token and writes a warning line; a refresh that fails
//! keeps the last set fetched and writes one warning line.
//! No token or claim is ever printed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use ess_compiler::EssIr;
use ess_compiler::ir::ResolvedTypeRef;
use http_body_util::{BodyExt, Empty, Limited};
use hyper::body::Bytes;
use hyper::header::{ACCEPT, HOST};
use hyper::{Request, Uri};
use hyper_util::rt::TokioIo;
use jsonwebtoken::jwk::{AlgorithmParameters, EllipticCurve, Jwk, PublicKeyUse};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rustls::pki_types::ServerName;
use serde::Deserialize;
use serde_json::{Map, Value};
use spindle_capability::loopback_host;
use spindle_ingress::{Caller, CallerToken, Unverified};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::log;

/// The most bytes of a key set read.
const JWKS_LIMIT: usize = 1024 * 1024;

/// How long one fetch of the key set may take.
const FETCH: Duration = Duration::from_secs(10);

/// The signature algorithm a mapping allows where it names none: OpenID Connect's default.
const DEFAULT_ALGORITHM: &str = "RS256";

/// The clock skew allowed on `exp` and `nbf` where the mapping names none, and the most it may.
const DEFAULT_LEEWAY: u64 = 60;
const MOST_LEEWAY: u64 = 300;

/// How often the key set is fetched again where the mapping names no period, and the bounds.
const DEFAULT_REFRESH: u64 = 300;
const MOST_REFRESH: u64 = 86_400;

/// The mapping file as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    issuer: String,
    audience: String,
    jwks: String,
    ca_file: Option<PathBuf>,
    algorithms: Option<Vec<String>>,
    leeway_seconds: Option<u64>,
    refresh_seconds: Option<u64>,
    actor: ActorFile,
    #[serde(default)]
    attributes: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActorFile {
    claim: String,
    values: BTreeMap<String, String>,
}

/// The family of keys an algorithm verifies with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Rsa,
    /// An EC key on this curve.
    Ec(Curve),
    Ed25519,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Curve {
    P256,
    P384,
}

/// The family `algorithm` verifies with; `None` for a symmetric one.
fn family(algorithm: Algorithm) -> Option<Family> {
    match algorithm {
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => None,
        Algorithm::RS256
        | Algorithm::RS384
        | Algorithm::RS512
        | Algorithm::PS256
        | Algorithm::PS384
        | Algorithm::PS512 => Some(Family::Rsa),
        Algorithm::ES256 => Some(Family::Ec(Curve::P256)),
        Algorithm::ES384 => Some(Family::Ec(Curve::P384)),
        Algorithm::EdDSA => Some(Family::Ed25519),
    }
}

/// The name a JWT header or a JWK gives `algorithm`.
fn named(algorithm: Algorithm) -> String {
    format!("{algorithm:?}")
}

/// The URL of the key set, checked: where to connect, the `Host` header, the request target and,
/// for `https://`, the name the server's certificate must carry.
#[derive(Debug, Clone)]
struct Url {
    text: String,
    address: String,
    host: String,
    target: Uri,
    /// The server name of an `https://` URL; `None` for a loopback `http://` one.
    server: Option<ServerName<'static>>,
}

impl Url {
    /// `text` as a key set's URL: `https://`, or `http://` to a loopback host, without userinfo.
    fn parse(label: &str, text: &str) -> Result<Self> {
        let Ok(uri) = text.parse::<Uri>() else {
            bail!("{label}: jwks {text} is not a URL");
        };
        let secure = match uri.scheme_str() {
            Some(scheme) if scheme.eq_ignore_ascii_case("https") => true,
            Some(scheme) if scheme.eq_ignore_ascii_case("http") => false,
            _ => bail!(
                "{label}: jwks {text} is neither https:// nor http:// to a loopback host: the key \
                 set is fetched over verified TLS unless it is served on this host"
            ),
        };
        let Some(authority) = uri.authority() else {
            bail!("{label}: jwks {text} names no host");
        };
        if authority.as_str().contains('@') {
            bail!("{label}: jwks carries userinfo; a key set is public and needs no credential");
        }
        if !secure && !loopback_host(authority.host()) {
            bail!(
                "{label}: jwks {text} is http:// to a host that is not loopback (localhost, \
                 127.0.0.0/8, [::1]): a key set fetched from another host is fetched over https://"
            );
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
            bail!("{label}: jwks {text} has no request path");
        };
        let server = if secure {
            let bare = authority
                .host()
                .trim_start_matches('[')
                .trim_end_matches(']');
            let Ok(server) = ServerName::try_from(bare.to_owned()) else {
                bail!("{label}: jwks {text}: its host is not a name a certificate can carry");
            };
            Some(server)
        } else {
            None
        };
        Ok(Self {
            text: text.to_owned(),
            address,
            host,
            target,
            server,
        })
    }
}

/// A `--caller-token` mapping file, read and checked.
#[derive(Debug)]
pub(crate) struct Mapping {
    /// `--caller-token <FILE>`, as every message names it.
    label: String,
    issuer: String,
    audience: String,
    jwks: Url,
    ca_file: Option<PathBuf>,
    algorithms: Vec<Algorithm>,
    leeway: u64,
    refresh: Duration,
    actor_claim: String,
    actors: BTreeMap<String, String>,
    attributes: BTreeMap<String, BTreeMap<String, String>>,
}

impl Mapping {
    /// The mapping file at `path`.
    ///
    /// # Errors
    /// A file that cannot be read or is not such a mapping ([`Mapping::parse`]).
    pub(crate) fn read(path: &Path) -> Result<Self> {
        let label = format!("--caller-token {}", path.display());
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {label}"))?;
        Self::parse(&label, &text)
    }

    /// The mapping `text` holds, every message naming `label`.
    ///
    /// # Errors
    /// Text that is not TOML of the mapping's keys, an empty issuer, audience or role claim, a key
    /// set URL neither `https://` nor `http://` to a loopback host, a `ca_file` beside an
    /// `http://` one, an algorithm that is unknown or symmetric, a leeway or refresh period out of
    /// bounds, no role mapped, or attributes for an actor no role maps to.
    fn parse(label: &str, text: &str) -> Result<Self> {
        let file: File = toml::from_str(text).with_context(|| format!("in {label}"))?;
        if file.issuer.is_empty() || file.audience.is_empty() {
            bail!("{label}: issuer and audience must each name one value");
        }
        let jwks = Url::parse(label, &file.jwks)?;
        if jwks.server.is_none() && file.ca_file.is_some() {
            bail!("{label}: names a ca_file, but its jwks is not https://");
        }
        let names = file
            .algorithms
            .unwrap_or_else(|| vec![DEFAULT_ALGORITHM.to_owned()]);
        if names.is_empty() {
            bail!("{label}: algorithms names none, so no token could verify");
        }
        let mut algorithms = Vec::new();
        for name in &names {
            let Ok(algorithm) = Algorithm::from_str(name) else {
                bail!(
                    "{label}: algorithms names {name}, which is no JWS signature algorithm this \
                     build verifies (RS256, RS384, RS512, PS256, PS384, PS512, ES256, ES384, EdDSA)"
                );
            };
            if family(algorithm).is_none() {
                bail!(
                    "{label}: algorithms names {name}, a symmetric algorithm: a token is verified \
                     against the issuer's public keys only, never a shared secret"
                );
            }
            algorithms.push(algorithm);
        }
        let leeway = file.leeway_seconds.unwrap_or(DEFAULT_LEEWAY);
        if leeway > MOST_LEEWAY {
            bail!("{label}: leeway_seconds is {leeway}, more than {MOST_LEEWAY}");
        }
        let refresh = file.refresh_seconds.unwrap_or(DEFAULT_REFRESH);
        if !(1..=MOST_REFRESH).contains(&refresh) {
            bail!("{label}: refresh_seconds is {refresh}, outside 1 to {MOST_REFRESH}");
        }
        if file.actor.claim.is_empty() {
            bail!("{label}: [actor] claim names no claim");
        }
        if file.actor.values.is_empty() {
            bail!("{label}: [actor.values] maps no value to an actor, so no token proves a caller");
        }
        if let Some(actor) = file
            .attributes
            .keys()
            .find(|actor| !file.actor.values.values().any(|mapped| mapped == *actor))
        {
            bail!(
                "{label}: [attributes.\"{actor}\"] maps attributes for an actor no value of \
                 [actor.values] names"
            );
        }
        Ok(Self {
            label: label.to_owned(),
            issuer: file.issuer,
            audience: file.audience,
            jwks,
            ca_file: file.ca_file,
            algorithms,
            leeway,
            refresh: Duration::from_secs(refresh),
            actor_claim: file.actor.claim,
            actors: file.actor.values,
            attributes: file.attributes,
        })
    }

    /// Refuses a mapping that names what the model `ir` does not declare: an actor a role maps to,
    /// or an attribute of an actor (`story:verified-caller-token`, F3), or that maps a required
    /// attribute of a mapped actor from no claim (G3), so no token maps to a caller the model
    /// could never bind.
    ///
    /// # Errors
    /// The first such actor or attribute, the message naming the file.
    pub(crate) fn check(&self, ir: &EssIr) -> Result<()> {
        let declared = |actor: &str| {
            ir.actors()
                .iter()
                .find_map(|(name, declared)| (name.to_string() == actor).then_some(declared))
        };
        for (role, actor) in &self.actors {
            if declared(actor).is_none() {
                bail!(
                    "{}: [actor.values] maps {role} to the actor {actor}, which the specification \
                     does not declare",
                    self.label
                );
            }
        }
        for (actor, attributes) in &self.attributes {
            let Some(declared) = declared(actor) else {
                bail!(
                    "{}: [attributes.\"{actor}\"] names an actor the specification does not \
                     declare",
                    self.label
                );
            };
            if let Some(attribute) = attributes.keys().find(|attribute| {
                !declared
                    .attributes
                    .iter()
                    .any(|field| field.name.as_str() == attribute.as_str())
            }) {
                bail!(
                    "{}: [attributes.\"{actor}\"] maps the attribute {attribute}, which the actor \
                     {actor} does not declare",
                    self.label
                );
            }
        }
        for (role, actor) in &self.actors {
            let Some(declared) = declared(actor) else {
                continue;
            };
            // A required attribute mapped from no claim leaves every token of this role no
            // caller (`story:verified-caller-token`, G3).
            let mapped = self.attributes.get(actor);
            if let Some(required) = declared.attributes.iter().find(|field| {
                !matches!(field.type_ref, ResolvedTypeRef::Optional { .. })
                    && !mapped.is_some_and(|mapped| mapped.contains_key(field.name.as_str()))
            }) {
                bail!(
                    "{}: [actor.values] maps {role} to the actor {actor}, whose required \
                     attribute {} [attributes.\"{actor}\"] maps from no claim",
                    self.label,
                    required.name
                );
            }
        }
        Ok(())
    }

    /// The caller `claims` map to: the actor its role claim's value names, each attribute of it
    /// from the claim the mapping names, an absent claim left out. `None` where the role claim is
    /// absent, not a string, or a value the mapping does not name.
    fn mapped(&self, claims: &Map<String, Value>) -> Option<Caller> {
        let role = claims.get(&self.actor_claim)?.as_str()?;
        let actor = self.actors.get(role)?;
        let mut attributes = Map::new();
        for (attribute, claim) in self.attributes.get(actor).into_iter().flatten() {
            if let Some(value) = claims.get(claim) {
                attributes.insert(attribute.clone(), value.clone());
            }
        }
        Some(Caller {
            actor: actor.clone(),
            attributes,
        })
    }
}

/// One key of the set: its id, the family it verifies, the algorithm it names, if any, and the
/// key.
struct Key {
    kid: String,
    family: Family,
    algorithm: Option<String>,
    decoding: DecodingKey,
}

impl Key {
    /// `jwk` as a key a token may name; `None` for one without an id, one for encryption, a
    /// symmetric one, or one on a curve this build does not verify.
    fn of(jwk: &Jwk) -> Option<Self> {
        let kid = jwk.common.key_id.clone()?;
        if matches!(
            jwk.common.public_key_use,
            Some(PublicKeyUse::Encryption | PublicKeyUse::Other(_))
        ) {
            return None;
        }
        let family = match &jwk.algorithm {
            AlgorithmParameters::RSA(_) => Family::Rsa,
            AlgorithmParameters::EllipticCurve(params) => match params.curve {
                EllipticCurve::P256 => Family::Ec(Curve::P256),
                EllipticCurve::P384 => Family::Ec(Curve::P384),
                EllipticCurve::P521 | EllipticCurve::Ed25519 => return None,
            },
            AlgorithmParameters::OctetKeyPair(params) => match params.curve {
                EllipticCurve::Ed25519 => Family::Ed25519,
                _ => return None,
            },
            AlgorithmParameters::OctetKey(_) => return None,
        };
        let algorithm = jwk
            .common
            .key_algorithm
            .map(|algorithm| format!("{algorithm:?}"));
        let decoding = DecodingKey::from_jwk(jwk).ok()?;
        Some(Self {
            kid,
            family,
            algorithm,
            decoding,
        })
    }

    /// Whether this key verifies a token whose header names `kid` and `algorithm`.
    fn verifies(&self, kid: &str, algorithm: Algorithm) -> bool {
        self.kid == kid
            && family(algorithm) == Some(self.family)
            && self
                .algorithm
                .as_ref()
                .is_none_or(|named_by_key| *named_by_key == named(algorithm))
    }
}

/// The keys of one fetch of the set.
struct Keys(Vec<Key>);

impl Keys {
    /// The keys of the JWKS `body`: every key of `keys` a token may name; one that does not
    /// parse, or that [`Key::of`] leaves out, is skipped, so the set may hold none
    /// ([`Keys::unusable`]).
    ///
    /// # Errors
    /// A body that is not a JSON object with a `keys` array.
    fn read(body: &[u8]) -> Result<Self, String> {
        let set: Value = serde_json::from_slice(body)
            .map_err(|error| format!("its answer is not JSON: {error}"))?;
        let Some(listed) = set.get("keys").and_then(Value::as_array) else {
            return Err("its answer is no JWKS, a JSON object with a keys array".to_owned());
        };
        Ok(Self(
            listed
                .iter()
                .filter_map(|jwk| serde_json::from_value::<Jwk>(jwk.clone()).ok())
                .filter_map(|jwk| Key::of(&jwk))
                .collect(),
        ))
    }

    /// Why no token can verify against this set, where it lists no key a token may name.
    fn unusable(&self) -> Option<&'static str> {
        self.0
            .is_empty()
            .then_some("it lists no signing key with a key id (kid) this build verifies")
    }
}

/// What fetches the key set: its URL, and the TLS client of an `https://` one.
struct Fetcher {
    url: Url,
    tls: Option<tokio_rustls::TlsConnector>,
    runtime: tokio::runtime::Runtime,
}

impl Fetcher {
    fn new(mapping: &Mapping) -> Result<Self> {
        let tls = match (&mapping.jwks.server, &mapping.ca_file) {
            (None, _) => None,
            (Some(_), Some(path)) => Some(crate::calls::client(crate::calls::file_roots(
                &mapping.label,
                &path.display().to_string(),
            )?)?),
            (Some(_), None) => {
                let roots = crate::calls::platform_roots();
                if roots.is_empty() {
                    bail!(
                        "{}: jwks is https://, and no trust root of the platform could be read: \
                         name the CA that issued the server's certificate in ca_file",
                        mapping.label
                    );
                }
                Some(crate::calls::client(roots)?)
            }
        }
        .map(tokio_rustls::TlsConnector::from);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .context("starting the runtime the key set is fetched on")?;
        Ok(Self {
            url: mapping.jwks.clone(),
            tls,
            runtime,
        })
    }

    /// The key set, fetched now.
    fn fetch(&self) -> Result<Keys, String> {
        let body = self
            .runtime
            .block_on(async { tokio::time::timeout(FETCH, self.get()).await })
            .map_err(|_| format!("it did not answer within {} s", FETCH.as_secs()))??;
        Keys::read(&body)
    }

    async fn get(&self) -> Result<Bytes, String> {
        let stream = tokio::net::TcpStream::connect(&self.url.address)
            .await
            .map_err(|error| format!("connecting to {}: {error}", self.url.address))?;
        match (&self.tls, &self.url.server) {
            (Some(tls), Some(server)) => {
                let stream = tls
                    .connect(server.clone(), stream)
                    .await
                    .map_err(|error| format!("TLS with {}: {error}", self.url.address))?;
                self.exchange(stream).await
            }
            _ => self.exchange(stream).await,
        }
    }

    async fn exchange<S>(&self, stream: S) -> Result<Bytes, String>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .map_err(|error| error.to_string())?;
        tokio::spawn(connection);
        let request = Request::get(self.url.target.clone())
            .header(HOST, self.url.host.as_str())
            .header(ACCEPT, "application/json")
            .body(Empty::<Bytes>::new())
            .map_err(|error| error.to_string())?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(format!("it answered {status}"));
        }
        Limited::new(response.into_body(), JWKS_LIMIT)
            .collect()
            .await
            .map(http_body_util::Collected::to_bytes)
            .map_err(|error| {
                if error.is::<http_body_util::LengthLimitError>() {
                    format!("its answer is longer than {JWKS_LIMIT} bytes")
                } else {
                    error.to_string()
                }
            })
    }
}

/// The verifier of a token run: the mapping, and the keys of the last fetch of the set.
pub(crate) struct Verifier {
    mapping: Mapping,
    keys: RwLock<Keys>,
    /// The model each mapped caller is bound against: a caller it does not bind is no caller
    /// (`story:verified-caller-token`, G1).
    ir: EssIr,
}

impl Verifier {
    /// The verifier of the mapping file at `path`, checked against the model `ir`
    /// ([`Mapping::check`]), its key set fetched now and again every `refresh_seconds` on a thread
    /// of its own.
    ///
    /// A refresh whose answer is a JWKS replaces the keys, also one that lists no key a token may
    /// name, after which every token is refused until a refresh lists one, with a warning line
    /// naming the URL: an issuer that removes its last key revokes it. A refresh that fails, or
    /// whose answer is no JWKS, keeps the keys fetched last and writes a warning line.
    ///
    /// # Errors
    /// A mapping file [`Mapping::read`] or [`Mapping::check`] refuses, or a key set that cannot be
    /// fetched now or lists no key a token may name, the message naming its URL.
    pub(crate) fn start(path: &Path, ir: &EssIr) -> Result<Arc<Self>> {
        let mapping = Mapping::read(path)?;
        mapping.check(ir)?;
        let fetcher = Fetcher::new(&mapping)?;
        let keys = fetcher
            .fetch()
            .and_then(|keys| match keys.unusable() {
                Some(why) => Err(why.to_owned()),
                None => Ok(keys),
            })
            .map_err(|why| {
                anyhow::anyhow!(
                    "{}: the key set at {} could not be fetched: {why}",
                    mapping.label,
                    mapping.jwks.text
                )
            })?;
        let verifier = Arc::new(Self {
            mapping,
            keys: RwLock::new(keys),
            ir: ir.clone(),
        });
        let refreshed = Arc::clone(&verifier);
        std::thread::Builder::new()
            .name("caller-token-jwks".to_owned())
            .spawn(move || {
                loop {
                    std::thread::sleep(refreshed.mapping.refresh);
                    match fetcher.fetch() {
                        Ok(keys) => {
                            if let Some(why) = keys.unusable() {
                                log::warn(format!(
                                    "{}: the key set at {} was refreshed, and {why}: every token \
                                     is refused until a refresh lists one",
                                    refreshed.mapping.label, refreshed.mapping.jwks.text
                                ))
                                .write();
                            }
                            *refreshed
                                .keys
                                .write()
                                .unwrap_or_else(PoisonError::into_inner) = keys;
                        }
                        Err(why) => log::warn(format!(
                            "{}: the key set at {} could not be refreshed: {why}; the keys \
                             fetched last still verify",
                            refreshed.mapping.label, refreshed.mapping.jwks.text
                        ))
                        .write(),
                    }
                }
            })
            .context("starting the thread that refreshes the key set")?;
        Ok(verifier)
    }

    /// The line `run` prints before listening: the file, the key set, the issuer and audience,
    /// how often the set is refreshed and the role claim; never a key or a token.
    pub(crate) fn describe(&self) -> String {
        let mapping = &self.mapping;
        format!(
            "each request's caller is read from its bearer token ({}), verified against the key \
             set at {}, refreshed every {} s: signed with {}, issuer {}, audience {}; the actor is \
             the claim {} maps to",
            mapping.label,
            mapping.jwks.text,
            mapping.refresh.as_secs(),
            mapping
                .algorithms
                .iter()
                .map(|algorithm| named(*algorithm))
                .collect::<Vec<_>>()
                .join(", "),
            mapping.issuer,
            mapping.audience,
            mapping.actor_claim
        )
    }

    /// The claims of `token`, once it verifies: an algorithm of the mapping, a key of the set of
    /// that algorithm's family named by its id, the signature, the issuer, the audience, the
    /// expiry and, where given, `nbf`.
    fn verified(&self, token: &str) -> Result<Map<String, Value>, Unverified> {
        let header = jsonwebtoken::decode_header(token).map_err(|_| Unverified)?;
        if !self.mapping.algorithms.contains(&header.alg) {
            return Err(Unverified);
        }
        let kid = header.kid.as_deref().ok_or(Unverified)?;
        let keys = self.keys.read().unwrap_or_else(PoisonError::into_inner);
        let key = keys
            .0
            .iter()
            .find(|key| key.verifies(kid, header.alg))
            .ok_or(Unverified)?;
        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&self.mapping.issuer]);
        validation.set_audience(&[&self.mapping.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        validation.leeway = self.mapping.leeway;
        validation.validate_exp = true;
        validation.validate_nbf = true;
        jsonwebtoken::decode::<Map<String, Value>>(token, &key.decoding, &validation)
            .map(|data| data.claims)
            .map_err(|_| Unverified)
    }
}

impl CallerToken for Verifier {
    fn caller(&self, token: &str) -> Result<Option<Caller>, Unverified> {
        let claims = self.verified(token)?;
        Ok(self
            .mapping
            .mapped(&claims)
            .filter(|caller| caller.fixed(&self.ir).is_ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mapping(jwks: &str, algorithms: &str) -> Result<Mapping> {
        Mapping::parse(
            "--caller-token mapping.toml",
            &format!(
                r#"issuer = "https://issuer.test"
audience = "porch"
jwks = "{jwks}"
algorithms = {algorithms}

[actor]
claim = "role"

[actor.values]
auditor = "porch.visit.Auditor"
"#
            ),
        )
    }

    fn base64url(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut text = String::new();
        for chunk in bytes.chunks(3) {
            let group = chunk.iter().enumerate().fold(0u32, |group, (at, byte)| {
                group | (u32::from(*byte) << (16 - 8 * at))
            });
            for at in 0..=chunk.len() {
                text.push(char::from(
                    ALPHABET[((group >> (18 - 6 * at)) & 0x3f) as usize],
                ));
            }
        }
        text
    }

    #[test]
    fn a_mapping_naming_a_symmetric_algorithm_is_refused() {
        for symmetric in ["HS256", "HS384", "HS512"] {
            let refused = mapping(
                "http://127.0.0.1:9/jwks",
                &format!(r#"["ES256", "{symmetric}"]"#),
            )
            .expect_err("a symmetric algorithm is refused");
            let refused = format!("{refused:#}");
            assert!(
                refused.contains(symmetric) && refused.contains("symmetric"),
                "{refused}"
            );
        }
        for unknown in [r#"["none"]"#, r#"["HS1"]"#, "[]"] {
            assert!(
                mapping("http://127.0.0.1:9/jwks", unknown).is_err(),
                "{unknown} is refused"
            );
        }
        let allowed = mapping(
            "http://127.0.0.1:9/jwks",
            r#"["RS256", "PS512", "ES256", "ES384", "EdDSA"]"#,
        )
        .expect("every asymmetric algorithm is allowed");
        assert_eq!(allowed.algorithms.len(), 5);
    }

    #[test]
    fn a_jwks_url_neither_https_nor_loopback_http_is_refused() {
        for refused in [
            "http://192.0.2.1/jwks",
            "http://issuer.test/jwks",
            "http://[2001:db8::1]/jwks",
            "ftp://127.0.0.1/jwks",
            "file:///jwks.json",
            "https://user:pass@issuer.test/jwks",
            "jwks.json",
        ] {
            let error = mapping(refused, r#"["ES256"]"#)
                .expect_err("the URL is refused")
                .to_string();
            assert!(error.contains("--caller-token mapping.toml"), "{error}");
        }
        for allowed in [
            "https://issuer.test/.well-known/jwks.json",
            "http://127.0.0.1:8080/jwks",
            "http://localhost/jwks",
            "http://[::1]:9/jwks",
        ] {
            mapping(allowed, r#"["ES256"]"#)
                .unwrap_or_else(|error| panic!("{allowed} is allowed: {error:#}"));
        }
    }

    #[test]
    fn a_jwk_of_another_family_than_the_token_alg_does_not_verify() {
        let mapping = mapping("http://127.0.0.1:9/jwks", r#"["ES256", "RS256", "EdDSA"]"#)
            .expect("the mapping parses");
        let signer = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).expect("a key");
        let point = signer.public_key_raw();
        let set = json!({"keys": [
            {"kty": "RSA", "kid": "rsa", "n": base64url(&[0xc5; 256]), "e": "AQAB"},
            {"kty": "EC", "crv": "P-256", "kid": "ec",
             "x": base64url(&point[1..33]), "y": base64url(&point[33..])},
            {"kty": "OKP", "crv": "Ed25519", "kid": "ed", "x": base64url(&[7; 32])},
            {"kty": "oct", "kid": "secret", "k": base64url(b"shared")},
        ]});
        let keys = Keys::read(set.to_string().as_bytes()).expect("the set reads");
        assert_eq!(keys.0.len(), 3, "a symmetric key is left out of the set");
        let verifier = Verifier {
            mapping,
            keys: RwLock::new(keys),
            ir: spindle_spec::load(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trusted-caller"),
            )
            .expect("the trusted-caller fixture compiles")
            .ir,
        };
        let claims = json!({
            "iss": "https://issuer.test",
            "aud": "porch",
            "exp": jsonwebtoken::get_current_timestamp() + 600,
            "role": "auditor",
        });
        let signed = |kid: &str| {
            let mut header = jsonwebtoken::Header::new(Algorithm::ES256);
            header.kid = Some(kid.to_owned());
            let key = jsonwebtoken::EncodingKey::from_ec_der(&signer.serialize_der());
            jsonwebtoken::encode(&header, &claims, &key).expect("the token is signed")
        };
        assert_eq!(
            verifier
                .caller(&signed("ec"))
                .map(|caller| caller.map(|c| c.actor)),
            Ok(Some("porch.visit.Auditor".to_owned())),
            "the EC key verifies its own ES256 token"
        );
        for kid in ["rsa", "ed", "secret"] {
            assert_eq!(
                verifier.caller(&signed(kid)),
                Err(Unverified),
                "an ES256 token naming the {kid} key"
            );
        }
        let forged = |alg: &str, kid: &str| {
            format!(
                "{}.{}.{}",
                base64url(json!({"alg": alg, "kid": kid}).to_string().as_bytes()),
                base64url(claims.to_string().as_bytes()),
                base64url(&[1; 64])
            )
        };
        for (alg, kid) in [
            ("RS256", "ec"),
            ("RS256", "ed"),
            ("EdDSA", "ec"),
            ("EdDSA", "rsa"),
            ("HS256", "secret"),
            ("none", "ec"),
        ] {
            assert_eq!(
                verifier.caller(&forged(alg, kid)),
                Err(Unverified),
                "a {alg} token naming the {kid} key"
            );
        }
    }
}
