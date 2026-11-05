//! `play --component`: one component of the rally example in this process, its bindings carried
//! over TCP to the processes that play the others, as the process launcher starts it.

use std::collections::BTreeMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Clock, Host, HostError, Report, rally};
use spindle_target::{ENV_LISTEN, ENV_PEER_PREFIX, env_name};

use crate::scaled::Scaled;
use crate::{MATCH_STATE, OPEN_MATCH, REFUSED, SIGNALLED, signals};

/// How long a component waits for its peers to listen and to link to it.
const LINK_WITHIN: Duration = Duration::from_secs(30);
/// How long between two attempts to reach a peer that does not listen yet.
const RETRY: Duration = Duration::from_millis(10);
/// How long the table, having won, waits for its peers to close their links.
const END_WITHIN: Duration = Duration::from_secs(5);
/// How often the loop looks for arrivals, ticks and signals.
const POLL: Duration = Duration::from_millis(1);

/// The arguments of `play --component`.
pub(crate) struct Playing<'a> {
    pub spec: &'a Path,
    pub environment: &'a Path,
    pub target_score: i64,
    pub match_id: &'a str,
    pub speed: u32,
    pub component: &'a str,
    pub listen: Option<SocketAddr>,
}

/// The exit status of a signal recorded in `signalled`, if one was.
fn signalled_status(signalled: &AtomicUsize) -> Option<u8> {
    match signalled.load(Ordering::SeqCst) {
        0 => None,
        signal => {
            Some(SIGNALLED.saturating_add(u8::try_from(signal).unwrap_or(u8::MAX - SIGNALLED)))
        }
    }
}

/// Plays `playing.component` alone; see the `play` command's help.
///
/// Whatever ends the run once SIGINT or SIGTERM arrived, the exit status is the signal's: a
/// peer that went down with the same signal is not this component's failure.
pub(crate) fn play(playing: &Playing<'_>) -> Result<ExitCode> {
    let signalled = signals()?;
    let component = playing.component;
    let listen = playing.listen.with_context(|| {
        format!("--component {component} needs the address to listen on: --listen or {ENV_LISTEN}")
    })?;
    let loaded = spindle_spec::load(playing.spec)?;
    let opener = loaded
        .ir
        .components()
        .values()
        .find(|declared| {
            declared
                .accepts
                .iter()
                .any(|accepted| accepted.to_string() == OPEN_MATCH)
        })
        .map(|declared| declared.name.as_str().to_owned());
    let opens = opener.as_deref() == Some(component);
    let text = std::fs::read_to_string(playing.environment)
        .with_context(|| format!("reading {}", playing.environment.display()))?;
    let parsed = Environment::parse(&text)
        .with_context(|| format!("in {}", playing.environment.display()))?;
    let state = tempfile::tempdir().context("creating the state directory")?;
    let clock = Scaled::new(playing.speed);
    let mut host =
        match Host::component(loaded.ir, &parsed, Arc::new(clock), state.path(), component) {
            Ok(host) => host,
            Err(HostError::Refused(refusal)) => {
                println!("{refusal}");
                return Ok(ExitCode::from(REFUSED));
            }
            Err(error) => return Err(error.into()),
        };
    let watched = Arc::clone(&signalled);
    host.stop_when(move || watched.load(Ordering::SeqCst) != 0);
    if let Some(opener) = opener.as_deref().filter(|opener| *opener != component) {
        host.accept_end_from(opener);
    }

    let addresses = peer_addresses(&host, component)?;
    let result = linked(
        &mut host, playing, listen, opens, &clock, &addresses, &signalled,
    );
    host.stop();
    let status = match (result, signalled_status(&signalled)) {
        (Ok(status), _) => Ok(status),
        (Err(_), Some(status)) => Ok(ExitCode::from(status)),
        (Err(error), None) => Err(error),
    };
    drop(state);
    status
}

/// Links `host` to its peers and plays until the match ends, a link fails or a signal arrives.
fn linked(
    host: &mut Host,
    playing: &Playing<'_>,
    listen: SocketAddr,
    opens: bool,
    clock: &Scaled,
    addresses: &BTreeMap<String, SocketAddr>,
    signalled: &AtomicUsize,
) -> Result<ExitCode> {
    let component = playing.component;
    let _telemetry = match host.shared_counters(component) {
        Some(counters) => {
            spindle_telemetry::Sender::from_env(counters).map_err(|error| anyhow!(error))?
        }
        None => None,
    };
    let listener = TcpListener::bind(listen).with_context(|| format!("listening on {listen}"))?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(
        out,
        "{}",
        json!({
            "component": component,
            "listening": listener.local_addr()?.to_string(),
            "peers": addresses.iter().map(|(peer, address)| (peer.clone(), json!(address.to_string()))).collect::<Map<_, _>>(),
        })
    )?;
    out.flush()?;

    let deadline = Instant::now() + LINK_WITHIN;
    for (peer, address) in addresses {
        loop {
            if let Some(status) = signalled_status(signalled) {
                return Ok(ExitCode::from(status));
            }
            match host.connect(peer, *address) {
                Ok(()) => break,
                Err(error) if Instant::now() >= deadline => {
                    return Err(error).with_context(|| {
                        format!("{peer} did not listen at {address} within {LINK_WITHIN:?}")
                    });
                }
                Err(_) => std::thread::sleep(RETRY),
            }
        }
    }
    loop {
        let unlinked = host.unlinked();
        if unlinked.is_empty() {
            break;
        }
        if let Some(status) = signalled_status(signalled) {
            return Ok(ExitCode::from(status));
        }
        if Instant::now() >= deadline {
            bail!(
                "{component} has no link from {} within {LINK_WITHIN:?}",
                unlinked.join(", ")
            );
        }
        host.accept(&listener)?;
        refused(host);
        std::thread::sleep(POLL);
    }

    if opens {
        let report = host.submit(
            OPEN_MATCH,
            Map::from_iter([
                ("match_id".to_owned(), json!(playing.match_id)),
                ("target_score".to_owned(), json!(playing.target_score)),
            ]),
        )?;
        print(&mut out, &report)?;
        let context = Map::from_iter([("match_id".to_owned(), json!(playing.match_id))]);
        for binding in host.periodic() {
            host.start(&binding, context.clone())?;
        }
    }

    loop {
        if let Some(status) = signalled_status(signalled) {
            return Ok(ExitCode::from(status));
        }
        host.accept(&listener)?;
        refused(host);
        let report = host.deliver()?;
        print(&mut out, &report)?;
        if host.ended() {
            writeln!(out, "{}", json!({ "ended": "a peer ended the match" }))?;
            out.flush()?;
            return Ok(ExitCode::SUCCESS);
        }
        if host
            .next_deadline()
            .is_some_and(|deadline| deadline <= clock.now())
        {
            let report = host.advance()?;
            print(&mut out, &report)?;
            if opens {
                let view = host
                    .view(MATCH_STATE, &json!(playing.match_id))?
                    .with_context(|| format!("no match {}", playing.match_id))?;
                let won = rally::winner(&view).is_some();
                writeln!(out, "{}", json!({ "view": Value::Object(view) }))?;
                out.flush()?;
                if won {
                    host.end(END_WITHIN);
                    writeln!(out, "{}", json!({ "ended": "the match has a winner" }))?;
                    out.flush()?;
                    return Ok(ExitCode::SUCCESS);
                }
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Reports every connection the host refused on stderr.
fn refused(host: &mut Host) {
    for refusal in host.refused() {
        eprintln!("b10x-spindle: refused {refusal}");
    }
}

/// The address of every peer `component`'s events lead to, read from `SPINDLE_PEER_<PEER>`;
/// refused, naming each peer and variable, when one is unset or not one address.
fn peer_addresses(host: &Host, component: &str) -> Result<BTreeMap<String, SocketAddr>> {
    let mut addresses = BTreeMap::new();
    let mut missing = Vec::new();
    for (peer, bindings) in host.peers() {
        let variable = format!("{ENV_PEER_PREFIX}{}", env_name(&peer));
        let bindings = bindings.into_iter().collect::<Vec<_>>().join(", ");
        match std::env::var(&variable) {
            Err(std::env::VarError::NotPresent) => {
                missing.push(format!(
                    "{peer} (bindings {bindings}): {variable} is not set"
                ));
            }
            Err(std::env::VarError::NotUnicode(_)) => {
                missing.push(format!(
                    "{peer} (bindings {bindings}): {variable} is not UTF-8"
                ));
            }
            Ok(value) => {
                let parsed = value
                    .split(',')
                    .map(|address| address.trim().parse::<SocketAddr>())
                    .collect::<Result<Vec<_>, _>>();
                match parsed.as_deref() {
                    Ok([address]) => {
                        addresses.insert(peer, *address);
                    }
                    Ok(_) => missing.push(format!(
                        "{peer} (bindings {bindings}): {variable}={value} names more than one \
                         address, and this build links one instance per component"
                    )),
                    Err(error) => missing.push(format!(
                        "{peer} (bindings {bindings}): {variable}={value} is not an address: {error}"
                    )),
                }
            }
        }
    }
    if !missing.is_empty() {
        bail!(
            "{component} sends to peers it has no address for:\n  {}",
            missing.join("\n  ")
        );
    }
    Ok(addresses)
}

/// One JSON line per command `report` ran; refusals and failures on stderr, as `play` reports
/// them.
fn print(out: &mut impl Write, report: &Report) -> Result<()> {
    for ran in &report.ran {
        let cause = match &ran.cause {
            Cause::Submitted => "submitted",
            Cause::Tick { .. } => "tick",
            Cause::Delivered { .. } => "delivered",
        };
        writeln!(
            out,
            "{}",
            json!({"ran": ran.command, "cause": cause, "outcome": ran.executed.outcome})
        )?;
    }
    out.flush()?;
    for failed in &report.failed {
        eprintln!(
            "b10x-spindle: {} did not run {}: {}",
            failed.component, failed.command, failed.error
        );
    }
    for refused in &report.refused {
        eprintln!(
            "b10x-spindle: {} refused {} ({}) with {} {}",
            refused.component,
            refused.command,
            refused.outcome,
            refused.error.name,
            Value::Object(refused.error.fields.clone())
        );
    }
    Ok(())
}
