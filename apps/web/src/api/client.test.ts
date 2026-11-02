import { afterEach, expect, it, vi } from "vitest";
import { controllerSource, parseProjects, parseSnapshot } from "./client";
import { EventOrder, consumeEvents, reconnectDelay } from "./events";
import { parseRoute, routeHash } from "../app/route";
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});
it.each([401, 403, 503])(
  "keeps HTTP %i failures distinct without demo fallback",
  async (status) => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("{}", { status })),
    );
    await expect(
      controllerSource.projects(new AbortController().signal),
    ).rejects.toMatchObject({
      kind:
        status === 401
          ? "unauthorized"
          : status === 403
            ? "forbidden"
            : "unavailable",
    });
  },
);
it("rejects malformed and cross-scope snapshots", () => {
  expect(() =>
    parseSnapshot({ schema: 1, provenance: "demo" }, "other"),
  ).toThrow();
  expect(() =>
    parseProjects([{ id: "../leak", name: "X", detail: "" }]),
  ).toThrow();
});
it("orders invalidations, detects gaps and ignores replay", () => {
  const o = new EventOrder("project_a", 2, 3);
  const e = (cursor: number, revision = 4) =>
    JSON.stringify({ schema: 1, project: "project_a", cursor, revision });
  expect(o.accept(e(3), "3")).toBe("refresh");
  expect(o.accept(e(3), "3")).toBe("duplicate");
  expect(o.accept(e(5), "5")).toBe("gap");
  expect(() => o.accept(e(6), "7")).toThrow();
  expect(() => o.accept('{"schema":1,"project":"other"}', "6")).toThrow();
});
it("parses chunked SSE without using keepalives as activity", async () => {
  const events: string[] = [];
  const body = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(new TextEncoder().encode(': keepalive\n\nid: 3\ndata: {"x":'));
      c.enqueue(new TextEncoder().encode("1}\n\n"));
      c.close();
    },
  });
  await consumeEvents(
    body,
    (data, id) => events.push(id + data),
    new AbortController().signal,
  );
  expect(events).toEqual(['3{"x":1}']);
  expect(reconnectDelay(100)).toBe(30000);
});
it("bounds event frames and rejects truncated events", async () => {
  const body = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(new TextEncoder().encode("data: " + "x".repeat(17000)));
      c.close();
    },
  });
  await expect(
    consumeEvents(body, () => undefined, new AbortController().signal),
  ).rejects.toThrow("limit");
});
it("preserves shared task context and rejects route injection", () => {
  const route = {
    section: "execution",
    project: "p1",
    task: "task_A",
  } as const;
  expect(parseRoute(routeHash(route))).toEqual(route);
  expect(parseRoute("#/unknown/..%2Fsecret?task=%3Cscript%3E")).toEqual({
    section: "projects",
    project: null,
    task: null,
  });
});
import { demoSnapshot } from "../testing/demo";
it.each([
  "incomplete",
  "false_acceptance",
  "foreign_attempt",
  "foreign_scope",
  "hidden_data",
])("rejects contradictory controller state: %s", (issue) => {
  const data = demoSnapshot();
  data.provenance = "controller";
  data.connections.forEach((c) => (c.status = "unverified"));
  if (issue === "incomplete") data.plan.complete = false;
  if (issue === "false_acceptance") data.results[0]!.verification = "pending";
  if (issue === "foreign_attempt") data.results[0]!.attempt = "foreign_attempt";
  if (issue === "foreign_scope") data.connections[0]!.scope = "other_project";
  if (issue === "hidden_data") data.capabilities.results = false;
  expect(() => parseSnapshot(data, data.project.id)).toThrow();
});
import controllerContract from "../testing/controller-snapshot.json";
import { streamProject } from "./events";
it("accepts the exact Rust-serialized redacted projection contract", () => {
  expect(parseSnapshot(controllerContract, "project-a").tasks[0]?.title).toBe(
    "Bounded panel task",
  );
});
it("classifies explicit SSE revocation as authorization denial", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue(new Response("", { status: 403 })),
  );
  await expect(
    streamProject(
      "project-a",
      0,
      new AbortController().signal,
      () => undefined,
    ),
  ).rejects.toMatchObject({ kind: "forbidden" });
});

it("bounds slow REST requests and cleans up timeout authority", async () => {
  vi.useFakeTimers();
  vi.stubGlobal(
    "fetch",
    vi.fn(
      (_path, init: RequestInit) =>
        new Promise((_resolve, reject) => {
          init.signal?.addEventListener(
            "abort",
            () => reject(new DOMException("Aborted", "AbortError")),
            { once: true },
          );
        }),
    ),
  );
  const expected = expect(
    controllerSource.projects(new AbortController().signal),
  ).rejects.toMatchObject({ kind: "network" });
  await vi.advanceTimersByTimeAsync(10000);
  await expected;
  expect(vi.getTimerCount()).toBe(0);
});

import { coalesceInvalidations } from "./events";
it("coalesces event bursts to at most two snapshot refreshes per second and cancels on scope exit", async () => {
  vi.useFakeTimers();
  const refresh = vi.fn();
  const queue = coalesceInvalidations(refresh);
  for (let i = 0; i < 1000; i++) queue.request();
  expect(refresh).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(500);
  expect(refresh).toHaveBeenCalledOnce();
  queue.request();
  queue.cancel();
  await vi.advanceTimersByTimeAsync(500);
  expect(refresh).toHaveBeenCalledOnce();
});
it("an open SSE stream reports explicit revocation and reset frames without stale data", async () => {
  for (const [event, code, kind] of [
    ["unauthorized", "unauthorized", "unauthorized"],
    ["forbidden", "forbidden", "forbidden"],
    ["reset", "replay_gap", "invalid"],
  ]) {
    const received = vi.fn();
    const body = new Response(
      `event: ${event}\ndata: {"schema":1,"error":"${code}"}\n\n`,
    ).body!;
    await expect(
      consumeEvents(body, received, new AbortController().signal),
    ).rejects.toMatchObject({ kind });
    expect(received).not.toHaveBeenCalled();
  }
});
