import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFile } from "node:fs/promises";
const template = await readFile(
  new URL("../public/sw.js", import.meta.url),
  "utf8",
);
function harness() {
  const listeners = {},
    stored = new Map(),
    deleted = [],
    requests = [];
  let skipped = 0,
    claimed = 0;
  const scope = {
    location: { origin: "https://control.invalid" },
    addEventListener: (k, v) => (listeners[k] = v),
    skipWaiting: () => {
      skipped++;
    },
    clients: {
      claim: () => {
        claimed++;
      },
    },
  };
  const cache = {
    put: async (k, v) => stored.set(k, v),
    match: async (k) => stored.get(k),
    keys: async () => [...stored.keys()],
  };
  const context = {
    self: scope,
    URL,
    Request,
    Response,
    fetch: async (req) => {
      requests.push(req);
      return new Response("shell", {
        headers: { "X-Rolvya-Static": "public-shell-v1" },
      });
    },
    caches: {
      open: async () => cache,
      keys: async () => ["rolvya-shell-v1-old", "unrelated-cache"],
      delete: async (k) => deleted.push(k),
    },
  };
  vm.runInNewContext(
    template
      .replaceAll("__CACHE_NAME__", "rolvya-shell-v1-test")
      .replace(
        "__SHELL_PATHS__",
        JSON.stringify([
          "/",
          "/index.html",
          "/app.12345678.js",
          "/manifest.webmanifest",
        ]),
      ),
    context,
  );
  return {
    context,
    listeners,
    stored,
    deleted,
    requests,
    get skipped() {
      return skipped;
    },
    get claimed() {
      return claimed;
    },
  };
}
test("SW ignores authenticated/private APIs, queries, foreign URLs and mutations", () => {
  const h = harness();
  for (const [url, init] of [
    ["https://control.invalid/api/projects", {}],
    ["https://control.invalid/?token=x", {}],
    ["https://else.invalid/app.12345678.js", {}],
    ["https://control.invalid/", { method: "POST", body: "private" }],
    [
      "https://control.invalid/",
      { headers: { Authorization: "Bearer synthetic" } },
    ],
  ]) {
    let responded = false;
    h.listeners.fetch({
      request: new Request(url, init),
      respondWith: () => (responded = true),
    });
    assert.equal(responded, false, url);
  }
});
test("SW requires explicit same-origin activation and cleans only its own old cache", async () => {
  const h = harness();
  h.listeners.message({
    data: { type: "ACTIVATE" },
    origin: "https://evil.invalid",
    source: { url: "https://evil.invalid/" },
    waitUntil: () => undefined,
  });
  assert.equal(h.skipped, 0);
  h.listeners.message({
    data: { type: "ACTIVATE" },
    origin: "https://control.invalid",
    source: { url: "https://control.invalid/" },
    waitUntil: () => undefined,
  });
  assert.equal(h.skipped, 1);
  let completion;
  h.listeners.activate({ waitUntil: (p) => (completion = p) });
  await completion;
  assert.deepEqual(h.deleted, ["rolvya-shell-v1-old"]);
  assert.equal(h.claimed, 1);
});
test("SW rejects missing, opaque or foreign message origins even with a same-origin source URL", () => {
  const h = harness();
  for (const origin of [undefined, "", "null", "https://evil.invalid"]) {
    h.listeners.message({
      origin,
      data: { type: "ACTIVATE" },
      source: { url: "https://control.invalid/" },
      waitUntil: () => undefined,
    });
  }
  assert.equal(h.skipped, 0);
});

test("old SW never replaces its coherent HTML with a different generation", async () => {
  const h = harness();
  const old = new Response('<script src="/app.12345678.js"></script>');
  h.stored.set("/", old);
  h.context.fetch = async () => {
    const r = new Response('<script src="/app.abcdefgh.js"></script>', {
      headers: { "X-Rolvya-Static": "public-shell-v1" },
    });
    Object.defineProperty(r, "type", { value: "basic" });
    return r;
  };
  let response;
  h.listeners.fetch({
    request: new Request("https://control.invalid/"),
    respondWith: (p) => (response = p),
  });
  await response;
  assert.equal(
    h.stored.get("/"),
    old,
    "new HTML must not corrupt old shell cache",
  );
});
