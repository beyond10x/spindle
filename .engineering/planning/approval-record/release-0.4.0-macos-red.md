---
format: aep.planning-md/3
id: approval-record:release-0.4.0-macos-red
kind: approval-record
status: accepted
title: What to do when 0.4.0's tagged commit is red on macOS
relations:
- decides: story:outcome-scoped-refusal
revision: 2
transitions:
- {from: "draft", to: "accepted", at: "2026-11-07T02:11:48Z", actor: "human:timo", revision: 2}
---
# What to do when 0.4.0's tagged commit is red on macOS

## Words

The orchestrator, relaying the operator's grant, 2026-11-07 (class C):

> spindle 0.4.0 Release run 42326722964 failed. Its job "Gate green on the tagged commit" found
> main's CI run 42326647238 on 2c211a9 red in the macOS job, so the upload step was skipped and the
> release page has 0 assets. Failing test:
> `adversary_links::a_second_hello_before_the_link_opens_is_refused`, panicked at
> crates/spindle-host/tests/spindle-host/adversary_links.rs:275:5 (83 passed, 1 failed). Same job
> on the PR head: passed on 058292b.
>
> 1. Rerun the failed jobs of CI run 42326647238 through the bot.
> 2. If macOS goes green, it is a timing flake. Rerun the failed jobs of Release run 42326722964
>    through the bot. File a draft that makes that test independent of timing, and name it in the
>    verify notes.
> 3. If it fails again, it is a real failure. Find the cause and fix it in a 0.4.1 through the same
>    wave path. Leave 0.4.0 as a tag with no assets, and record it as not released.

## What it decides

How the 0.4.0 release proceeds after the macOS job on the tagged commit failed: one rerun decides
between a timing flake (rerun the Release run, file a draft) and a real failure (0.4.0 not released,
a fix in 0.4.1).
