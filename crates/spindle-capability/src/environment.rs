//! The environment document: which implementations provide which capabilities.

use std::collections::BTreeMap;

use crate::Capability;

/// The longest an HTTP call may take, in milliseconds: a call holds the thread that runs every
/// command of a served run until its reply or its timeout.
pub const MAX_TIMEOUT_MS: u64 = 30_000;

/// The format an environment document declares.
pub const FORMAT: &str = "spindle-environment/1";

/// Where Spindle runs, and what it can provide there.
///
/// ```toml
/// format = "spindle-environment/1"
/// name = "laptop"
/// os = "linux"
/// arch = "x86_64"
///
/// [provides]
/// "store.durable" = ["sqlite", "memory"]
/// channel = ["in-process"]
/// "external.*" = ["operator-prompt"]
/// ```
///
/// Each list is in order of preference. A key ending in `.*` provides every capability under
/// that prefix, and an exact key wins over it.
///
/// The runtime keeps `store.durable` in `memory`, `sqlite`, `postgres` or, in the browser,
/// `indexeddb`. `postgres` is an operator's PostgreSQL: the document names only the
/// implementation, and where the database is, and with which role, comes from the settings
/// `database_url`, `database_ca`, `database_schema`, `database_pool` and
/// `database_connections`, never from the document.
///
/// An `external.` key may name an HTTP call the environment declares as `[http.<name>]`; the
/// cause it provides is then answered by that call where the runtime makes one:
///
/// ```toml
/// [provides]
/// "external.shop.order.Place.approved" = ["approvals"]
///
/// [http.approvals]
/// method = "POST"
/// url = "http://127.0.0.1:8080/approvals"
/// timeout_ms = 2000
/// secret_env = "APPROVALS_TOKEN"
/// ```
///
/// `[seed]` names the seed file a served `run` submits at every start:
///
/// ```toml
/// [seed]
/// file = "seed/roles.json"
/// ```
///
/// `[decisions]` names the decisions file `run` answers external causes from, where an
/// `external.` key lists [`DECISIONS_FILE`] and `--decisions` names none:
///
/// ```toml
/// [provides]
/// "external.*" = ["decisions-file"]
///
/// [decisions]
/// file = "decisions/answers.toml"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    /// Always [`FORMAT`].
    pub format: String,
    /// What the environment is called: `laptop`, `microvm-tcp`.
    pub name: String,
    /// The operating system, as Rust's `std::env::consts::OS` spells it.
    pub os: String,
    /// The architecture, as `std::env::consts::ARCH` spells it.
    pub arch: String,
    /// Capability (or `prefix.*`) to implementations, most preferred first.
    #[serde(default)]
    pub provides: BTreeMap<String, Vec<String>>,
    /// The HTTP calls an `external.` key may name, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub http: BTreeMap<String, HttpCall>,
    /// `[seed]`: the seed file a served `run` submits at every start, where `--seed` names none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<SeedFile>,
    /// `[decisions]`: the decisions file external causes bound to [`DECISIONS_FILE`] are answered
    /// from, where `--decisions` names none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decisions: Option<DecisionsFile>,
}

/// The implementation that answers an external cause from a decisions file.
pub const DECISIONS_FILE: &str = "decisions-file";

/// The seed file of an environment: `[seed]` with `file`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct SeedFile {
    /// The JSON array of commands, a path relative to where the runtime starts.
    pub file: String,
}

/// The decisions file of an environment: `[decisions]` with `file`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionsFile {
    /// The TOML file of answers, a path relative to where the runtime starts.
    pub file: String,
}

/// An HTTP call that answers external causes: `[http.<name>]`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct HttpCall {
    /// The request method: `POST`.
    pub method: String,
    /// The URL the request is sent to.
    pub url: String,
    /// How long the call may take, connecting to the last byte of its reply, in milliseconds:
    /// 1 to 30000.
    pub timeout_ms: u64,
    /// The environment variable holding the call's secret, never the secret itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_env: Option<String>,
    /// A PEM file of the certificates an `https://` call's server is verified against, in place
    /// of the platform's trust roots; a path relative to where the runtime starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_file: Option<String>,
}

/// Why an environment document was refused.
#[derive(Debug, thiserror::Error)]
pub enum EnvironmentError {
    /// Not TOML, or not this shape: TOML's message and where, never the source line it quotes
    /// nor a quoted value, either of which may hold a credential ([`malformed`]). TOML's error is
    /// not kept as the source, so no chain of causes prints it either.
    #[error("not a spindle environment: {0}")]
    Parse(String),
    /// Another format, or none.
    #[error("format is {found:?}, expected {FORMAT:?}")]
    Format {
        /// What the document declared.
        found: String,
    },
    /// A capability listed with no implementation.
    #[error("{capability} lists no implementation; leave it out instead")]
    EmptyList {
        /// The capability.
        capability: String,
    },
    /// An HTTP call no `external.` key names.
    #[error(
        "[http.{name}] is named by no `external.` key of provides, so it would never be called"
    )]
    HttpUnused {
        /// The call.
        name: String,
    },
    /// A `[decisions]` table no `external.` key binds to [`DECISIONS_FILE`].
    #[error(
        "[decisions] names the file {file}, but no `external.` key of provides lists \
         `decisions-file`, so it would never be read"
    )]
    DecisionsUnbound {
        /// The file the table names.
        file: String,
    },
    /// An HTTP call named by a key that is not `external.`.
    #[error("{capability} names the HTTP call `{name}`; an HTTP call answers external causes only")]
    HttpNotExternal {
        /// The call.
        name: String,
        /// The key naming it.
        capability: String,
    },
    /// An HTTP call that may take no time, or longer than [`MAX_TIMEOUT_MS`].
    #[error(
        "[http.{name}] timeout_ms is {timeout_ms}; a call may take 1 to 30000 ms, as it \
         holds the thread that runs every command while it waits"
    )]
    HttpTimeout {
        /// The call.
        name: String,
        /// The timeout it declares.
        timeout_ms: u64,
    },
    /// A URL carrying userinfo (`user:password@`); the URL is not repeated, as it holds a secret.
    #[error(
        "[http.{name}] url carries userinfo (`user:password@`): a secret is named by secret_env, \
         never written in the document"
    )]
    UrlUserinfo {
        /// The call.
        name: String,
    },
    /// A plain `http://` URL whose host is not loopback: a call that leaves the host goes over
    /// TLS. The URL is not repeated.
    #[error(
        "[http.{name}] url is http:// to a host that is not loopback (localhost, 127.0.0.0/8, \
         [::1]): a call that leaves the host is made over https://"
    )]
    HttpOffHost {
        /// The call.
        name: String,
    },
    /// A `ca_file` on a plain `http://` call, which no certificate verifies.
    #[error("[http.{name}] names a ca_file, but its url is not https://")]
    HttpCaWithoutTls {
        /// The call.
        name: String,
    },
    /// A `secret_env` that is no variable name; the value is not repeated, in case it is a secret.
    #[error(
        "[http.{name}] secret_env is not an environment variable name (ASCII letters, digits and \
         `_`, not starting with a digit): the document names the variable, never the secret"
    )]
    SecretName {
        /// The call.
        name: String,
    },
}

impl From<toml::de::Error> for EnvironmentError {
    fn from(error: toml::de::Error) -> Self {
        Self::Parse(malformed(&error))
    }
}

impl Environment {
    /// Parses and checks an environment document.
    pub fn parse(text: &str) -> Result<Self, EnvironmentError> {
        let environment: Self = toml::from_str(text)?;
        if environment.format != FORMAT {
            return Err(EnvironmentError::Format {
                found: environment.format,
            });
        }
        if let Some((capability, _)) = environment
            .provides
            .iter()
            .find(|(_, implementations)| implementations.is_empty())
        {
            return Err(EnvironmentError::EmptyList {
                capability: capability.clone(),
            });
        }
        environment.check_http()?;
        if let Some(decisions) = &environment.decisions
            && !environment
                .provides
                .iter()
                .any(|(capability, implementations)| {
                    capability.starts_with(Capability::EXTERNAL_PREFIX)
                        && implementations.iter().any(|name| name == DECISIONS_FILE)
                })
        {
            return Err(EnvironmentError::DecisionsUnbound {
                file: decisions.file.clone(),
            });
        }
        Ok(environment)
    }

    /// Refuses an `[http.<name>]` table no `external.` key names or another key names, one with
    /// no time to answer or more than [`MAX_TIMEOUT_MS`], a URL carrying userinfo, a plain
    /// `http://` URL off loopback or with a `ca_file`, and a `secret_env` that is no variable name.
    fn check_http(&self) -> Result<(), EnvironmentError> {
        for (name, call) in &self.http {
            let naming: Vec<&String> = self
                .provides
                .iter()
                .filter(|(_, implementations)| implementations.contains(name))
                .map(|(capability, _)| capability)
                .collect();
            if let Some(capability) = naming
                .iter()
                .find(|capability| !capability.starts_with(Capability::EXTERNAL_PREFIX))
            {
                return Err(EnvironmentError::HttpNotExternal {
                    name: name.clone(),
                    capability: (*capability).clone(),
                });
            }
            if naming.is_empty() {
                return Err(EnvironmentError::HttpUnused { name: name.clone() });
            }
            if !(1..=MAX_TIMEOUT_MS).contains(&call.timeout_ms) {
                return Err(EnvironmentError::HttpTimeout {
                    name: name.clone(),
                    timeout_ms: call.timeout_ms,
                });
            }
            if userinfo(&call.url) {
                return Err(EnvironmentError::UrlUserinfo { name: name.clone() });
            }
            if let Some(host) = plain_http_host(&call.url) {
                if !loopback_host(host) {
                    return Err(EnvironmentError::HttpOffHost { name: name.clone() });
                }
                if call.ca_file.is_some() {
                    return Err(EnvironmentError::HttpCaWithoutTls { name: name.clone() });
                }
            }
            if call
                .secret_env
                .as_deref()
                .is_some_and(|variable| !variable_name(variable))
            {
                return Err(EnvironmentError::SecretName { name: name.clone() });
            }
        }
        Ok(())
    }

    /// The implementations that provide `capability`, most preferred first: the exact key, or
    /// else the longest `prefix.*` key that covers it.
    pub fn implementations(&self, capability: &str) -> Option<&[String]> {
        if let Some(exact) = self.provides.get(capability) {
            return Some(exact);
        }
        self.provides
            .iter()
            .filter_map(|(key, implementations)| {
                let prefix = key.strip_suffix('*')?;
                (prefix.ends_with('.') && capability.starts_with(prefix))
                    .then_some((prefix.len(), implementations))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, implementations)| implementations.as_slice())
    }
}

/// `error` without the source it quotes: TOML's message, every `"..."` in it and every `` `...` ``
/// holding an `@` elided, then the line and column its first line names. A document is refused
/// as TOML before any `[http]` check runs, so its refusal must not print a URL's userinfo.
fn malformed(error: &toml::de::Error) -> String {
    let message = elided(error.message().trim_end());
    let rendered = error.to_string();
    match rendered
        .lines()
        .next()
        .and_then(|first| first.strip_prefix("TOML parse error at "))
    {
        Some(at) => format!("{message} (at {at})"),
        None => message,
    }
}

/// `message` with the inside of every `"..."` elided, and of every `` `...` `` that holds an
/// `@`: a value TOML or serde quotes back.
fn elided(message: &str) -> String {
    let mut shown = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(start) = rest.find(['"', '`']) {
        let quote = rest[start..].chars().next().unwrap_or('"');
        shown.push_str(&rest[..=start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(quote) else {
            shown.push_str(if quote == '"' || after.contains('@') {
                "…"
            } else {
                after
            });
            return shown;
        };
        let inside = &after[..end];
        if quote == '"' || inside.contains('@') {
            shown.push('…');
        } else {
            shown.push_str(inside);
        }
        shown.push(quote);
        rest = &after[end + 1..];
    }
    shown.push_str(rest);
    shown
}

/// Whether `url`'s authority carries userinfo: an `@` before the first `/`, `?` or `#` after the
/// scheme.
fn userinfo(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#'])
        .next()
        .is_some_and(|authority| authority.contains('@'))
}

/// The host of a plain `http://` URL (scheme in any case), as written: `[::1]` keeps its
/// brackets. `None` for any other scheme.
fn plain_http_host(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = match authority.strip_prefix('[') {
        Some(inside) => inside
            .find(']')
            .map_or(authority, |end| &authority[..end + 2]),
        None => authority.split(':').next().unwrap_or_default(),
    };
    Some(host)
}

/// Whether `host`, as a URL writes it, names this host only: `localhost` (in any case), an IPv4
/// address in `127.0.0.0/8`, or `[::1]`. Nothing is resolved; any other spelling is not loopback.
pub fn loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Some(inside) = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return inside
            .parse::<std::net::Ipv6Addr>()
            .is_ok_and(|address| address == std::net::Ipv6Addr::LOCALHOST);
    }
    host.parse::<std::net::Ipv4Addr>()
        .is_ok_and(|address| address.is_loopback())
}

/// Whether `name` is a portable environment variable name: ASCII letters, digits and `_`, not
/// starting with a digit.
fn variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_')
}

#[cfg(test)]
mod tests {
    use super::{Environment, EnvironmentError, HttpCall};

    const HEAD: &str = "format = \"spindle-environment/1\"\nname = \"approvals\"\nos = \"linux\"\narch = \"x86_64\"\n\n";

    fn parse(rest: &str) -> Result<Environment, EnvironmentError> {
        Environment::parse(&format!("{HEAD}{rest}"))
    }

    #[test]
    fn an_external_key_may_name_an_http_call() {
        let environment = parse(
            "[provides]\n\"external.shop.Place.approved\" = [\"approvals\"]\n\n\
             [http.approvals]\nmethod = \"POST\"\nurl = \"http://127.0.0.1:8080/approvals\"\n\
             timeout_ms = 2000\nsecret_env = \"APPROVALS_TOKEN\"\n",
        )
        .expect("the document parses");
        assert_eq!(
            environment.http.get("approvals"),
            Some(&HttpCall {
                method: "POST".to_owned(),
                url: "http://127.0.0.1:8080/approvals".to_owned(),
                timeout_ms: 2000,
                secret_env: Some("APPROVALS_TOKEN".to_owned()),
                ca_file: None,
            })
        );
        assert_eq!(
            environment.implementations("external.shop.Place.approved"),
            Some(&["approvals".to_owned()][..])
        );
    }

    #[test]
    fn a_document_without_http_reads_and_writes_as_before() {
        let environment =
            parse("[provides]\n\"external.*\" = [\"operator-prompt\"]\n").expect("it parses");
        assert!(environment.http.is_empty());
        let written = toml::to_string(&environment).expect("it serialises");
        assert!(!written.contains("http"), "{written}");
    }

    #[test]
    fn a_seed_table_names_its_file_and_nothing_else() {
        let seeded = parse("[seed]\nfile = \"seed/roles.json\"\n").expect("it parses");
        assert_eq!(
            seeded.seed,
            Some(super::SeedFile {
                file: "seed/roles.json".to_owned()
            })
        );
        assert!(parse("").expect("it parses").seed.is_none());
        assert!(matches!(
            parse("[seed]\nfile = \"a.json\"\nfatal = true\n"),
            Err(EnvironmentError::Parse(_))
        ));
    }

    #[test]
    fn a_decisions_table_names_its_file() {
        let bound = parse(
            "[provides]\n\"external.*\" = [\"decisions-file\"]\n\n\
             [decisions]\nfile = \"decisions/answers.toml\"\n",
        )
        .expect("it parses");
        assert_eq!(
            bound.decisions,
            Some(super::DecisionsFile {
                file: "decisions/answers.toml".to_owned()
            })
        );
        let written = toml::to_string(&bound).expect("it serialises");
        assert_eq!(
            Environment::parse(&written).expect("it reads back"),
            bound,
            "{written}"
        );
        let plain = parse("").expect("it parses");
        assert!(plain.decisions.is_none());
        let written = toml::to_string(&plain).expect("it serialises");
        assert!(!written.contains("decisions"), "{written}");
        assert!(matches!(
            parse(
                "[provides]\n\"external.*\" = [\"decisions-file\"]\n\n\
                 [decisions]\nfile = \"a.toml\"\nfatal = true\n"
            ),
            Err(EnvironmentError::Parse(_))
        ));
    }

    #[test]
    fn a_decisions_table_no_external_key_binds_is_refused() {
        for provides in [
            "",
            "[provides]\n\"external.*\" = [\"operator-prompt\"]\n",
            "[provides]\nchannel = [\"decisions-file\"]\n\"external.*\" = [\"operator-prompt\"]\n",
        ] {
            let refused = parse(&format!(
                "{provides}\n[decisions]\nfile = \"answers.toml\"\n"
            ));
            let Err(error) = refused else {
                panic!("{provides}: a table nothing binds is refused: {refused:?}");
            };
            assert!(
                matches!(error, EnvironmentError::DecisionsUnbound { ref file } if file == "answers.toml"),
                "{provides}: {error:?}"
            );
            assert!(
                error.to_string().contains("decisions-file"),
                "{provides}: the refusal names the implementation: {error}"
            );
        }
        let exact = parse(
            "[provides]\n\"external.*\" = [\"operator-prompt\"]\n\
             \"external.shop.Place.approved\" = [\"decisions-file\"]\n\n\
             [decisions]\nfile = \"answers.toml\"\n",
        );
        assert!(exact.is_ok(), "an exact external key binds it: {exact:?}");
        let unbound = parse("[provides]\n\"external.*\" = [\"decisions-file\"]\n")
            .expect("a binding with no table parses: --decisions names the file");
        assert!(unbound.decisions.is_none());
    }

    #[test]
    fn an_http_call_no_external_key_names_is_refused() {
        let unused = parse(
            "[provides]\n\"external.*\" = [\"operator-prompt\"]\n\n\
             [http.approvals]\nmethod = \"POST\"\nurl = \"http://127.0.0.1:1/\"\ntimeout_ms = 1\n",
        );
        assert!(
            matches!(unused, Err(EnvironmentError::HttpUnused { ref name }) if name == "approvals"),
            "{unused:?}"
        );
        let elsewhere = parse(
            "[provides]\nchannel = [\"approvals\"]\n\"external.*\" = [\"approvals\"]\n\n\
             [http.approvals]\nmethod = \"POST\"\nurl = \"http://127.0.0.1:1/\"\ntimeout_ms = 1\n",
        );
        assert!(
            matches!(
                elsewhere,
                Err(EnvironmentError::HttpNotExternal { ref capability, .. }) if capability == "channel"
            ),
            "{elsewhere:?}"
        );
    }

    #[test]
    fn an_http_call_with_no_time_or_a_secret_in_place_of_its_variable_is_refused() {
        let call = |extra: &str| {
            parse(&format!(
                "[provides]\n\"external.*\" = [\"approvals\"]\n\n[http.approvals]\n\
                 method = \"POST\"\nurl = \"http://127.0.0.1:1/\"\n{extra}"
            ))
        };
        assert!(matches!(
            call("timeout_ms = 0\n"),
            Err(EnvironmentError::HttpTimeout { .. })
        ));
        let pasted = call("timeout_ms = 10\nsecret_env = \"s3cr3t-t0ken.value\"\n");
        let Err(error) = pasted else {
            panic!("a secret in place of a variable name is refused: {pasted:?}");
        };
        assert!(matches!(error, EnvironmentError::SecretName { .. }));
        assert!(
            !error.to_string().contains("s3cr3t"),
            "the refusal never repeats the value: {error}"
        );
        assert!(matches!(
            call("timeout_ms = 10\nheaders = {}\n"),
            Err(EnvironmentError::Parse(_))
        ));
    }

    #[test]
    fn a_timeout_above_the_maximum_is_refused() {
        let call = |timeout_ms: u64| {
            parse(&format!(
                "[provides]\n\"external.*\" = [\"approvals\"]\n\n[http.approvals]\n\
                 method = \"POST\"\nurl = \"http://127.0.0.1:1/\"\ntimeout_ms = {timeout_ms}\n"
            ))
        };
        assert!(call(super::MAX_TIMEOUT_MS).is_ok());
        let over = call(super::MAX_TIMEOUT_MS + 1);
        assert!(
            matches!(over, Err(EnvironmentError::HttpTimeout { ref name, .. }) if name == "approvals"),
            "{over:?}"
        );
    }

    #[test]
    fn a_url_carrying_userinfo_is_refused_without_repeating_it() {
        for url in [
            "http://desk-user:desk-password@127.0.0.1:1/approvals",
            "http://desk-password@127.0.0.1:1/",
            "HTTP://u:desk-password@127.0.0.1:1?q=a@b",
        ] {
            let refused = parse(&format!(
                "[provides]\n\"external.*\" = [\"approvals\"]\n\n[http.approvals]\n\
                 method = \"POST\"\nurl = \"{url}\"\ntimeout_ms = 10\n"
            ));
            let Err(error) = refused else {
                panic!("a URL with userinfo is refused: {url}");
            };
            assert!(
                matches!(error, EnvironmentError::UrlUserinfo { ref name } if name == "approvals"),
                "{error:?}"
            );
            assert!(
                !error.to_string().contains("desk-password"),
                "the refusal never repeats the credential: {error}"
            );
        }
        let query = parse(
            "[provides]\n\"external.*\" = [\"approvals\"]\n\n[http.approvals]\n\
             method = \"POST\"\nurl = \"http://127.0.0.1:1/a?to=x@y\"\ntimeout_ms = 10\n",
        );
        assert!(
            query.is_ok(),
            "an @ after the authority is no userinfo: {query:?}"
        );
    }

    #[test]
    fn a_document_toml_cannot_read_is_refused_without_quoting_its_source() {
        let url = "http://desk-user:desk-password@127.0.0.1:1/approvals";
        let head = "[provides]\n\"external.*\" = [\"approvals\"]\n\n[http]\n";
        for (mistake, row) in [
            (
                "a negative timeout",
                format!("approvals = {{ method = \"POST\", url = \"{url}\", timeout_ms = -1 }}"),
            ),
            (
                "no timeout",
                format!("approvals = {{ method = \"POST\", url = \"{url}\" }}"),
            ),
            (
                "a misspelt key",
                format!("approvals = {{ method = \"POST\", url = \"{url}\", timeout = 1 }}"),
            ),
            (
                "a bad escape",
                format!("approvals = {{ method = \"POST\", url = \"{url}\\q\", timeout_ms = 1 }}"),
            ),
            (
                "the URL where a number belongs",
                format!("approvals = {{ method = \"POST\", url = \"x\", timeout_ms = \"{url}\" }}"),
            ),
            (
                "the URL as a key",
                format!(
                    "approvals = {{ method = \"POST\", url = \"x\", timeout_ms = 1, \"{url}\" = 1 }}"
                ),
            ),
            (
                "the URL as a number",
                format!("approvals = {{ method = \"POST\", url = {url}, timeout_ms = 1 }}"),
            ),
        ] {
            let refused = parse(&format!("{head}{row}\n"));
            let Err(error @ EnvironmentError::Parse(_)) = refused else {
                panic!("{mistake}: TOML cannot read it: {refused:?}");
            };
            let shown = error.to_string();
            assert!(
                !shown.contains("desk-password"),
                "{mistake}: the refusal quotes the credential: {shown}"
            );
            assert!(
                shown.contains("line 10"),
                "{mistake}: the refusal says where: {shown}"
            );
            assert!(
                std::error::Error::source(&error).is_none(),
                "{mistake}: no cause chain carries TOML's error, which quotes the source line"
            );
        }
    }
}
