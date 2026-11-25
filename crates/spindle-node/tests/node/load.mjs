// Loads the Node export for the tests beside this file: the directory SPINDLE_NODE_MODULE names,
// or the lane's own output, target/spindle-node, handed to the module as bytes.

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const root = fileURLToPath(new URL("../../../../", import.meta.url));

export const directory = process.env.SPINDLE_NODE_MODULE ?? join(root, "target", "spindle-node");

export const glue = join(directory, "spindle_node.js");

export const wasm = join(directory, "spindle_node_bg.wasm");

const loaded = await import(pathToFileURL(glue).href);
loaded.initSync({ module: readFileSync(wasm) });

export const { Spindle } = loaded;

export function text(path) {
  return readFileSync(join(root, path), "utf8");
}

export function json(path) {
  return JSON.parse(text(path));
}

// A specification's source files under `directory`, each as [name, text], named as `run` names
// them: `system.yaml` and `domains/<domain>.yaml`.
export function sources(directory) {
  const named = [["system.yaml", text(join(directory, "system.yaml"))]];
  for (const file of readdirSync(join(root, directory, "domains")).sort()) {
    named.push([`domains/${file}`, text(join(directory, "domains", file))]);
  }
  return named;
}

export const REVIEW = "crates/spindle-wasm/tests/fixtures/review";

export const NODE = text("examples/environments/node.toml");

// The review fixture with the Node environment and, unless `extra` says otherwise, its decisions
// file.
export function review(extra = {}) {
  return {
    sources: sources(REVIEW),
    environment: NODE,
    decisions: text(`${REVIEW}/decisions.toml`),
    ...extra,
  };
}

// An exchange's request as `handle` takes it.
export function request(exchange) {
  const headers = exchange.content_type ? { "content-type": exchange.content_type } : {};
  return { method: exchange.method, path: exchange.target, headers, body: exchange.body };
}

// What `spindle` answered, in the shape of a recorded exchange: the status, the `allow` header
// and the body as text.
export async function replay(spindle, exchanges) {
  const answered = [];
  for (const exchange of exchanges) {
    const response = await spindle.handle(request(exchange));
    const allow = response.headers.find(([name]) => name === "allow");
    answered.push({
      ...exchange,
      status: response.status,
      allow: allow ? allow[1] : null,
      answer: new TextDecoder().decode(response.body),
    });
  }
  return answered;
}
