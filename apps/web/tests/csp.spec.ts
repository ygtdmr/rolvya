import { test, expect } from "@playwright/test";
import { ready } from "./support";
// Both initial rendering and navigation must preserve the strict browser policy.
test("five sections preserve task identity and exact financial boundaries", async ({
  page,
}) => {
  const violations: string[] = [];
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (e) => {
      (window as unknown as { csp: unknown[] }).csp ??= [];
      (window as unknown as { csp: unknown[] }).csp.push({
        directive: e.violatedDirective,
        blocked: e.blockedURI,
        source: e.sourceFile,
        line: e.lineNumber,
        column: e.columnNumber,
        sample: e.sample,
      });
    });
  });
  page.on("pageerror", (e) => violations.push(e.message));
  await ready(page);
  expect(
    await page.evaluate(
      () => (window as unknown as { csp?: unknown[] }).csp ?? [],
    ),
  ).toEqual([]);
  await expect(page.locator(".rv-sidebar nav a")).toHaveCount(5);
  await expect(page.getByText("DEMO WORKSPACE", { exact: true })).toBeVisible();
  await page.locator("[data-ui=dependency-graph] [data-task=A]").click();
  await expect(
    page.getByRole("dialog", { name: "Task inspector" }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("dialog")
      .getByText("Harden request validation", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Execution", exact: true })
    .click();
  await expect(page).toHaveURL(/execution.*task=A/);
  await expect(
    page.getByText("attempt_A1", { exact: false }).first(),
  ).toBeVisible();
  await page
    .locator(".rv-sidebar")
    .getByRole("link", { name: /Costs/ })
    .click();
  await expect(
    page.getByText("Unknown", { exact: true }).first(),
  ).toBeVisible();
  await page.getByRole("button", { name: "Preview approval" }).click();
  await page
    .getByLabel("New total ceiling · integer minor units")
    .fill("999999");
  await expect(
    page.getByRole("button", {
      name: "Prepare exact approval",
    }),
  ).toBeDisabled();
  await expect(page.getByRole("checkbox")).toBeDisabled();
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await expect(
    page.getByText("$240.00", { exact: true }).first(),
  ).toBeVisible();
  await page
    .locator(".rv-sidebar")
    .getByRole("link", { name: "Connections 04" })
    .click();
  await expect(page.getByRole("heading", { name: "Codex CLI" })).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Claude Code CLI" }),
  ).toBeVisible();
  await page
    .locator(".rv-sidebar")
    .getByRole("link", { name: /Results & Activity/ })
    .click();
  await expect(
    page
      .locator("[data-ui=badge]")
      .filter({ hasText: "Independently accepted" })
      .first(),
  ).toBeVisible();
  await expect(page).toHaveURL(/task=A/);
  await page.goBack();
  await expect(page).toHaveURL(/connections/);
  expect(violations).toEqual([]);
  expect(
    await page.evaluate(
      () => (window as unknown as { csp?: unknown[] }).csp ?? [],
    ),
  ).toEqual([]);
});
