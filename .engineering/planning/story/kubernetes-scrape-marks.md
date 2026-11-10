---
format: aep.planning-md/3
id: story:kubernetes-scrape-marks
kind: story
status: draft
title: Rendered workloads say what to scrape, and Prometheus is never deployed
relations:
- serves: vision:spindle
- decomposes: epic:kubernetes-target
- depends_on: story:kubernetes-render
- depends_on: story:component-process-observe
scope:
- confidence: cited
  path: crates/spindle-cli/src/cli.rs
- confidence: inferred
  path: crates/spindle-kubernetes
revision: 4
---
# Rendered workloads say what to scrape, and Prometheus is never deployed

## Outcome

Every rendered Deployment of a process that answers `/__spindle/metrics`
(`story:component-process-observe`) carries the scrape annotations (`prometheus.io/scrape`,
`prometheus.io/port`, `prometheus.io/path`) and a named metrics port. With
`--monitor pod-monitor` or `--monitor service-monitor`, render also writes one PodMonitor or
ServiceMonitor selecting them. No rendered resource is a Prometheus, an Alertmanager, a collector
or their configuration.

## Acceptance

- `every_observed_workload_is_annotated`: each rendered component and front ingress Deployment
  carries the three annotations, their port naming the metrics listener.
- `no_monitor_without_the_flag`: without `--monitor` no PodMonitor or ServiceMonitor is rendered.
- `pod_monitor_selects_every_workload`: with `--monitor pod-monitor`, exactly one PodMonitor is
  rendered, and its selector matches every rendered workload.
- `no_prometheus_is_rendered`: no rendered resource has a kind or image of Prometheus,
  Alertmanager or a collector.

## Scope

- `crates/spindle-kubernetes/src/` (annotations, monitor documents).
- `crates/spindle-cli/src/cli.rs` (`--monitor`).
