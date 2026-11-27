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
        "Execution",
        "Node export",
        "spindle-node, bound by wasm-bindgen as an ES module, serves a specification in-process in a Node program: created from its sources, an environment whose stores are bound to memory (ingress.network bound to node) and a decisions file or the answer default, it answers one request at a time through the router the loopback listener calls, the console routes and a reset carrying a world included, and reports what it does not serve as run does. Its Node tests (task check:node_serves_a_specification) replay exchanges the loopback listener answers natively.",
        "/docs/concepts/environment-and-admission#the-node-export",
        "crates/spindle-node/tests/spindle-node/listener.rs::the_loopback_listener_answers_the_world_exchanges",
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
        "Event bindings on stdin",
        "run reading JSON lines on stdin fires the event bindings a served run routes, on its one store: each command a binding maps a line's events to runs in the same step and prints its result on its own line with caused_by (the line and the binding), retried as a served run retries it unless --drop-retries is given. A bound command that is refused or does not run fails the run after the last line; a binding the router refuses stops the run before stdin is read; a specification without components routes as one implied component.",
        "/docs/concepts/components-and-bindings",
        "crates/spindle-cli/tests/spindle-cli/stdin_bindings.rs::a_stdin_line_fires_the_binding_its_event_matches",
    ),
    shipped(
        "Bindings",
        "Retry on failure",
        "An on_failure: retry binding runs its command again at once after each refused or failed attempt, up to 5 attempts, before the next envelope runs; each attempt and each given-up invocation is counted. A per-refusal policy (on_failure with named refusals) retries or drops by refusal, with the same limit. --drop-retries runs every retrying binding as a drop binding.",
        "/docs/concepts/components-and-bindings",
        "crates/spindle-host/tests/spindle-host/retry.rs::a_failing_invocation_is_retried_until_it_succeeds",
    ),
    shipped(
        "Bindings",
        "Escalation on failure",
        "A binding whose on_failure escalates, for every failure or for the refusals an escalate entry names beside drop and retry, invokes its command no further once an attempt is answered so: the component that ran it publishes the escalation event once from the command's input, served, on stdin and in conform, and the bindings that listen for it run. A required event field the input lacks publishes nothing and is reported unmet; an optional one is left out. The event goes to the outbox and is not in the event history. --drop-retries keeps every escalation.",
        "/docs/concepts/components-and-bindings",
        "crates/spindle-conformance/tests/spindle-conformance/escalation.rs::an_escalated_refusal_emits_the_named_event_and_invokes_nothing",
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
        "External refusals before the row",
        "An external: refusal declared before_row: true is asked among the input-guarded refusals, in declaration order, before the addressed row is read: a cause that holds answers the refusal for a row that exists or not, reading no row, and one that does not lets the command go on, as ESS's interpreter answers.",
        "/docs/concepts/specification",
        "crates/spindle-kernel/tests/spindle-kernel/before_row.rs::an_external_refusal_before_the_row_answers_as_the_interpreter",
    ),
    shipped(
        "Execution",
        "Guards over a list of references",
        "A when_related: {via: input.<list>, exists: false} guard over a List of references reads the row each element names and is taken where any names no row, an empty list naming none; {missing: input.<list>} is the first such element in list order, as ESS's interpreter decides and answers it.",
        "/docs/concepts/specification",
        "crates/spindle-kernel/tests/spindle-kernel/reference_list.rs::a_guard_over_a_list_of_references_is_decided_as_the_interpreter",
    ),
    shipped(
        "Execution",
        "Values from the one row a predicate selects",
        "A value {related: {entity, where, field}} in sets: or an event payload reads the field of the one row of the entity the predicate selects, from the store as it was before the outcome; no row or several give no value and nothing is written, as ESS's interpreter decides. A predicate reading payload. or walking the rows of an entity, the value in an error's fields, and a selector over an entity another component owns, stay refused.",
        "/docs/concepts/specification",
        "crates/spindle-kernel/tests/spindle-kernel/where_selected_values.rs::where_selected_values_agree_with_the_interpreter",
    ),
    shipped(
        "Execution",
        "Values read through two rows",
        "A value {related: {via: [input.<field>, <field>], field}} in sets: or an event payload reads the field or the state of the row a reference of the row the input names names, each row from the store as it was before the outcome, as ESS's interpreter reads it: an absent Optional reference on the way leaves the value absent, a row none carries gives no value and nothing is written, a field filled through a declared conversion is checked as a one-hop read is, and a row of an entity another component owns is read only where its owner's store is reached. A chain that starts at a field of the subject stays refused.",
        "/docs/concepts/specification",
        "crates/spindle-kernel/tests/spindle-kernel/related_chain_values.rs::a_value_source_through_two_rows_reads_the_far_row",
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
        "Execution",
        "View filters read through a reference",
        "A view filter that reads a field of the row a reference names (person_id.state == Active), or compares a field with another field or a parameter (person_id.name == holder, person_id.team == param.team), answers the rows ESS's interpreter answers, on the host's thread and through its reader, a row whose reference is empty or names no row left out where the filter is unknown only for that; each referenced row is loaded once per read by its identity.",
        "/docs/concepts/specification",
        "crates/spindle-host/tests/spindle-host/filter_through.rs::filters_through_a_reference_agree_with_the_interpreter",
    ),
    shipped(
        "Execution",
        "View filters testing text",
        "A view filter that tests a text field with contains, starts_with or ends_with against a text parameter or literal (address: {contains: {param: text}}) answers the rows ESS's interpreter answers on every store: byte for byte and case-sensitive, the empty text matching every row, an Optional parameter left out holding no row, a field the row does not hold refusing the read.",
        "/docs/concepts/specification",
        "crates/spindle-host/tests/spindle-host/text_filters.rs::text_filters_agree_with_the_interpreter",
    ),
    shipped(
        "Execution",
        "View filters with always, never and struct members",
        "A view filter that holds always or never, inside a quantifier among them (not (exists t in param.tags: (always))), or reads a member of a struct field of the row, the identity's own members among them (key.integration == param.id), answers the rows ESS's interpreter answers on every store.",
        "/docs/concepts/specification",
        "crates/spindle-host/tests/spindle-host/always_and_key_filters.rs::always_and_key_filters_agree_with_the_interpreter",
    ),
    shipped(
        "Execution",
        "View filters reading parameters and case folds",
        "A view filter that compares a parameter with a literal of its type (param.label == \"\", param.direction == all), reads the count of a list parameter (param.kinds.count == 0), tests a text field, a text parameter or an element of a list parameter with equals_ignore_case or in_ignore_case, alone or under trimmed, or reads a member of an element of a list field (exists g in groups: (g.id == param.group)) answers the rows ESS's interpreter answers, with each parameter left out and given.",
        "/docs/concepts/specification",
        "crates/spindle-host/tests/spindle-host/parameter_literal_filters.rs::the_consumer_filter_shapes_are_served",
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
        "Path variables of every scalar type",
        "A path variable of an ess-http/1 binding whose field is a Boolean, Decimal, Timestamp, Duration or Uuid, or an unconstrained newtype of one such as an id type over Uuid, is read from its percent-decoded segment as ESS's generated servers read it, on a command route and on a view route; a segment the type refuses answers 400, and a binding that puts a constrained newtype in the path is refused by ESS with ESS-HTTP-009.",
        "/docs/concepts/capabilities",
        "crates/spindle-ingress/tests/spindle-ingress/binding_shapes.rs::a_path_variable_of_each_admitted_type_is_read_as_the_type",
    ),
    shipped(
        "Execution",
        "View answer forms",
        "A view an ess-http/1 binding answers with body: array, body: single or either under one member, {<member>: array} or {<member>: single}, answers its rows bare, its first row bare, or the same as the one member of an object, as ess generate --kind openapi publishes it; a single view holding no row answers its empty status and body, unwrapped.",
        "/docs/concepts/capabilities",
        "crates/spindle-ingress/tests/spindle-ingress/binding_shapes.rs::a_wrapped_view_answers_its_rows_or_its_one_row_under_its_member",
    ),
    shipped(
        "Execution",
        "Form and query requests",
        "A command an ess-http/1 binding reads as request: {encoding: form} or {encoding: query} is served: its input is read from an application/x-www-form-urlencoded body or from the query string, each value by its field's type as ESS's served application reads it; an undeclared field, one the path carries, or a form body under another Content-Type is refused 400 and writes nothing. A json or form command whose binding lists request: {query: [input.<field>, ...]} reads those fields from the query string beside its body, a List field from its key repeated, and runs on the joined input; a body naming a listed field is refused 400.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/form_query.rs::a_form_encoded_command_runs",
    ),
    shipped(
        "Execution",
        "Outcome bodies",
        "A command outcome an ess-http/1 binding answers with body: response, body: response.<field> or body: none answers its bound status with the command's response bare, the one field's value bare, or no body and no Content-Type, and one answered with either of the first two under one member, {<member>: response}, with an object whose one member holds it, as ess generate --kind openapi publishes it, with the response headers the binding declares (a WWW-Authenticate challenge among them); one bound with a redirect or the text encoding answers 501 naming which.",
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
        "HTTPS listener and client certificates",
        "run --tls-cert with --tls-key serves HTTPS on the listener, TLS 1.2 and 1.3, and a plain HTTP request gets no HTTP answer; --tls-client-ca requires every client, the proxy in front, to present a certificate that CA signed (mTLS), on every route, /__spindle/live and /__spindle/ready included. A file that cannot be read, holds no certificate or key, or a key that does not match the certificate exits 2 before listening, naming the flag and the file. Off loopback the trusted caller header and secret are still required; a renewed certificate is read by a restart.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/listener_tls.rs::a_served_run_given_a_certificate_answers_https",
    ),
    shipped(
        "Execution",
        "Seed at start",
        "run --seed, or the environment's [seed] file, names a JSON array of commands, each with its input, actor, caller and request id, that a served run submits on every start after its stores open: checked against the specification before listening, then one at a time in the file's order beside the listener. Each counts as applied, already there (an existing_instance: outcome, or a request id an earlier applied or already-there entry carried), refused (a declared refusing outcome) or rejected (the host or the kernel refused it before an outcome); each is logged and the rest still run, the counts are in one log line and in /__spindle/metrics, and a seed that ends with a rejected entry stops the run with exit 1. On a --state directory, seed.json records the seed file's digest and how many entries are done: a restart with the same file sends only the entries not yet done, and one with another file exits 1 before listening unless the state holds no row; a store kept elsewhere records nothing, and every start sends the whole seed. A reset drops the entries not yet sent; with no world it submits the seed again before it answers, and with a world it loads that world alone and records every entry done. GET /__spindle/seed answers whether the seed is running or ended with its counts, and GET /__spindle/ready answers 503 until it has ended and 200 from then on, with or without --observe, so a readiness probe needs no view row.",
        "/docs/concepts/environment-and-admission#a-seed-submitted-at-start",
        "crates/spindle-cli/tests/spindle-cli/seed.rs::a_second_run_on_one_state_applies_nothing_new",
    ),
    shipped(
        "Execution",
        "External causes bound to a decisions file",
        "An environment binds external causes to decisions-file and names the file in a [decisions] table; run, on stdin or serving HTTP, answers them from it as from --decisions, which wins over the table. A cause bound to decisions-file with no file named, or to an implementation run does not provide (anything but operator-prompt, decisions-file and a declared [http.<name>] call), exits 1 before anything runs, naming the capability. A [decisions] table no external key binds is refused when the document is read, and play and deploy refuse one.",
        "/docs/concepts/environment-and-admission#external-causes-answered-from-a-decisions-file",
        "crates/spindle-cli/tests/spindle-cli/decisions.rs::an_environment_binding_external_to_a_decisions_file_answers_from_it",
    ),
    shipped(
        "Execution",
        "A rejected line fails the run",
        "run reading JSON lines on stdin answers every line, then exits 1 when a line was rejected (not JSON, no declared command, input not admitted, an external cause unanswered, or refused by the kernel before an outcome), naming each such line on stderr by its place in the input. A line answered by a declared outcome, a refusing one included, leaves the exit status 0.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/spindle-cli/rejected_line.rs::a_stdin_run_with_a_rejected_line_exits_1",
    ),
    shipped(
        "Execution",
        "Arrange lines and line references on stdin",
        "run reading JSON lines on stdin takes {\"arrange\": [rows]} in the shape a reset's world gives them, checked by ESS and against each entity's invariants and written all or none, before the next line; a seeded mint never mints an identity one wrote. A command's input may name a value an earlier line answered with {\"$line\": n, \"pointer\": \"/events/0/payload/<field>\"}; a reference to a line not yet answered, or to a pointer its answer lacks, rejects the line.",
        "/docs/reference/cli#run",
        "crates/spindle-cli/tests/spindle-cli/stdin_lines.rs::an_arrange_line_writes_rows_a_later_command_reads",
    ),
    shipped(
        "Execution",
        "Simulated clock and seeded mint",
        "run --clock starts a run reading stdin, serving HTTP or replaying --scenario on a manual clock at an RFC 3339 instant; on stdin a line's at moves it forward before its command runs, an earlier instant rejects the line, and a rejected line leaves the clock where it stood. run --mint-seed mints every identity and generated value from a seed on stdin and over HTTP, so two runs of the same commands mint the same identities, never one the kept state already holds; a served reset without a world mints from it again. An environment binding identity.mint to seeded runs on stdin with seed 0, and serves HTTP given --mint-seed. No run fires a periodic binding.",
        "/docs/reference/cli",
        "crates/spindle-cli/tests/spindle-cli/clock_and_mint.rs::a_line_moves_the_clock",
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
        "run --scenario replays an ess-scenario document through ESS's reader and conformance runner: an entity it arranges with setup is written into the store of the component that owns it, captures feed later steps, view asserts are checked, each act's line names the outcome its command took with its declared error, or its refusal, the first divergent step is named and ends the run, an external cause the scenario does not force is answered by --decisions, and a specification that declares no component replays as one implied component that owns every domain.",
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
         The releases, each with its version, date and changes, are in \
         [CHANGELOG.md](https://github.com/beyond10x/spindle/blob/main/CHANGELOG.md).\n\n\
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
