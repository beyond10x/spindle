---
format: aep.planning-md/3
id: story:settle-error-keeps-report
kind: story
status: draft
title: A failed settle keeps the unmet inputs it gathered
relations:
- informed_by: review-result:w17-u7-adversary-1
- serves: vision:spindle
revision: 1
---
## Outcome

A request whose command committed and whose settling then fails keeps what settling observed before
the failure. Its request record lists every unmet binding input gathered up to that point, whatever
the error.

## Why

Wave 17 (story:binding-delivery-specified, review-result:w17-u7-adversary-1 finding 1) made
`HostError::Runaway` carry the partial report's failures, so a runaway request lists its unmet
inputs. Every other error `Host::settle` returns after the command committed still drops them:

- `Channel`, from receive, acknowledge and collect_acknowledgements in `crates/spindle-host/src/host.rs` settle;
- `Store` and the other errors through invoke → execute;
- `Peer`, on the remote path (`crates/spindle-host/src/host/component.rs`).

The RequestRecord.unmet summary in `ess/domains/observe.yaml` states this limit as of wave 17.

## Acceptance

- One host-wide shape lets a committed call return its partial report beside any later error.
- For each of Channel, Store and Peer, a host test makes the error occur after an unmet input was
  gathered, and the record lists that input.
- The RequestRecord.unmet summary drops the limit.
