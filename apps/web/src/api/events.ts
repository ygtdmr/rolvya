import { isId } from "./model";
import { ReadError } from "./client";
export interface Invalidation {
  schema: 1 | 2;
  project: string;
  cursor: number;
  revision: number;
}
/** SSE is invalidation only; current snapshots, never event payload claims, provide display data. */
export class EventOrder {
  constructor(
    private project: string,
    private cursor: number,
    private revision: number,
  ) {}
  accept(raw: string, id: string): "duplicate" | "refresh" | "gap" {
    if (raw.length > 4096) throw Error("event limit");
    const e = JSON.parse(raw) as Partial<Invalidation>;
    if (
      ![1, 2].includes(Number(e.schema)) ||
      e.project !== this.project ||
      !Number.isSafeInteger(e.cursor) ||
      !Number.isSafeInteger(e.revision) ||
      (e.schema === 1
        ? String(e.cursor) !== id
        : !new RegExp(`^${e.cursor}\\.[a-f0-9]{64}$`).test(id)) ||
      !isId(e.project) ||
      Number(e.cursor) < 0 ||
      Number(e.revision) < 0
    )
      throw Error("invalid event");
    if (Number(e.cursor) <= this.cursor) return "duplicate";
    const gap =
      Number(e.cursor) !== this.cursor + 1 ||
      Number(e.revision) < this.revision;
    this.cursor = Number(e.cursor);
    this.revision = Number(e.revision);
    return gap ? "gap" : "refresh";
  }
}
/** Bounded UTF-8 SSE frames; comments are keepalives, not human activity. */
export async function consumeEvents(
  body: ReadableStream<Uint8Array>,
  onEvent: (data: string, id: string) => void,
  signal: AbortSignal,
): Promise<void> {
  const reader = body.getReader(),
    decoder = new TextDecoder("utf-8", { fatal: true });
  let buffer = "",
    data = "",
    id = "",
    event = "",
    frame = 0;
  const abort = () => {
    void reader.cancel();
  };
  signal.addEventListener("abort", abort, { once: true });
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      buffer += decoder.decode(next.value, { stream: true });
      if (buffer.length > 16384) throw Error("event frame limit");
      let index: number;
      while ((index = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, index).replace(/\r$/, "");
        buffer = buffer.slice(index + 1);
        frame += line.length;
        if (frame > 8192) throw Error("event frame limit");
        if (line === "") {
          if (data) {
            if (["unauthorized", "forbidden", "reset"].includes(event)) {
              const control = JSON.parse(data) as Record<string, unknown>;
              const expected = event === "reset" ? "replay_gap" : event;
              if (
                control.schema !== 1 ||
                control.error !== expected ||
                Object.keys(control).length !== 2
              )
                throw new ReadError("invalid");
              throw new ReadError(
                event === "unauthorized"
                  ? "unauthorized"
                  : event === "forbidden"
                    ? "forbidden"
                    : "invalid",
              );
            }
            if (event && event !== "message") throw new ReadError("invalid");
            onEvent(data.slice(0, -1), id);
          }
          data = "";
          id = "";
          event = "";
          frame = 0;
        } else if (line.startsWith("data:"))
          data += line.slice(5).replace(/^ /, "") + "\n";
        else if (line.startsWith("id:")) id = line.slice(3).trim();
        else if (line.startsWith("event:")) event = line.slice(6).trim();
      }
    }
    decoder.decode();
    if (buffer || data) throw Error("truncated event");
  } finally {
    signal.removeEventListener("abort", abort);
    await reader.cancel().catch(() => undefined);
    reader.releaseLock();
  }
}
export function reconnectDelay(attempt: number): number {
  return Math.min(30000, 1000 * 2 ** Math.min(5, Math.max(0, attempt)));
}
export async function streamProject(
  project: string,
  cursor: number | string,
  signal: AbortSignal,
  onEvent: (data: string, id: string) => void,
): Promise<void> {
  if (
    !isId(project) ||
    (typeof cursor === "number"
      ? !Number.isSafeInteger(cursor) || cursor < 0
      : !/^[0-9]{1,16}\.[a-f0-9]{64}$/.test(cursor))
  )
    throw Error("invalid stream scope");
  const response = await fetch(`/api/control/v1/projects/${project}/events`, {
    signal,
    credentials: "same-origin",
    cache: "no-store",
    redirect: "error",
    headers: { Accept: "text/event-stream", "Last-Event-ID": String(cursor) },
  });
  if (
    !response.ok ||
    !response.headers.get("content-type")?.startsWith("text/event-stream") ||
    !response.body
  )
    throw new ReadError(
      response.status === 401
        ? "unauthorized"
        : response.status === 403
          ? "forbidden"
          : response.status === 409
            ? "invalid"
            : "unavailable",
    );
  await consumeEvents(response.body, onEvent, signal);
}
/** Coalesce update bursts; never turn an event into a state mutation or authority. */
export function coalesceInvalidations(refresh: () => void) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    request: () => {
      if (timer === undefined)
        timer = setTimeout(() => {
          timer = undefined;
          refresh();
        }, 500);
    },
    cancel: () => {
      clearTimeout(timer);
      timer = undefined;
    },
  };
}
