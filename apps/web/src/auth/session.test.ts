import { afterEach, expect, it, vi } from "vitest";
import { BrowserSession } from "./session";
const clients: BrowserSession[] = [];
function client() {
  vi.stubGlobal("BroadcastChannel", undefined);
  const c = new BrowserSession();
  clients.push(c);
  return c;
}
const response = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
const session = () => ({
  schema: 1,
  state: "authenticated",
  user: "a".repeat(32),
  session: "b".repeat(32),
  generation: 1,
  assurance: "primary",
  csrf: "c".repeat(64),
  absoluteExpires: 9000000,
  lastHumanAt: 1000000,
  serverNow: 1000000,
  tokenExpiresAt: 1086400,
});
const context = () => ({
  schema: 1,
  state: "unauthenticated",
  password: true,
  email: false,
  csrf: "d".repeat(64),
});
it("uses only configured HTTPS support destinations without credentials", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(response({}, 401))
    .mockResolvedValueOnce(
      response({ ...context(), support: "https://support.example.test/help" }),
    );
  vi.stubGlobal("fetch", fetch);
  const c = client();
  await c.start();
  expect(c.getSnapshot().supportUrl).toBe("https://support.example.test/help");
});
afterEach(() => {
  clients.forEach((c) => c.dispose());
  clients.length = 0;
  vi.unstubAllGlobals();
});
it("synthetic DOM input and background inspection cannot record human activity", async () => {
  const fetch = vi.fn().mockResolvedValue(response(session()));
  vi.stubGlobal("fetch", fetch);
  const c = client();
  await c.start();
  document.dispatchEvent(new Event("pointerdown"));
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
  await Promise.resolve();
  expect(fetch).toHaveBeenCalledTimes(1);
});
it("confirmed server logout remains confirmed if the next login-context fetch fails", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(response(session()))
    .mockResolvedValueOnce(response({ schema: 1, state: "signed_out" }))
    .mockRejectedValueOnce(new TypeError("offline"));
  vi.stubGlobal("fetch", fetch);
  const c = client();
  await c.start();
  await c.logout();
  expect(c.getSnapshot().phase).toBe("unauthenticated");
  expect(c.getSnapshot().message).toContain("Signed out");
  expect(c.getSnapshot().session).toBeNull();
  expect(fetch).toHaveBeenCalledTimes(3);
});
it("unconfirmed logout clears private presentation without claiming server revocation", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(response(session()))
    .mockRejectedValueOnce(new TypeError("offline"));
  vi.stubGlobal("fetch", fetch);
  const c = client();
  await c.start();
  await c.logout();
  expect(c.getSnapshot().phase).toBe("unavailable");
  expect(c.getSnapshot().message).toContain("not confirmed");
  expect(c.getSnapshot().session).toBeNull();
  expect(fetch).toHaveBeenCalledTimes(2);
});
it("primary factor requirement grants no authenticated state and exposes no cookie", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(response({}, 401))
    .mockResolvedValueOnce(response(context()))
    .mockResolvedValueOnce(response({ schema: 1, state: "factor_required" }));
  vi.stubGlobal("fetch", fetch);
  const c = client();
  await c.start();
  await c.password("person@example.invalid", "offline-ui-fixture-password");
  expect(c.getSnapshot().phase).toBe("factor_required");
  expect(c.getSnapshot().stage).toBe("factor");
  expect(c.getSnapshot().session).toBeNull();
  expect(JSON.stringify(c.getSnapshot())).not.toContain("d".repeat(64));
  const args = fetch.mock.calls.at(-1)!;
  expect(args[1].headers["X-Rolvya-CSRF"]).toBe("d".repeat(64));
  expect(args[1].credentials).toBe("same-origin");
  expect(args[1].redirect).toBe("error");
});
it("lost renewal response cannot retry the billable-free session mutation or restore stale state", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(response(session()))
    .mockRejectedValueOnce(new TypeError("lost response"))
    .mockResolvedValueOnce(response(context()));
  vi.stubGlobal("fetch", fetch);
  const c = client();
  await c.start();
  await c.renew();
  expect(c.getSnapshot().phase).toBe("expired");
  expect(c.getSnapshot().session).toBeNull();
  expect(
    fetch.mock.calls.filter((x) => String(x[0]).endsWith("/renew")),
  ).toHaveLength(1);
});
it("API session metadata cannot expose CSRF or opaque handles through public UI state", async () => {
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValue(response({ ...session(), handle: "e".repeat(64) })),
  );
  const c = client();
  await c.start();
  expect(c.getSnapshot().phase).toBe("authenticated");
  expect(JSON.stringify(c.getSnapshot())).not.toContain("c".repeat(64));
  expect(JSON.stringify(c.getSnapshot())).not.toContain("e".repeat(64));
  expect(localStorage.length).toBe(0);
  expect(sessionStorage.length).toBe(0);
});
it("budget commands use only the fixed same-origin family and private session CSRF", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(response(session()))
    .mockResolvedValueOnce(
      response({
        schema: 1,
        state: "pending",
        action: "e".repeat(32),
        password: true,
        email: false,
        totp: false,
        proofExpiresAt: null,
        serverNow: 1000000,
      }),
    );
  vi.stubGlobal("fetch", fetch);
  const c = client();
  await c.start();
  const proposal = {
    expectedVersion: 1,
    currency: "USD",
    previous: "20000",
    total: "25000",
    confirm: true,
  } as const;
  await c.budget("project-a", { stage: "prepare", proposal });
  expect(fetch.mock.calls[1]?.[0]).toBe(
    "/api/control/v1/projects/project-a/budget",
  );
  expect(fetch.mock.calls[1]?.[1]).toMatchObject({
    method: "POST",
    credentials: "same-origin",
    cache: "no-store",
    redirect: "error",
    headers: { "X-Rolvya-CSRF": "c".repeat(64) },
  });
  expect(JSON.stringify(c.getSnapshot())).not.toContain("c".repeat(64));
  await expect(
    c.budget("../other", { stage: "prepare", proposal }),
  ).rejects.toThrow();
  expect(fetch).toHaveBeenCalledTimes(2);
});
it("a lost approval acknowledgement is never retried automatically", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(response(session()))
    .mockRejectedValueOnce(new TypeError("connection lost"));
  vi.stubGlobal("fetch", fetch);
  const c = client();
  await c.start();
  await expect(
    c.budget("project-a", {
      stage: "execute",
      action: "e".repeat(32),
      proposal: {
        expectedVersion: 1,
        currency: "USD",
        previous: "20000",
        total: "25000",
        confirm: true,
      },
    }),
  ).rejects.toThrow();
  expect(fetch).toHaveBeenCalledTimes(2);
});
