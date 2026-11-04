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

it("accepts versioned ledger data with unavailable forecasts without inventing zeros", () => {
  const data = demoSnapshot();
  const value = structuredClone(data) as unknown as Record<string, unknown>;
  value.schema = 2;
  value.provenance = "controller";
  value.attempts = [];
  value.connections = [];
  value.results = [];
  value.activity = [];
  value.pages = {};
  value.eventCursor = `${data.cursor}.` + "a".repeat(64);
  value.capabilities = {
    ...data.capabilities,
    mutations: false,
    budgetIncrease: false,
    execution: true,
    activity: true,
  };
  value.finance = {
    currency: "USD",
    version: 1,
    ceiling: "1200",
    reserved: "100",
    settled: "20",
    provisional: "5",
    available: null,
    unknownLiabilities: 1,
    overCeiling: false,
    paused: true,
    forecasts: null,
    categories: ["ai", "compute", "storage", "transfer"].map((id, i) => ({
      id,
      settled: i === 0 ? "20" : "0",
      reserved: i === 0 ? "100" : "0",
      provisional: i === 0 ? "5" : "0",
      forecast: null,
    })),
    approvals: [],
  };
  expect(parseSnapshot(value, data.project.id).finance?.forecasts).toBeNull();
  const recovery = structuredClone(value);
  recovery.recoveryRequired = true;
  const recoveredFinance = recovery.finance as Record<string, unknown>;
  recoveredFinance.unknownLiabilities = 0;
  recoveredFinance.recoveryRequired = true;
  expect(
    parseSnapshot(recovery, data.project.id).finance?.available,
  ).toBeNull();
  const malformedRecovery = structuredClone(recovery);
  malformedRecovery.recoveryRequired = "true";
  expect(() => parseSnapshot(malformedRecovery, data.project.id)).toThrow();
  const forged = structuredClone(value);
  (forged.finance as Record<string, unknown>).settled = "0";
  expect(() => parseSnapshot(forged, data.project.id)).toThrow();
});
it("scoped version-two SSE IDs remain invalidations and reject substituted frame identities", () => {
  const order = new EventOrder("project-a", 2, 2);
  const frame = (cursor: number) =>
    JSON.stringify({
      schema: 2,
      project: "project-a",
      cursor,
      revision: cursor,
    });
  expect(order.accept(frame(3), "3." + "a".repeat(64))).toBe("refresh");
  expect(order.accept(frame(3), "3." + "a".repeat(64))).toBe("duplicate");
  expect(order.accept(frame(9), "9." + "b".repeat(64))).toBe("gap");
  expect(() => order.accept(frame(10), "9." + "b".repeat(64))).toThrow();
});
it("rejects forged paid qualification and private qualification in Viewer snapshots", () => {
  for (const kind of ["paid", "viewer", "domain"] as const) {
    const value = structuredClone(controllerContract) as unknown as Record<
      string,
      unknown
    >;
    const qualification = value.qualification as Record<string, unknown>;
    if (kind === "paid") qualification.paidExecutionAllowed = true;
    else if (kind === "viewer") value.role = "viewer";
    else qualification.blockers = ["worker_claimed_success"];
    expect(() => parseSnapshot(value, "project-a")).toThrow();
  }
});
