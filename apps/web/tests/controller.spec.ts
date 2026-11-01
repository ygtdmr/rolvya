import { test, expect } from "@playwright/test";
import contract from "../src/testing/controller-snapshot.json" with { type: "json" };
for (const status of [401, 403])
  test(`production HTTP ${status} exposes no private state or demo fallback`, async ({
    page,
  }) => {
    await page.route("**/api/control/v1/projects", (r) =>
      r.fulfill({ status, json: { error: "denied" } }),
    );
    await page.goto("http://127.0.0.1:4173/");
    await expect(
      page.getByRole("heading", {
        name:
          status === 401
            ? "Human authentication required"
            : "Project access denied",
      }),
    ).toBeVisible();
    await expect(page.getByText("DEMO WORKSPACE", { exact: true })).toHaveCount(
      0,
    );
  });
test("real Rust projection contract renders read-only and SSE revocation clears it", async ({
  page,
}) => {
  const snapshot = structuredClone(contract);
  snapshot.capabilities.events = true;
  await page.route("**/api/control/v1/projects", (r) =>
    r.fulfill({
      json: [
        {
          id: "project-a",
          name: "project-a",
          detail: "Synthetic protocol fixture",
        },
      ],
    }),
  );
  await page.route("**/api/control/v1/projects/project-a/snapshot", (r) =>
    r.fulfill({ json: snapshot }),
  );
  let eventCalls = 0;
  await page.route("**/api/control/v1/projects/project-a/events", (r) => {
    eventCalls++;
    return r.fulfill({ status: 403, body: "Forbidden" });
  });
  await page.goto("http://127.0.0.1:4173/#/projects/project-a");
  await expect(
    page.getByRole("heading", { name: "Project access denied" }),
  ).toBeVisible();
  await expect(
    page.getByText("Bounded panel task", { exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText("DEMO WORKSPACE", { exact: true })).toHaveCount(
    0,
  );
  expect(eventCalls).toBe(1);
});
test("untrusted repository text remains text and cannot execute markup", async ({
  page,
}) => {
  const snapshot = structuredClone(contract);
  snapshot.tasks[0]!.title = '<img src=x onerror="window.pwned=true">';
  await page.route("**/api/control/v1/projects", (r) =>
    r.fulfill({
      json: [{ id: "project-a", name: "project-a", detail: "Fixture" }],
    }),
  );
  await page.route("**/api/control/v1/projects/project-a/snapshot", (r) =>
    r.fulfill({ json: snapshot }),
  );
  await page.goto("http://127.0.0.1:4173/#/projects/project-a");
  await expect(
    page.getByText(snapshot.tasks[0]!.title, { exact: true }).last(),
  ).toBeVisible();
  expect(
    await page.evaluate(() => Reflect.get(window, "pwned")),
  ).toBeUndefined();
});
test("keyboard skip link preserves route and task scope", async ({ page }) => {
  await page.goto("/#/execution/prj_atlas?task=A");
  await expect(page.locator("h1")).toBeVisible();
  await page.getByRole("link", { name: "Skip to main content" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#main")).toBeFocused();
  await expect(page).toHaveURL(/execution\/prj_atlas\?task=A/);
});
