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
        "crates/spindle-spec/tests/load.rs::a_refused_document_reports_what_ess_said",
    ),
    shipped(
        "Capabilities",
        "Derived from the specification",
        "Entities need store.durable, periodic bindings timer and clock, cross-component bindings channel, external outcomes external.<command>.<outcome>.",
        "/docs/concepts/capabilities",
        "crates/spindle-capability/tests/admission.rs::a_periodic_cause_needs_a_timer_and_a_clock_and_a_cross_component_binding_a_channel",
    ),
    shipped(
        "Capabilities",
        "Per component",
        "capabilities --component lists only what one component needs when it runs on its own.",
        "/docs/concepts/capabilities",
        "crates/spindle-capability/tests/admission.rs::each_component_needs_only_what_it_runs",
    ),
    shipped(
        "Admission",
        "Bind by preference",
        "Each required capability is bound to the first implementation the environment lists for it; an exact key wins over a prefix key.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-capability/tests/admission.rs::admission_takes_the_most_preferred_implementation",
    ),
    shipped(
        "Admission",
        "Refusal names what is missing",
        "A refused admission names every capability without an implementation and exits 3.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-cli/tests/admit.rs::refused_exits_three_and_names_the_missing_capability",
    ),
    shipped(
        "Execution",
        "Commands against the compiled model",
        "spindle-kernel selects the outcome by the IR's conditions and guards, applies its effect and emits its events; b10x-spindle run reads commands as JSON lines.",
        "/docs/reference/cli",
        "crates/spindle-kernel/tests/deployment.rs::offer_then_admit_moves_a_deployment_to_admitted",
    ),
    shipped(
        "Execution",
        "Memory and SQLite stores",
        "The store follows the store.durable binding; SQLite state survives a restart.",
        "/docs/concepts/environment-and-admission",
        "crates/spindle-kernel/tests/deployment.rs::state_survives_a_restart_on_sqlite",
    ),
    shipped(
        "Bindings",
        "Periodic bindings",
        "spindle-timer yields due ticks for periodic bindings on an injected clock, honouring the ESS periodic profile.",
        "/docs/concepts/components-and-bindings",
        "crates/spindle-timer/tests/periodic.rs::a_periodic_binding_is_due_every_period",
    ),
    shipped(
        "Bindings",
        "Event bindings in process and over TCP",
        "spindle-channel carries an event to the command it binds to, in process or as length-prefixed JSON frames over TCP; at_least_once redelivers.",
        "/docs/concepts/components-and-bindings",
        "crates/spindle-channel/tests/channel.rs::an_event_reaches_the_bound_component_over_tcp",
    ),
    shipped(
        "Rally",
        "Every component in one process",
        "b10x-spindle play runs the three rally components in one process with in-process channels until a side wins.",
        "/docs/examples/rally",
        "crates/spindle-host/tests/rally.rs::a_match_played_in_one_process_ends_with_a_winner",
    ),
    shipped(
        "Targets",
        "The target contract and the process launcher",
        "Placement refuses an instance above its target's ceiling; the process launcher starts and stops an instance and reports its exit.",
        "/docs/concepts/deployment-targets",
        "crates/spindle-target/tests/target.rs::the_process_launcher_starts_and_stops_an_instance_and_reports_its_exit",
    ),
    shipped(
        "Targets",
        "QEMU microvm launcher",
        "A library launcher: one qemu-system-x86_64 -M microvm per instance with KVM, without root. Its tests run on a host with /dev/kvm (task check:vm), not in CI.",
        "/docs/concepts/deployment-targets",
        "crates/spindle-qemu/tests/microvm.rs::a_qemu_microvm_instance_boots_and_runs_b10x_spindle",
    ),
    shipped(
        "Targets",
        "Firecracker launcher",
        "A library launcher: Firecracker VMs on a bridge inside a user and network namespace, without root. Its tests need /dev/kvm, the Firecracker binary and a guest kernel (task check:vm), not in CI.",
        "/docs/concepts/deployment-targets",
        "crates/spindle-firecracker/tests/firecracker.rs::a_firecracker_instance_boots_and_runs_b10x_spindle",
    ),
    shipped(
        "Execution",
        "External decisions from a file",
        "run --decisions answers external outcomes from a TOML file, per command or for all; --answer-default takes a command's default branch.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/decisions.rs::a_decisions_file_answers_every_external_outcome",
    ),
    shipped(
        "Execution",
        "Minted creation identities",
        "A creation whose identity no input or payload supplies needs identity.mint; a Uuid identity is minted as a UUIDv4, a String newtype identity as its prefix and a UUIDv4's text, and emitted.",
        "/docs/concepts/capabilities",
        "crates/spindle-kernel/tests/mint.rs::a_creation_without_an_id_input_mints_one_and_emits_it",
    ),
    shipped(
        "Execution",
        "List views with filters",
        "List views over every instance of an entity, with a state filter and an order.",
        "/docs/concepts/specification",
        "crates/spindle-host/tests/views.rs::a_state_filter_keeps_only_instances_in_that_state",
    ),
    shipped(
        "Observe",
        "Counters derived from the specification",
        "spindle-telemetry counts commands by outcome, events, bindings, timer ticks and store writes per component; b10x-spindle metrics prints the catalogue, play prints the table's counters.",
        "/docs/concepts/observe",
        "crates/spindle-telemetry/tests/catalogue.rs::every_command_outcome_and_event_of_a_component_has_a_counter",
    ),
    shipped(
        "Observe",
        "Host-side resource sampling",
        "Launchers read CPU seconds and resident memory per instance from /proc and its share of the requested quota.",
        "/docs/concepts/observe",
        "crates/spindle-target/tests/sampling.rs::cpu_share_is_cpu_seconds_over_wall_time_and_requested_vcpus",
    ),
    shipped(
        "Observe",
        "Telemetry transport",
        "An instance that starts the spindle-telemetry sender sends its cumulative counters and its own resource readings as spindle-telemetry/1 frames every second, over a Unix socket for a process and vsock for QEMU and Firecracker, and drops and counts frames no collector takes.",
        "/docs/concepts/observe",
        "crates/spindle-telemetry/tests/transport.rs::a_lost_frame_loses_no_count",
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
    planned(
        "Rally",
        "Three processes",
        "Three b10x-spindle run processes, one per component, joined by TCP channels.",
        "/docs/examples/rally",
    ),
    planned(
        "Rally",
        "Three microVMs",
        "Each rally component in its own microVM, joined by TCP channels across the VM network.",
        "/docs/examples/rally",
    ),
    shipped(
        "Execution",
        "HTTP ingress",
        "run serves the accepted commands and views over HTTP on 127.0.0.1 where ingress.network is bound to http-loopback: at the routes ess generate --kind openapi publishes, with or without an ess-http/1 binding, answering the envelope ESS publishes.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/ingress.rs::a_bound_route_answers_with_the_bindings_status_per_outcome",
    ),
    shipped(
        "Execution",
        "Console routes",
        "A loopback run also serves GET /api/model (the IR as ess specify compile prints it, and each outcome's subject), GET /api/events (every event published, in order) and POST /api/reset (empties every store); a route the binding or ESS publishes at one of those paths is refused at startup.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/console.rs::reset_empties_every_store_of_the_run",
    ),
    shipped(
        "Execution",
        "Serve what runs",
        "An HTTP run serves every command and view that runs and answers each other one 501 with ESS's envelope, naming the construct; the list is printed at startup. on_failure: retry bindings start only with --drop-retries, and sqlite state without --state is kept in a temporary directory removed when the run ends.",
        "/docs/concepts/capabilities",
        "crates/spindle-cli/tests/unserved.rs::an_unrunnable_command_answers_501_and_the_rest_serve",
    ),
    shipped(
        "Conformance",
        "Scenario replay",
        "run --scenario replays an ess-scenario document through ESS's reader and conformance runner: captures feed later steps, view asserts are checked, and the first divergent step is named and ends the run.",
        "/docs/concepts/specification",
        "crates/spindle-cli/tests/replay.rs::the_first_divergent_step_is_named",
    ),
    planned(
        "Conformance",
        "Self-conformance",
        "The suite ESS synthesises from Spindle's own specification runs in task check.",
        "/docs/concepts/specification",
    ),
    planned(
        "Observe",
        "The control process",
        "b10x-spindle deploy places and launches instances, collects their samples and serves the read-only observe API on 127.0.0.1; the UI then shows live data and is embedded in the binary.",
        "/docs/concepts/observe",
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
