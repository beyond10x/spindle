//! The status data, `website/data/status.json` (`b10x-status/1`), and the status page,
//! `website/docs/status.md`, both from [`CAPABILITIES`].
//!
//! Every `shipped` item names the test on `main` that holds it as `file::name`: a Rust `fn`, a
//! Vitest `test('…')` in a `.ts` file, or a task in `Taskfile.yml`. [`missing_tests`] refuses an
//! item whose test does not exist. A `planned` item is a story of
//! the planning store that is not implemented, or a milestone of the design (§ 12) with no story
//! yet.

use std::{fs, path::Path};

use serde_json::json;

use crate::page::{Meta, cell, front_matter};

/// Where the status data lands, relative to the repository root.
pub const DATA: &str = "website/data/status.json";
/// Where the status page lands, relative to the repository root.
pub const PAGE: &str = "website/docs/status.md";

/// The date the list below was last checked against the code and the planning store.
const AS_OF: &str = "2026-11-04";

/// One capability on the status page.
pub struct Capability {
    pub area: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    /// `shipped` or `planned`.
    pub status: &'static str,
    /// The documentation route that explains it, `/docs/...`.
    pub href: &'static str,
    /// For a shipped item, the test that holds it: `path/to/file.rs::function`.
    pub test: Option<&'static str>,
}

const fn shipped(
    area: &'static str,
    label: &'static str,
    detail: &'static str,
    href: &'static str,
    test: &'static str,
) -> Capability {
    Capability {
        area,
        label,
        detail,
        status: "shipped",
        href,
        test: Some(test),
    }
}

const fn planned(
    area: &'static str,
    label: &'static str,
    detail: &'static str,
    href: &'static str,
) -> Capability {
    Capability {
        area,
        label,
        detail,
        status: "planned",
        href,
        test: None,
    }
}

/// Every capability the site claims, in the order the status table lists them.
pub const CAPABILITIES: &[Capability] = &[
    shipped(
        "Specification",
        "Load and compile",
        "A directory of ESS documents is compiled by ess-compiler to the resolved IR; a document ESS refuses is refused with ESS's diagnostics.",
        "/docs/concepts/specification",
        "crates/spindle-spec/tests/spindle-spec/load.rs::a_refused_document_reports_what_ess_said",
    ),
    shipped(
        "Capabilities",
        "Derived from the specification",
        "Entities need store.durable, periodic bindings timer and clock, cross-component bindings channel, external outcomes external.<command>.<outcome>.",
        "/docs/concepts/capabilities",
        "crates/spindle-capability/tests/spindle-capability/admission.rs::a_periodic_cause_needs_a_timer_and_a_clock_and_a_cross_component_binding_a_channel",
    ),
    shipped(
        "Capabilities",
        "Per component",
        "capabilities --component lists only what one component needs when it runs on its own.",
        "/docs/concepts/capabilities",
        "crates/spindle-capability/tests/spindle-capability/admission.rs::each_component_needs_only_what_it_runs",
    ),
    shipped(
        "Admission",
        "Bind by preference",
        "Each required capability is bound to the first implementation the environment lists for it; an exact key wins over a prefix key.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-capability/tests/spindle-capability/admission.rs::admission_takes_the_most_preferred_implementation",
    ),
    shipped(
        "Admission",
        "Refusal names what is missing",
        "A refused admission names every capability without an implementation and exits 3.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-cli/tests/spindle-cli/admit.rs::refused_exits_three_and_names_the_missing_capability",
    ),
    shipped(
        "Execution",
        "Commands against the compiled model",
        "spindle-kernel selects the outcome by the IR's conditions and guards, applies its effect and emits its events; b10x-spindle run reads commands as JSON lines.",
        "/docs/reference/cli",
        "crates/spindle-kernel/tests/spindle-kernel/deployment.rs::offer_then_admit_moves_a_deployment_to_admitted",
    ),
    shipped(
        "Execution",
        "Memory and SQLite stores",
        "The store follows the store.durable binding; SQLite state survives a restart.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-kernel/tests/spindle-kernel/deployment.rs::state_survives_a_restart_on_sqlite",
    ),
    shipped(
        "Execution",
        "Browser store",
        "spindle-store-browser keeps instances in the browser's IndexedDB (store.durable bound to indexeddb in examples/environments/browser.toml): it passes the store contract the memory and SQLite stores pass, and a command's effect is read back after the database is reopened. The Service Worker host opens one per component whose state is bound to indexeddb.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-store-browser/tests/spindle-store-browser/reload.rs::state_survives_a_reload",
    ),
    shipped(
        "Execution",
        "PostgreSQL store",
        "store.durable bound to postgres keeps each component's state in its own tables (spindle_<component>_*) of an operator's PostgreSQL, through eventlog-postgres: it passes the store contract the memory and SQLite stores pass, and run's state survives a restart. store migrate creates the tables with the owner role; with database_ca, run opens over TLS verified against it with a DML-only serving role inside the connection budget, and refuses any other before listening.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-kernel/tests/spindle-kernel/postgres.rs::the_postgres_store_passes_the_contract",
    ),
    shipped(
        "Execution",
        "State moved into PostgreSQL",
        "state move copies a run --state directory's streams into the PostgreSQL tables of every component bound to postgres, versions kept: run on PostgreSQL then answers every view as the run on the directory did, and a command decided after the move names the version the directory's last write left. A directory missing a component's file, or tables already holding a stream, is refused before anything is written.",
        "/docs/concepts/environment-and-admission#moving-a-state-directory-into-postgresql",
        "crates/spindle-cli/tests/spindle-cli/state_move.rs::state_moves_into_postgres_and_answers_as_before",
    ),
    shipped(
        "Execution",
        "Service Worker host",
        "spindle-wasm compiles a specification in the browser, admits it on a browser environment (ingress.network bound to service-worker) and answers its routes and the console routes from a Service Worker through the same router the loopback listener calls, state in IndexedDB and external outcomes answered from a decisions file; web/ is the Vue shell that registers the worker.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-wasm/tests/spindle-wasm/service_worker.rs::a_fetch_to_a_bound_route_is_answered_in_the_browser",
    ),
    shipped(
        "Bindings",
        "Periodic bindings",
        "spindle-timer yields due ticks for periodic bindings on an injected clock, honouring the ESS periodic profile.",
        "/docs/concepts/components-and-bindings",
        "crates/spindle-timer/tests/spindle-timer/periodic.rs::a_periodic_binding_is_due_every_period",
    ),
    shipped(
        "Bindings",
        "Event bindings in process and over TCP",
        "spindle-channel carries an event to the command it binds to, in process or as length-prefixed JSON frames over TCP; the receiver acknowledges an at_least_once envelope, and no runtime path resends one whose acknowledgement is overdue.",
        "/docs/concepts/components-and-bindings",
        "crates/spindle-channel/tests/spindle-channel/channel.rs::an_event_reaches_the_bound_component_over_tcp",
    ),
    shipped(
        "Bindings",
        "Retry on failure",
        "An on_failure: retry binding runs its command again at once after each refused or failed attempt, up to 5 attempts, before the next envelope runs; each attempt and each given-up invocation is counted. A per-refusal policy (on_failure with named refusals) retries or drops by refusal, with the same limit. --drop-retries runs every retrying binding as a drop binding.",
        "/docs/concepts/components-and-bindings",
        "crates/spindle-host/tests/spindle-host/retry.rs::a_failing_invocation_is_retried_until_it_succeeds",
    ),
    shipped(
        "Rally",
        "Every component in one process",
        "b10x-spindle play runs the three rally components in one process with in-process channels until a side wins.",
        "/docs/examples/rally",
        "crates/spindle-host/tests/spindle-host/rally.rs::a_match_played_in_one_process_ends_with_a_winner",
    ),
    shipped(
        "Targets",
        "The target contract and the process launcher",
        "Placement refuses an instance above its target's ceiling; the process launcher starts and stops an instance and reports its exit.",
        "/docs/concepts/deployment-targets",
        "crates/spindle-target/tests/spindle-target/target.rs::the_process_launcher_starts_and_stops_an_instance_and_reports_its_exit",
    ),
    shipped(
        "Targets",
        "QEMU microvm launcher",
        "A library launcher: one qemu-system-x86_64 -M microvm per instance with KVM, without root. Its tests run on a host with /dev/kvm (task check:vm), not in CI.",
        "/docs/concepts/deployment-targets",
        "crates/spindle-qemu/tests/spindle-qemu/microvm.rs::a_qemu_microvm_instance_boots_and_runs_b10x_spindle",
    ),
    shipped(
        "Targets",
        "Firecracker launcher",
        "A library launcher: Firecracker VMs on a bridge inside a user and network namespace, without root. Its tests need /dev/kvm, the Firecracker binary and a guest kernel (task check:vm), not in CI.",
        "/docs/concepts/deployment-targets",
        "crates/spindle-firecracker/tests/spindle-firecracker/firecracker.rs::a_firecracker_instance_boots_and_runs_b10x_spindle",
    ),
    shipped(
        "Execution",
        "External decisions from a file",
        "run --decisions answers external outcomes from a TOML file, per command or for all, for stdin lines and served requests; --answer-default takes a command's default branch.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/decisions.rs::a_decisions_file_answers_every_external_outcome",
    ),
    shipped(
        "Execution",
        "Minted creation identities",
        "A creation whose identity no input or payload supplies needs identity.mint; a Uuid identity is minted as a UUIDv4, a String newtype identity as its prefix and a UUIDv4's text, and emitted.",
        "/docs/concepts/capabilities",
        "crates/spindle-kernel/tests/spindle-kernel/mint.rs::a_creation_without_an_id_input_mints_one_and_emits_it",
    ),
    shipped(
        "Execution",
        "Deleting outcomes",
        "An outcome that declares deletes: on the instance its input names removes the row as ESS's interpreter does: it leaves every view, a later command on the identity answers as an unknown instance, and the outcome's events are published; on SQLite the removal survives a restart.",
        "/docs/concepts/specification",
        "crates/spindle-kernel/tests/spindle-kernel/deletes.rs::deletes_agree_with_the_interpreter",
    ),
    shipped(
        "Execution",
        "Set outcomes",
        "An outcome that declares instances: selects every row of its entity whose filter holds, read from the store before the outcome, and moves, updates or removes each in ascending order of identity as ESS's interpreter does; a move skips a row outside its from states, and zero rows selected is a successful outcome. {count: changed} reads how many rows changed, and an event declared once per changed row is published once for each. A filter that walks the rows of an entity stays refused, by name.",
        "/docs/concepts/specification",
        "crates/spindle-kernel/tests/spindle-kernel/set_outcomes.rs::set_outcomes_agree_with_the_interpreter",
    ),
    shipped(
        "Execution",
        "List views with filters",
        "List views over every instance of an entity, with a state filter and an order.",
        "/docs/concepts/specification",
        "crates/spindle-host/tests/spindle-host/views.rs::a_state_filter_keeps_only_instances_in_that_state",
    ),
    shipped(
        "Execution",
        "View fields read through a reference",
        "A view field declared field: {related: {via, field}} answers what the row the reference names holds now, from any component, and is left out where the reference is empty or names no row, as ESS's interpreter answers; each referenced row is loaded once per read by its identity.",
        "/docs/concepts/specification",
        "crates/spindle-host/tests/spindle-host/related.rs::related_fields_agree_with_the_interpreter",
    ),
    shipped(
        "Observe",
        "Counters derived from the specification",
        "spindle-telemetry counts commands by outcome, events, bindings, timer ticks and store writes per component; b10x-spindle metrics prints the catalogue, play prints the table's counters.",
        "/docs/concepts/observe",
        "crates/spindle-telemetry/tests/spindle-telemetry/catalogue.rs::every_command_outcome_and_event_of_a_component_has_a_counter",
    ),
    shipped(
        "Observe",
        "Host-side resource sampling",
        "Launchers read CPU seconds and resident memory per instance from /proc and its share of the requested quota.",
        "/docs/concepts/observe",
        "crates/spindle-target/tests/spindle-target/sampling.rs::cpu_share_is_cpu_seconds_over_wall_time_and_requested_vcpus",
    ),
    shipped(
        "Observe",
        "Telemetry transport",
        "An instance that starts the spindle-telemetry sender sends its cumulative counters and its own resource readings as spindle-telemetry/1 frames every second, over a Unix socket for a process and vsock for QEMU and Firecracker, and drops and counts frames no collector takes.",
        "/docs/concepts/observe",
        "crates/spindle-telemetry/tests/spindle-telemetry/transport.rs::a_lost_frame_loses_no_count",
    ),
    shipped(
        "Observe",
        "The observe API contract",
        "The spindle.observe domain specifies the API answers; their JSON Schema, Rust and TypeScript types are generated and drift-checked.",
        "/docs/concepts/observe",
        "Taskfile.yml::check:the_generated_types_have_no_drift",
    ),
    shipped(
        "Observe",
        "The observe UI, on fixtures",
        "A Vue 3 app in ui/ with topology, instance and view screens, built and tested on fixture answers; no live data until the control process exists.",
        "/docs/concepts/observe",
        "ui/src/screens/TopologyScreen.test.ts::the_topology_shows_one_card_per_component_and_one_edge_per_binding",
    ),
    shipped(
        "Rally",
        "Three processes",
        "Three b10x-spindle play --component processes, one per component, started by the process launcher from its SPINDLE_* variables and joined by TCP channels, play a match to a winner; each sends its telemetry when given an endpoint.",
        "/docs/examples/rally",
        "crates/spindle-cli/tests/spindle-cli/processes.rs::a_match_between_three_processes_ends_with_a_winner",
    ),
    shipped(
        "Rally",
        "Three microVMs",
        "Each rally component in its own microVM, one b10x-spindle play per VM in its per-component mode, joined by TCP channels across the VM network, plays a match to a winner; one placement (examples/rally/placement.toml) runs on QEMU and on Firecracker. Its tests run on a host with /dev/kvm (task check:vm), not in CI.",
        "/docs/examples/rally",
        "crates/spindle-qemu/tests/spindle-qemu/rally.rs::a_match_between_three_qemu_microvms_ends_with_a_winner",
    ),
    shipped(
        "Execution",
        "HTTP ingress",
        "run serves the accepted commands and views over HTTP on 127.0.0.1 where ingress.network is bound to http-loopback: at the routes ess generate --kind openapi publishes, with or without an ess-http/1 binding, answering the envelope ESS publishes.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/ingress.rs::a_bound_route_answers_with_the_bindings_status_per_outcome",
    ),
    shipped(
        "Execution",
        "DELETE routes",
        "A command an ess-http/1 binding puts on method: delete is served at its path: a DELETE with no body runs with the input its path carries and answers the status the binding lists; a method the path does not serve answers 405 with Allow naming DELETE.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/ingress.rs::a_command_bound_to_delete_is_served",
    ),
    shipped(
        "Execution",
        "View parameters",
        "A view that declares parameters is served: each is read from the path or the query string where the OpenAPI document ESS publishes places it, and the view answers the rows ESS's interpreter answers for those values; a required parameter left out, or one not of its declared type, is refused 400 naming it.",
        "/docs/concepts/capabilities",
        "crates/spindle-ingress/tests/spindle-ingress/view_parameters.rs::a_view_parameter_in_the_query_filters_the_rows",
    ),
    shipped(
        "Execution",
        "Form and query requests",
        "A command an ess-http/1 binding reads as request: {encoding: form} or {encoding: query} is served: its input is read from an application/x-www-form-urlencoded body or from the query string, each value by its field's type as ESS's served application reads it; an undeclared field, one the path carries, or a form body under another Content-Type is refused 400 and writes nothing.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/form_query.rs::a_form_encoded_command_runs",
    ),
    shipped(
        "Execution",
        "Outcome bodies",
        "A command outcome an ess-http/1 binding answers with body: response, body: response.<field> or body: none answers its bound status with the command's response bare, the one field's value bare, or no body and no Content-Type, as ess generate --kind openapi publishes it, with the response headers the binding declares (a WWW-Authenticate challenge among them); one bound with a redirect or the text encoding answers 501 naming which.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/outcome_bodies.rs::a_response_body_is_served_bare",
    ),
    shipped(
        "Execution",
        "Error object bodies",
        "A refusing outcome an ess-http/1 binding answers with body: {<member>: error} answers its bound status with an object whose <member> is the error's wire name, beside the error's fields under their wire names, as ess generate --kind openapi publishes it, and writes nothing.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/error_object_bodies.rs::a_bound_error_outcome_answers_its_status_and_error_object",
    ),
    shipped(
        "Execution",
        "Console routes",
        "A loopback run also serves GET /api/model (the IR as ess specify compile prints it, and each outcome's subject), GET /api/events (every event published, in order) and POST /api/reset (empties every store); a route the binding or ESS publishes at one of those paths is refused at startup.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/console.rs::reset_empties_every_store_of_the_run",
    ),
    shipped(
        "Execution",
        "A world per reset",
        "POST /api/reset may carry a world: rows checked by ESS and written where their entity rests, a decision table answering external causes and a seed identities are minted from, each until the next reset; a world that does not validate is refused 400 and changes nothing.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/served_world.rs::a_reset_with_seed_rows_serves_them",
    ),
    shipped(
        "Execution",
        "Serve what runs",
        "An HTTP run serves every command and view that runs and answers each other one 501 naming the construct, ESS's envelope for a command; the list is printed at startup, in name order. A view the ingress cannot route answers 501 naming the construct too, and is listed at startup. SQLite state without --state is kept in a temporary directory removed when the run ends.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/unserved.rs::an_unrunnable_command_answers_501_and_the_rest_serve",
    ),
    shipped(
        "Execution",
        "Fixed caller (test backend)",
        "run --fixed-actor with --fixed-caller sends served commands as actors and callers fixed for the whole run, so a command that reads its caller is served: a test-backend switch, not authentication, opt-in only and printed on stderr at startup. The grant check still runs: a command the actor may not send answers 403 not granted and writes nothing. Given more than once, each command is sent as the first fixed actor its grant names.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/fixed_caller.rs::a_command_reading_the_caller_runs_with_the_fixed_caller",
    ),
    shipped(
        "Execution",
        "Caller proven by a trusted proxy header",
        "run --trusted-caller-header reads each request's actor and caller attributes from one JSON header a proxy in front of the ingress sets once it has authenticated the request, so a command that reads its caller runs for that caller and a read-granted view is read as it. The proxy is trusted by a shared secret: --trusted-caller-secret names the file holding it, and the proxy presents it on every request in the Spindle-Proxy-Secret header. A request without a valid caller header or the right secret is refused 403 not granted naming no actor and writes nothing, and so is a console or /__spindle/ request without the secret; the secret is never printed. The ingress must be reachable only through the proxy, which must remove both headers from client requests.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/trusted_caller.rs::a_command_reading_the_caller_runs_for_the_caller_the_request_proves",
    ),
    shipped(
        "Execution",
        "Reads beside commands",
        "An HTTP run answers every read while a command runs: a view, GET /api/model, GET /api/events and every GET /__spindle/ route are each answered on a thread of their own, from the state the last finished command left, never part of a command, on the memory and SQLite stores. Commands and the reset still run one at a time in arrival order; a slow external call holds the next command, never a read.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-cli/tests/spindle-cli/concurrent_reads.rs::reads_are_answered_from_the_last_committed_write_while_a_command_is_held",
    ),
    shipped(
        "Execution",
        "View reads on SQLite cost their rows",
        "A view read on a SQLite state costs the rows of the entity it reads, not the log: the store keeps each entity's rows as they rest, read from its file once when it opens and then from the events each write appends, and every scan answers what a read of the whole log answers. A read of 40 rows beside 100,000 events answers in under 20 ms in a debug build (0.5 ms measured), where it took over a second.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-kernel/tests/spindle-kernel/resting_rows.rs::a_view_read_of_forty_rows_beside_a_hundred_thousand_events_answers_in_under_20_ms",
    ),
    shipped(
        "Conformance",
        "Scenario replay",
        "run --scenario replays an ess-scenario document through ESS's reader and conformance runner: an entity it arranges with setup is written into the store of the component that owns it, captures feed later steps, view asserts are checked, and the first divergent step is named and ends the run.",
        "/docs/concepts/specification",
        "crates/spindle-conformance/tests/spindle-conformance/establish.rs::an_established_row_is_read_by_the_next_command",
    ),
    shipped(
        "Conformance",
        "Self-conformance",
        "conform runs the suite ESS synthesises from a specification, or one written for it, against every component in one process; task check runs it on Spindle's own specification, lists every synthesis refusal with its code and holds the executed and skipped counts.",
        "/docs/concepts/specification",
        "crates/spindle-cli/tests/spindle-cli/conform.rs::every_executed_scenario_passes",
    ),
    shipped(
        "Observe",
        "The control process",
        "b10x-spindle deploy places and launches the instances of a spindle-placement/1 document on process targets, collects their samples, keeps the last 15 minutes and serves the read-only observe API and the UI embedded in the binary on 127.0.0.1, with a token and a Host check.",
        "/docs/concepts/observe",
        "crates/spindle-control/tests/spindle-control/control.rs::the_stream_carries_one_sample_per_instance_per_period",
    ),
    planned(
        "Deploy",
        "Bundle, deploy service and registry",
        "Upload a bundle to deploy it; design milestone M4, no story yet.",
        "/docs/concepts/deployment-targets",
    ),
    planned(
        "Deploy",
        "Cells, ownership and snapshots",
        "Cells across threads, ownership leases with fencing, snapshots; design milestone M5, no story yet.",
        "/docs/concepts/deployment-targets",
    ),
];

/// Every shipped item whose test is not a `fn` in the named file under `root`.
pub fn missing_tests(root: &Path) -> Vec<String> {
    let mut missing = Vec::new();
    for c in CAPABILITIES {
        let Some(test) = c.test else {
            if c.status == "shipped" {
                missing.push(format!("{}: shipped without a test", c.label));
            }
            continue;
        };
        let Some((file, function)) = test.split_once("::") else {
            missing.push(format!("{}: {test} is not file::function", c.label));
            continue;
        };
        let needle = if file.ends_with(".rs") {
            format!("fn {function}(")
        } else if file.ends_with(".ts") {
            format!("test('{function}'")
        } else {
            format!("{function}:")
        };
        let found = fs::read_to_string(root.join(file)).is_ok_and(|text| text.contains(&needle));
        if !found {
            missing.push(format!("{}: no test {test}", c.label));
        }
    }
    missing
}

/// `website/data/status.json`.
pub fn data() -> String {
    let items: Vec<_> = CAPABILITIES
        .iter()
        .map(|c| {
            json!({
                "area": c.area,
                "label": c.label,
                "detail": c.detail,
                "status": c.status,
                "href": c.href,
            })
        })
        .collect();
    let document = json!({"format": "b10x-status/1", "asOf": AS_OF, "items": items});
    serde_json::to_string_pretty(&document).expect("status data serializes") + "\n"
}

/// `website/docs/status.md`.
pub fn page() -> String {
    let mut page = front_matter(&Meta {
        title: "Status",
        label: "Status",
        description: "What Spindle ships on main, each item with the test that holds it, and what is planned.",
        position: 30,
    });
    let count = |status: &str| CAPABILITIES.iter().filter(|c| c.status == status).count();
    page.push_str(&format!(
        "Checked against `main` and the planning store on {AS_OF}: {} shipped, {} planned. \
         Every **shipped** item is held by a test on `main`, listed under the table. **Planned** \
         is a story that is not implemented yet, or a milestone of the design with no story; \
         nothing planned runs today.\n\n\
         | Area | Capability | Status | What it means |\n|---|---|---|---|\n",
        count("shipped"),
        count("planned"),
    ));
    for c in CAPABILITIES {
        page.push_str(&format!(
            "| {} | [{}]({}) | {} | {} |\n",
            cell(c.area),
            cell(c.label),
            c.href,
            c.status,
            cell(c.detail)
        ));
    }
    page.push_str("\n## The tests that hold them\n\n");
    for c in CAPABILITIES {
        if let Some((file, function)) = c.test.and_then(|test| test.split_once("::")) {
            page.push_str(&format!(
                "- {}: `{function}` in [`{file}`]({}/{file})\n",
                cell(c.label),
                crate::page::SOURCE
            ));
        }
    }
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_item_names_a_test_that_exists() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert_eq!(missing_tests(&root), Vec::<String>::new());
        for c in CAPABILITIES {
            assert!(
                c.status == "shipped" || c.status == "planned",
                "{}",
                c.label
            );
            assert_eq!(c.status == "shipped", c.test.is_some(), "{}", c.label);
            assert!(c.href.starts_with("/docs/"), "{}", c.href);
        }
    }

    /// `story:one-test-binary-per-crate`: every test target cargo discovers under a crate's
    /// `tests/` (a `*.rs` file there, or a directory with a `main.rs`) is that crate's
    /// `tests/<crate>/main.rs`, so a citation `crates/<crate>/tests/<crate>/<file>.rs` names the
    /// one binary that runs it. Two tests hold process-wide state and keep their own binary: the
    /// telemetry allocation test (its global allocator would replace every other test's) and the
    /// QEMU context-id test (it takes every guest context id the process may give out).
    #[test]
    fn each_crate_links_one_integration_test_binary() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut found = Vec::new();
        for entry in fs::read_dir(&crates).expect("crates/ reads") {
            let package = entry.expect("a crate").path();
            let Ok(tests) = fs::read_dir(package.join("tests")) else {
                continue;
            };
            let name = package.file_name().unwrap().to_string_lossy().into_owned();
            for target in tests {
                let path = target.expect("a test entry").path();
                let file = path.file_name().unwrap().to_string_lossy().into_owned();
                if path.extension().is_some_and(|ext| ext == "rs") {
                    found.push(format!("{name}/tests/{file}"));
                } else if path.join("main.rs").is_file() {
                    found.push(format!("{name}/tests/{file}/main.rs"));
                }
            }
        }
        let stray: Vec<&String> = found
            .iter()
            .filter(|target| {
                let (name, rest) = target.split_once("/tests/").unwrap();
                rest != format!("{name}/main.rs")
                    && ![
                        "spindle-telemetry/tests/blocking.rs",
                        "spindle-qemu/tests/context_ids.rs",
                    ]
                    .contains(&target.as_str())
            })
            .collect();
        assert_eq!(stray, Vec::<&String>::new(), "of {found:?}");
    }

    #[test]
    fn data_and_page_list_every_capability() {
        let data: serde_json::Value = serde_json::from_str(&data()).unwrap();
        assert_eq!(data["format"], "b10x-status/1");
        assert_eq!(data["items"].as_array().unwrap().len(), CAPABILITIES.len());
        let page = page();
        for c in CAPABILITIES {
            assert!(
                page.contains(&format!("[{}]({})", c.label, c.href)),
                "{}",
                c.label
            );
        }
    }
}
