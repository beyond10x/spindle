---
format: aep.planning-md/3
id: epic:application-manifest
kind: epic
status: draft
title: An application manifest and one command that starts what it lists
relations:
- serves: vision:spindle
- informed_by: epic:browser-host
- informed_by: epic:deployment-targets
revision: 1
---
# An application manifest and one command that starts what it lists

## Outcome

One YAML manifest names everything an application needs to run on spindle, and one command starts
it: the specifications (a path, or a git repository with ref and subpath), each one's `ess-http/1`
binding and decisions file, the environment and placement, and the web frontends (a static
directory, a git repository with a build command, or a container image) mounted on URL paths beside
the bound routes, same-origin.

- `b10x-spindle up <manifest>` admits and serves every specification, launches the frontends, prints
  the URLs and stops everything on SIGINT.
- `b10x-spindle bundle <manifest> --static <dir>` writes one static site that carries the
  `epic:browser-host` wasm host and Service Worker, for any static host.

## Status

Draft. It changes the architecture: the design goes to the operator as an ADR (decider: the
operator) before any story is accepted. Not accepted.

## Today

A consumer runs `b10x-spindle run <spec> --environment <env> --decisions <file> --http <binding>
--port N` beside a Vite dev server that proxies `/spindle`, wrapped in a Taskfile.

## Origin

The operator, 2026-11-06, relayed by `b10x-chef-orchestrator` from another session (filing approved,
"A"): "one command where I say, hey, here is a web app I want to deploy in Spindle, and here's the
spec I want to deploy it. So basically, like a manifest for Spindle, which is like a YAML file, and
we can like even give it Docker images, sort of, yeah, or repos, roots, and it would then basically
bind them all into the like deployment".
