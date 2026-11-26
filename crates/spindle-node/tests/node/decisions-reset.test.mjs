// story:decisions-rules-on-input-predicates: a reset keeps the decisions file the instance was
// created with, its rules included, and a world's rules decide until the next reset.

import assert from "node:assert/strict";
import { test } from "node:test";

import { Spindle, review } from "./load.mjs";

const BOUND = { timeout: 60_000 };

const DECISIONS = `flagged = false
escalated = false

["probe.review.Decide"]
vetoed = true

[[rules]]
command = "probe.review.Submit"
outcome = "flagged"
answer = true
when = { "input.amount" = 13 }
`;

function submit(id, amount) {
  return {
    method: "POST",
    path: "/review/commands/Submit",
    headers: [["Content-Type", "application/json"]],
    body: new TextEncoder().encode(JSON.stringify({ submission_id: id, amount })),
  };
}

async function outcome(spindle, request) {
  const response = await spindle.handle(request);
  const body = JSON.parse(new TextDecoder().decode(response.body));
  return [response.status, body.outcome];
}

test("a reset keeps the decisions file and its rules", BOUND, async () => {
  const spindle = Spindle.create(review({ decisions: DECISIONS }));
  const reset = await spindle.reset();
  assert.equal(reset.status, 200);
  const [status, flagged] = await outcome(spindle, submit("s1", 13));
  assert.equal(flagged, "flagged", "the file's rule decides after the reset");
  assert.ok(status >= 400, `a refusal: ${status}`);
  assert.deepEqual(await outcome(spindle, submit("s2", 5)), [202, "submitted"]);
  spindle.dispose();
});

test("a reset's world with rules decides by them", BOUND, async () => {
  const spindle = Spindle.create(review({ decisions: DECISIONS }));
  const world = {
    decisions: {
      rules: [
        {
          command: "probe.review.Submit",
          outcome: "escalated",
          answer: true,
          when: { "input.submission_id": "s7" },
        },
      ],
    },
  };
  assert.equal((await spindle.reset(world)).status, 200);
  const [, escalated] = await outcome(spindle, submit("s7", 5));
  assert.equal(escalated, "escalated", "the world's rule decides");
  const [, flagged] = await outcome(spindle, submit("s8", 13));
  assert.equal(flagged, "flagged", "the file's rule decides what the world's does not");
  assert.deepEqual(await outcome(spindle, submit("s9", 5)), [202, "submitted"]);
  spindle.dispose();
});
