import {
  isId,
  statusLabels,
  type ProjectSummary,
  type ReadSource,
  type Snapshot,
} from "./model";
export class ReadError extends Error {
  constructor(
    readonly kind:
      "unauthorized" | "forbidden" | "unavailable" | "invalid" | "network",
  ) {
    super(kind);
    this.name = "ReadError";
  }
}
const object = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);
const text = (v: unknown, max = 512): v is string =>
  typeof v === "string" &&
  v.length <= max &&
  ![...v].some((c) => c.charCodeAt(0) < 32 && !["\t", "\n", "\r"].includes(c));
const id = (v: unknown): v is string => text(v, 64) && isId(v);
const integer = (v: unknown): v is number =>
  Number.isSafeInteger(v) && Number(v) >= 0;
const list = (
  v: unknown,
  max: number,
  check: (item: unknown) => boolean,
): v is unknown[] => Array.isArray(v) && v.length <= max && v.every(check);
const strings = (v: unknown, max = 100) => list(v, max, (x) => text(x));
const nullable = (v: unknown, max = 512) => v === null || text(v, max);
const money = (v: unknown) =>
  typeof v === "string" &&
  /^(0|[1-9][0-9]{0,18})$/.test(v) &&
  BigInt(v) <= 9223372036854775807n;
const time = (v: unknown) =>
  text(v, 40) && /^\d{4}-\d\d-\d\dT/.test(v) && Number.isFinite(Date.parse(v));
const member = (v: unknown, values: readonly unknown[]) => values.includes(v);
export function parseProjects(v: unknown): ProjectSummary[] {
  if (
    !list(
      v,
      100,
      (x) => object(x) && id(x.id) && text(x.name, 120) && text(x.detail, 300),
    ) ||
    new Set(v.map((x) => (x as ProjectSummary).id)).size !== v.length
  )
    throw new ReadError("invalid");
  return v as ProjectSummary[];
}
export function parseSnapshot(v: unknown, project: string): Snapshot {
  if (
    !object(v) ||
    v.schema !== 1 ||
    v.provenance !== "controller" ||
    !integer(v.revision) ||
    !integer(v.cursor) ||
    !time(v.asOf) ||
    !member(v.role, ["owner", "editor", "viewer"]) ||
    !object(v.project) ||
    v.project.id !== project ||
    !text(v.project.name, 120) ||
    !text(v.project.goal, 4000) ||
    !nullable(v.project.repository) ||
    !nullable(v.project.repositoryId, 32) ||
    !object(v.plan) ||
    !id(v.plan.id) ||
    !integer(v.plan.version) ||
    !text(v.plan.fingerprint, 64) ||
    !/^([a-f0-9]{64})$/.test(v.plan.fingerprint) ||
    v.plan.complete !== true ||
    !object(v.capabilities) ||
    v.capabilities.mutations !== false ||
    !["events", "finance", "connections", "results"].every(
      (k) =>
        typeof (v.capabilities as Record<string, unknown>)[k] === "boolean",
    ) ||
    !list(
      v.tasks,
      5000,
      (x) =>
        object(x) &&
        id(x.id) &&
        text(x.title, 300) &&
        member(x.kind, ["group", "leaf"]) &&
        (x.parent === null || id(x.parent)) &&
        list(x.dependencies, 100, id) &&
        member(x.status, Object.keys(statusLabels)) &&
        nullable(x.model, 100) &&
        nullable(x.effort, 40) &&
        member(x.mode, [null, "api", "cli"]) &&
        strings(x.scope) &&
        strings(x.criteria) &&
        strings(x.blockers),
    ) ||
    !list(
      v.attempts,
      200,
      (x) =>
        object(x) &&
        id(x.id) &&
        id(x.task) &&
        integer(x.epoch) &&
        text(x.worker) &&
        text(x.provider) &&
        id(x.connection) &&
        member(x.state, ["running", "terminated", "unknown", "fenced"]) &&
        time(x.started) &&
        (x.ended === null || time(x.ended)) &&
        text(x.phase) &&
        list(
          x.steps,
          50,
          (s) =>
            object(s) &&
            text(s.label) &&
            time(s.at) &&
            typeof s.complete === "boolean",
        ) &&
        strings(x.logs, 100),
    ) ||
    !list(
      v.connections,
      100,
      (x) =>
        object(x) &&
        id(x.id) &&
        member(x.provider, ["github", "openai", "anthropic", "aws"]) &&
        text(x.label) &&
        text(x.account) &&
        member(x.mode, ["api", "cli", "repository", "infrastructure"]) &&
        member(x.status, [
          "fixture_verified",
          "unverified",
          "revoked",
          "unsupported",
        ]) &&
        text(x.scope) &&
        integer(x.version) &&
        text(x.reason),
    ) ||
    !list(
      v.results,
      200,
      (x) =>
        object(x) &&
        id(x.id) &&
        id(x.task) &&
        id(x.attempt) &&
        typeof x.accepted === "boolean" &&
        member(x.verification, ["passed", "pending", "failed"]) &&
        strings(x.tests) &&
        text(x.sha256, 64) &&
        /^[a-f0-9]{64}$/.test(x.sha256) &&
        nullable(x.ref) &&
        typeof x.published === "boolean" &&
        time(x.at),
    ) ||
    !list(
      v.activity,
      200,
      (x) =>
        object(x) &&
        id(x.id) &&
        (x.task === null || id(x.task)) &&
        time(x.at) &&
        member(x.kind, [
          "execution",
          "verification",
          "financial",
          "security",
          "publication",
        ]) &&
        text(x.title) &&
        text(x.detail),
    )
  )
    throw new ReadError("invalid");
  const tasks = v.tasks as Snapshot["tasks"];
  const ids = new Set(tasks.map((x) => x.id));
  if (
    ids.size !== tasks.length ||
    tasks.some(
      (x) =>
        (x.parent !== null && !ids.has(x.parent)) ||
        x.dependencies.some((d) => !ids.has(d)),
    )
  )
    throw new ReadError("invalid");
  const attempts = v.attempts as Snapshot["attempts"],
    connections = v.connections as Snapshot["connections"],
    results = v.results as Snapshot["results"];
  if (
    new Set(attempts.map((a) => a.id)).size !== attempts.length ||
    new Set(connections.map((c) => c.id)).size !== connections.length ||
    new Set(results.map((r) => r.id)).size !== results.length ||
    connections.some(
      (c) => c.scope !== project || c.status === "fixture_verified",
    ) ||
    attempts.some(
      (a) =>
        !ids.has(a.task) || !connections.some((c) => c.id === a.connection),
    ) ||
    results.some(
      (r) =>
        !ids.has(r.task) ||
        !attempts.some((a) => a.id === r.attempt && a.task === r.task) ||
        (r.accepted && r.verification !== "passed") ||
        (r.published && (!r.accepted || !r.ref)),
    ) ||
    (v.activity as Snapshot["activity"]).some(
      (e) => e.task !== null && !ids.has(e.task),
    ) ||
    (!v.capabilities.connections && connections.length > 0) ||
    (!v.capabilities.results && results.length > 0) ||
    (!v.capabilities.finance && v.finance !== null)
  )
    throw new ReadError("invalid");
  // Reject cyclic/overdeep hierarchy and dependencies instead of rendering a forged topology.
  const byId = new Map(tasks.map((t) => [t.id, t]));
  if (
    tasks.some(
      (t) =>
        (t.parent !== null && byId.get(t.parent)?.kind !== "group") ||
        new Set(t.dependencies).size !== t.dependencies.length,
    )
  )
    throw new ReadError("invalid");
  for (const kind of ["parent", "dependencies"] as const) {
    const depths = new Map<string, number>();
    const depth = (key: string, level = 0): number => {
      if (level > 64) throw new ReadError("invalid");
      const known = depths.get(key);
      if (known !== undefined) {
        if (known < 0) throw new ReadError("invalid");
        return known;
      }
      depths.set(key, -1);
      const task = byId.get(key);
      if (!task) throw new ReadError("invalid");
      const next =
        kind === "parent"
          ? task.parent
            ? [task.parent]
            : []
          : task.dependencies;
      const result = 1 + Math.max(0, ...next.map((k) => depth(k, level + 1)));
      if (result > 64) throw new ReadError("invalid");
      depths.set(key, result);
      return result;
    };
    tasks.forEach((t) => depth(t.id));
  }
  if (v.finance !== null) {
    const f = v.finance;
    if (
      v.role !== "owner" ||
      !object(f) ||
      !text(f.currency, 3) ||
      !/^[A-Z]{3}$/.test(f.currency) ||
      !integer(f.version) ||
      !["ceiling", "reserved", "settled", "provisional"].every((k) =>
        money(f[k]),
      ) ||
      !(f.available === null || money(f.available)) ||
      !integer(f.unknownLiabilities) ||
      typeof f.paused !== "boolean" ||
      !object(f.forecasts) ||
      !["optimistic", "expected", "pessimistic"].every((k) =>
        money((f.forecasts as Record<string, unknown>)[k]),
      ) ||
      !list(
        f.categories,
        4,
        (x) =>
          object(x) &&
          member(x.id, ["ai", "compute", "storage", "transfer"]) &&
          (x.settled === null || money(x.settled)) &&
          money(x.reserved) &&
          money(x.forecast),
      ) ||
      f.categories.length !== 4 ||
      new Set(f.categories.map((x) => (x as { id: string }).id)).size !== 4 ||
      !list(
        f.approvals,
        100,
        (x) =>
          object(x) &&
          id(x.id) &&
          text(x.actor) &&
          money(x.previous) &&
          money(x.total) &&
          integer(x.version) &&
          time(x.at),
      ) ||
      (Number(f.unknownLiabilities) > 0 && f.available !== null)
    )
      throw new ReadError("invalid");
  }
  // Render only typed display values. Unknown server fields cannot become credentials or commands.
  return v as unknown as Snapshot;
}
async function read(path: string, signal: AbortSignal): Promise<unknown> {
  signal.throwIfAborted();
  const request = new AbortController();
  const cancel = () => request.abort(signal.reason);
  signal.addEventListener("abort", cancel, { once: true });
  const timer = setTimeout(() => request.abort(), 10000);
  try {
    return await readBounded(path, request.signal);
  } catch (error) {
    if (!signal.aborted && request.signal.aborted)
      throw new ReadError("network");
    throw error;
  } finally {
    clearTimeout(timer);
    signal.removeEventListener("abort", cancel);
  }
}
async function readBounded(
  path: string,
  signal: AbortSignal,
): Promise<unknown> {
  let response: Response;
  try {
    response = await fetch(path, {
      signal,
      credentials: "same-origin",
      cache: "no-store",
      redirect: "error",
      headers: { Accept: "application/json" },
    });
  } catch (e) {
    if (signal.aborted) throw e;
    throw new ReadError("network");
  }
  if (!response.ok)
    throw new ReadError(
      response.status === 401
        ? "unauthorized"
        : response.status === 403
          ? "forbidden"
          : "unavailable",
    );
  if (!response.headers.get("content-type")?.startsWith("application/json"))
    throw new ReadError("invalid");
  if (!response.body) throw new ReadError("invalid");
  const reader = response.body.getReader();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  let body = "",
    bytes = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > 1_000_000) throw new ReadError("invalid");
      body += decoder.decode(value, { stream: true });
    }
    body += decoder.decode();
    return JSON.parse(body) as unknown;
  } catch (e) {
    await reader.cancel().catch(() => undefined);
    if (e instanceof ReadError) throw e;
    throw new ReadError("invalid");
  } finally {
    reader.releaseLock();
  }
}
export const controllerSource: ReadSource = {
  async projects(signal) {
    return parseProjects(await read("/api/control/v1/projects", signal));
  },
  async snapshot(project, signal) {
    if (!isId(project)) throw new ReadError("invalid");
    return parseSnapshot(
      await read(
        `/api/control/v1/projects/${encodeURIComponent(project)}/snapshot`,
        signal,
      ),
      project,
    );
  },
};
