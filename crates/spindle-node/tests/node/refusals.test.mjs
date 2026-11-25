// story:node-export-serves-a-specification, acceptance 4 and 5: the not-served report, and what
// `create` refuses before it serves anything.

import assert from "node:assert/strict";
import { test } from "node:test";

import { NODE, REVIEW, Spindle, json, review, sources, text } from "./load.mjs";

const BOUND = { timeout: 60_000 };

const OUTCOMES = "crates/spindle-cli/tests/fixtures/outcome-scoped-refusal";
const UNSERVABLE = "crates/spindle-cli/tests/fixtures/unservable";

function refusal(config) {
  try {
    Spindle.create(config);
  } catch (error) {
    assert.ok(error instanceof Error, `an Error: ${error}`);
    return error.message;
  }
  assert.fail("create refused nothing");
}

// The Node environment with one line of its [provides] table replaced.
function providing(capability, implementations) {
  return NODE.replace(
    new RegExp(`^"?${capability.replace(".", "\\.")}"? = .*$`, "m"),
    `"${capability}" = ${JSON.stringify(implementations)}`,
  );
}

test("create returns the not-served report", BOUND, () => {
  const spindle = Spindle.create({
    sources: sources(OUTCOMES),
    environment: NODE,
    binding: text("crates/spindle-cli/tests/fixtures/outcome-scoped-refusal-http.yaml"),
  });
  assert.deepEqual(spindle.notServed(), json("crates/spindle-node/tests/fixtures/not-served.json"));
  spindle.dispose();
});

test("refuses a configuration with an unknown key", BOUND, () => {
  const message = refusal({ ...review(), port: 1 });
  assert.ok(message.startsWith("the configuration: unknown field `port`"), message);
});

test("refuses a specification that does not compile", BOUND, () => {
  const message = refusal({ ...review(), sources: [sources(REVIEW)[0]] });
  assert.ok(message.startsWith("the specification: "), message);
});

test("refuses an environment that does not parse", BOUND, () => {
  const message = refusal({ ...review(), environment: "not toml =" });
  assert.ok(message.startsWith("the environment: "), message);
});

test("refuses an environment that does not admit a component", BOUND, () => {
  const environment = NODE.replace(/^"external\.\*" = .*\n/m, "");
  assert.notEqual(environment, NODE);
  const message = refusal({ ...review(), environment });
  assert.equal(
    message,
    "refused on node: no implementation for\n" +
      "  probe: external.probe.review.Decide.vetoed, external.probe.review.Submit.escalated, external.probe.review.Submit.flagged",
  );
});

test("refuses state not bound to memory", BOUND, () => {
  const message = refusal({ ...review(), environment: providing("store.durable", ["sqlite"]) });
  assert.equal(
    message,
    "probe has store.durable bound to `sqlite`; the Node export keeps state in `memory` only",
  );
});

test("refuses an ingress other than node", BOUND, () => {
  const message = refusal({
    ...review(),
    environment: providing("ingress.network", ["http-loopback"]),
  });
  assert.equal(
    message,
    "probe has ingress.network bound to `http-loopback`; the Node export serves `node` only",
  );
});

test("refuses a decisions file it cannot read", BOUND, () => {
  const message = refusal({ ...review(), decisions: "[nope" });
  assert.ok(message.startsWith("the decisions file: "), message);
});

test("refuses external outcomes nothing answers", BOUND, () => {
  const { decisions: _, ...config } = review();
  const message = refusal(config);
  assert.equal(
    message,
    "unsupported: probe has external.probe.review.Decide.vetoed bound to decisions-file; this host answers no external cause",
  );
});

test("refuses a surface that serves no operation", BOUND, () => {
  const message = refusal({ sources: sources(UNSERVABLE), environment: NODE });
  assert.equal(
    message,
    [
      "not served: command `notes.pad.CopyNote` at POST /pad/commands/CopyNote: this ingress does not serve copied, which answers with the command's response",
      "not served: command `notes.pad.DiscardNote` at POST /pad/commands/DiscardNote: unsupported: `input_absent:`, in `notes.pad.DiscardNote/body-missing`",
      "pad serves no operation: each one above is one this ingress cannot route or the kernel does not run",
    ].join("\n"),
  );
});
