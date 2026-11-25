// story:node-export-serves-a-specification, acceptance 1 to 3: the Node export answers what the
// loopback listener answers natively.

import assert from "node:assert/strict";
import { test } from "node:test";

import { NODE, REVIEW, Spindle, json, replay, review, sources } from "./load.mjs";

const BOUND = { timeout: 60_000 };

test("answers the recorded exchanges as the loopback listener does", BOUND, async () => {
  const recorded = json(`${REVIEW}/exchanges.json`);
  const spindle = Spindle.create(review());
  assert.deepEqual(await replay(spindle, recorded), recorded);
  spindle.dispose();
});

test("the decisions file answers external outcomes as run --decisions does", BOUND, async () => {
  const recorded = json(`${REVIEW}/exchanges.json`);
  const decide = recorded.findIndex((exchange) => exchange.target === "/review/commands/Decide");
  const spindle = Spindle.create(JSON.stringify(review()));
  const answered = await replay(spindle, recorded.slice(0, decide + 1));
  assert.equal(answered[decide].status, 502);
  assert.deepEqual(answered[decide], recorded[decide]);
  spindle.dispose();
});

const WORLD = "crates/spindle-node/tests/fixtures/world-exchanges.json";

function answeringDefaults() {
  return { sources: sources(REVIEW), environment: NODE, answerDefault: true };
}

test("a reset carries a world as the native reset route does", BOUND, async () => {
  const recorded = json(WORLD);
  const spindle = Spindle.create(answeringDefaults());
  assert.deepEqual(await replay(spindle, recorded), recorded);

  const first = recorded.find((exchange) => exchange.target === "/api/reset");
  const reset = await spindle.reset(JSON.parse(first.body));
  assert.equal(reset.status, first.status);
  assert.equal(new TextDecoder().decode(reset.body), first.answer);
  const emptied = await spindle.reset();
  assert.equal(emptied.status, 200);
  spindle.dispose();
});

test("the answer default answers external outcomes as run --answer-default does", BOUND, async () => {
  const world = json(WORLD);
  const recorded = world.slice(0, world.findIndex((exchange) => exchange.target === "/api/reset"));
  const commands = recorded.map((exchange) => exchange.target);
  assert.deepEqual(commands, ["/review/commands/Submit", "/review/commands/Decide"]);
  const spindle = Spindle.create(answeringDefaults());
  assert.deepEqual(await replay(spindle, recorded), recorded);
  spindle.dispose();
});
