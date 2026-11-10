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
}

/// Why an environment document was refused.
#[derive(Debug, thiserror::Error)]
pub enum EnvironmentError {
    /// Not TOML, or not this shape.
    #[error("not a spindle environment: {0}")]
    Parse(#[from] toml::de::Error),
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
        Ok(environment)
    }

    /// Refuses an `[http.<name>]` table no `external.` key names or another key names, one with
    /// no time to answer or more than [`MAX_TIMEOUT_MS`], a URL carrying userinfo, and a
    /// `secret_env` that is no variable name.
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

/// Whether `url`'s authority carries userinfo: an `@` before the first `/`, `?` or `#` after the
/// scheme.
fn userinfo(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#'])
        .next()
        .is_some_and(|authority| authority.contains('@'))
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
}
