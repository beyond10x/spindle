// story:node-export-serves-a-specification, acceptance 6 to 9: instances, disposal, order, and what
// the module does not reach for.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { Spindle, glue, review, wasm } from "./load.mjs";

const BOUND = { timeout: 60_000 };

const SUBMISSIONS = { method: "GET", path: "/review/views/Submissions" };

function submit(id, amount) {
  return {
    method: "POST",
    path: "/review/commands/Submit",
    headers: [["Content-Type", "application/json"]],
    body: new TextEncoder().encode(JSON.stringify({ submission_id: id, amount })),
  };
}

async function rows(spindle) {
  const response = await spindle.handle(SUBMISSIONS);
  assert.equal(response.status, 200);
  return JSON.parse(new TextDecoder().decode(response.body)).rows;
}

test("two instances share nothing", BOUND, async () => {
  const first = Spindle.create(review());
  const second = Spindle.create(review());
  assert.equal((await first.handle(submit("s1", 1))).status, 202);
  assert.deepEqual(await rows(first), [{ amount: 1, submission_id: "s1" }]);
  assert.deepEqual(await rows(second), []);
  assert.equal((await second.handle(submit("s1", 2))).status, 202);
  assert.deepEqual(await rows(first), [{ amount: 1, submission_id: "s1" }]);
  assert.deepEqual(await rows(second), [{ amount: 2, submission_id: "s1" }]);
  first.dispose();
  second.dispose();
});

test("a disposed instance refuses every call", BOUND, () => {
  const spindle = Spindle.create(review());
  spindle.dispose();
  const disposed = { message: "this Spindle was disposed" };
  assert.throws(() => spindle.handle(SUBMISSIONS), disposed);
  assert.throws(() => spindle.reset(), disposed);
  assert.throws(() => spindle.notServed(), disposed);
  assert.throws(() => spindle.dispose(), disposed);
});

test("requests are answered one at a time in arrival order", BOUND, async () => {
  const spindle = Spindle.create(review());
  const handed = [
    spindle.handle(submit("s1", 1)),
    spindle.handle(SUBMISSIONS),
    spindle.handle(submit("s2", 2)),
    spindle.handle({ ...SUBMISSIONS, path: "/review/views/Submissions?" }),
  ];
  const answered = await Promise.all(handed);
  const listed = answered.map((response) => new TextDecoder().decode(response.body));
  assert.deepEqual(
    answered.map((response) => response.status),
    [202, 200, 202, 200],
  );
  assert.deepEqual(JSON.parse(listed[1]).rows, [{ amount: 1, submission_id: "s1" }]);
  assert.deepEqual(JSON.parse(listed[3]).rows, [
    { amount: 1, submission_id: "s1" },
    { amount: 2, submission_id: "s2" },
  ]);
  spindle.dispose();
});

test("the module reads no file, opens no socket and starts no thread", BOUND, async () => {
  const code = readFileSync(glue, "utf8");
  const imports = [...code.matchAll(/^\s*import\b[^;]*?from\s*["']([^"']+)["']/gm)].map(
    (found) => found[1],
  );
  assert.deepEqual(imports, [], "the glue imports nothing");
  assert.doesNotMatch(code, /\brequire\s*\(/);
  for (const word of ["Worker", "worker_threads", "SharedArrayBuffer", "Atomics"]) {
    assert.ok(!code.includes(word), `the glue names ${word}`);
  }
  const module = await WebAssembly.compile(readFileSync(wasm));
  const memories = WebAssembly.Module.imports(module).filter((entry) => entry.kind === "memory");
  assert.deepEqual(memories, [], "the module imports no memory, shared or not");
  for (const entry of WebAssembly.Module.imports(module)) {
    assert.ok(
      entry.module.startsWith("./spindle_node_bg") || entry.module === "wbg",
      `an import from ${entry.module}`,
    );
  }
});
