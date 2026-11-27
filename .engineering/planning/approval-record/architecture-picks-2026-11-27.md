---
format: aep.planning-md/3
id: approval-record:architecture-picks-2026-11-27
kind: approval-record
status: accepted
title: Architecture picks for seven stories that change Spindle's architecture
relations:
- decides: story:event-egress-and-dead-letters
- decides: story:verified-caller-token
- decides: story:listener-tls
- decides: story:several-writers-one-postgres
- decides: story:spec-change-over-stored-rows
- decides: story:spindle-image-published
- decides: story:one-ingress-several-specifications
revision: 3
transitions:
- {from: "draft", to: "accepted", at: "2026-11-27T09:56:32Z", actor: "human:operator", revision: 2}
---
## Words

The operator, through the host operator, 2026-11-27 at about 10:00 CET: "A: all 7 as recommended".
The host operator relayed it at 10:51 CET with the order to record the picks and make the six
stories that change the code ready for waves after wave 30.

## The picks

- `story:event-egress-and-dead-letters`: A, webhook delivery and stored dead letters.
- `story:verified-caller-token`: A, JWT and JWKS beside `--trusted-caller-header`.
- `story:listener-tls`: A, server TLS and optional client certificates.
- `story:several-writers-one-postgres`: A, one leader with a lease and a fencing token.
- `story:spec-change-over-stored-rows`: A, refuse to start on a shape mismatch.
- `story:one-ingress-several-specifications`: B, no change; the story is archived.
- `story:spindle-image-published`: A, a distroless image for amd64 and arm64 on `ghcr.io`; the
  first push is approved.

Each story's section "Architecture decision" holds its option and the options not taken. The six
stories picked A move to `proposed`; each still gets a story-scoper's Scope and the plan critics
before a wave takes it.
