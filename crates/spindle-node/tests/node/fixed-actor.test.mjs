// story:node-export-fixed-actor: fixed actors with their callers, as `run --fixed-actor` with
// `--fixed-caller` takes them.

import assert from "node:assert/strict";
import { test } from "node:test";

import { NODE, Spindle, json, replay, sources } from "./load.mjs";

const BOUND = { timeout: 60_000 };

const FIXED_CALLERS = "crates/spindle-cli/tests/fixtures/fixed-callers";

const CLERK = { actor: "bureau.desk.Clerk", caller: { badge: "ada", shift: "early" } };
const KEEPER = { actor: "bureau.desk.Keeper", caller: { key: "brass", shift: "late" } };

function office(fixedActors) {
  return { sources: sources(FIXED_CALLERS), environment: NODE, fixedActors };
}

function refusal(config) {
  try {
    Spindle.create(config);
  } catch (error) {
    assert.ok(error instanceof Error, `an Error: ${error}`);
    return error.message;
  }
  assert.fail("create refused nothing");
}

test("a fixed actor sends each command as run does", BOUND, async () => {
  const recorded = json("crates/spindle-node/tests/fixtures/fixed-actor-exchanges.json");
  const spindle = Spindle.create(office([CLERK, KEEPER]));
  assert.deepEqual(await replay(spindle, recorded), recorded);
  spindle.dispose();
});

test("create refuses an undeclared fixed actor", BOUND, () => {
  assert.equal(
    refusal(office([{ actor: "bureau.desk.Nobody" }])),
    "the fixed actor bureau.desk.Nobody: the specification declares no such actor",
  );
});

test("create refuses a caller its binding refuses", BOUND, () => {
  const message = refusal(office([{ actor: "bureau.desk.Clerk", caller: { badge: "ada" } }]));
  assert.ok(
    message.startsWith(
      "the caller of the fixed actor bureau.desk.Clerk: invalid input: the caller is not a " +
        "caller of `bureau.desk.Clerk`: ",
    ),
    message,
  );
});

test("create refuses a fixed actor given twice", BOUND, () => {
  assert.equal(
    refusal(office([CLERK, KEEPER, { ...CLERK, caller: { badge: "bob", shift: "late" } }])),
    "the fixed actor bureau.desk.Clerk is given twice; the first would always be sent as",
  );
});

test("create refuses a fixed actor of an unknown shape", BOUND, () => {
  const message = refusal(office([{ ...CLERK, attributes: {} }]));
  assert.ok(message.startsWith("the configuration: unknown field `attributes`"), message);
});
