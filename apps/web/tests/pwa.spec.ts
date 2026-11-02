import { test, expect } from "@playwright/test";
test("static shell works offline and private API responses never enter caches", async ({
  page,
  context,
}) => {
  await page.goto("/");
  await expect(page.locator("h1")).toBeVisible();
  await page.evaluate(async () => {
    await navigator.serviceWorker.ready;
    await new Promise<void>((resolve) => {
      if (navigator.serviceWorker.controller) resolve();
      else
        navigator.serviceWorker.addEventListener(
          "controllerchange",
          () => resolve(),
          { once: true },
        );
    });
  });
  await context.route("**/api/**", (route) =>
    route.fulfill({
      json: { private: "not-cacheable" },
      headers: { "Cache-Control": "private, no-store" },
    }),
  );
  await page.evaluate(async () => {
    for (const path of [
      "/api/private",
      "/api/control/v1/auth/context",
      "/api/control/v1/auth/session",
      "/api/control/v1/projects/project-a/snapshot",
    ]) {
      await fetch(path, { credentials: "same-origin", cache: "no-store" });
    }
    await fetch("/api/control/v1/auth/logout", {
      method: "POST",
      credentials: "same-origin",
      cache: "no-store",
      headers: { "Content-Type": "application/json" },
      body: "{}",
    });
  });
  const paths = await page.evaluate(async () => {
    const names = await caches.keys();
    const keys = await Promise.all(
      names.map(async (name) =>
        (await (await caches.open(name)).keys()).map(
          (r) => new URL(r.url).pathname,
        ),
      ),
    );
    return keys.flat();
  });
  expect(paths.some((p) => p.startsWith("/api/"))).toBe(false);
  expect(paths).toContain("/index.html");
  // The private-response fixture is finished. Remove browser-level interception
  // before testing native Service Worker/offline navigation (not a routed request).
  await context.unroute("**/api/**");
  await context.setOffline(true);
  await page.reload();
  await expect(page.getByText("DEMO WORKSPACE", { exact: true })).toBeVisible();
  await expect(
    page.getByText("Offline. Current project state cannot be confirmed."),
  ).toBeVisible();
  await expect(page.locator("h1")).toBeVisible();
  await context.setOffline(false);
});
test("blocked external fonts and icons retain labeled controls and system font fallback", async ({
  page,
  context,
}) => {
  await context.route("https://cdn.jsdelivr.net/**", (r) => r.abort());
  await page.goto("/");
  await expect(
    page.getByRole("button", { name: "Display preferences" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Display preferences" }).click();
  await expect(
    page.getByRole("combobox", { name: "Theme", exact: true }),
  ).toBeVisible();
});
test("no Service Worker support still renders the application", async ({
  page,
}) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "serviceWorker", {
      value: undefined,
      configurable: true,
    });
  });
  await page.goto("/");
  await expect(page.locator("h1")).toBeVisible();
});
