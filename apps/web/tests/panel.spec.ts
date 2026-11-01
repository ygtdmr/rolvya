import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { ready, chooseOption } from "./support";
test("independent synthetic verification unblocks existing dependencies without changing ceiling", async ({
  page,
}) => {
  await ready(page);
  await page.getByRole("button", { name: "Advance scenario" }).click();
  await expect(
    page.getByText("Awaiting verification", { exact: true }).count(),
  ).resolves.toBeGreaterThan(0);
  await page.getByRole("button", { name: "Advance scenario" }).click();
  await expect(
    page.getByRole("button", { name: "Advance scenario" }),
  ).toBeDisabled();
  await page.getByRole("tab", { name: "Task hierarchy" }).click();
  await expect(
    page
      .locator(".rv-task-row")
      .filter({ hasText: "Verify the integrated contract" })
      .getByText("Ready", { exact: true }),
  ).toBeVisible();
});
test("production has no demo fallback, gallery or credential UI", async ({
  page,
}) => {
  await page.goto("http://127.0.0.1:4173/");
  await expect(
    page.getByRole("heading", { name: "Browser integration is not activated" }),
  ).toBeVisible();
  await expect(page.getByText("DEMO WORKSPACE", { exact: true })).toHaveCount(
    0,
  );
  await expect(
    page.getByRole("button", { name: "Advance scenario" }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Display preferences" }).click();
  await expect(
    page.getByRole("button", { name: "UI Elements gallery" }),
  ).toHaveCount(0);
});
for (const width of [320, 390, 768, 1024, 1440, 2560])
  test(`responsive ${width}px without page overflow`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await ready(page);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: `.artifacts/screenshots/projects-${width}.png`,
      fullPage: true,
    });
    if (width < 768) {
      await page.getByRole("button", { name: "Open navigation" }).click();
      await page
        .getByRole("dialog", { name: "Navigation" })
        .getByRole("link", { name: /Connections/ })
        .click();
      await expect(
        page.getByRole("heading", {
          name: "Connections, never ambient access.",
        }),
      ).toBeVisible();
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
    }
  });
test("keyboard, overlays, themes, RTL, long labels and reduced motion", async ({
  page,
}) => {
  await ready(page);
  await page.getByRole("button", { name: "Display preferences" }).click();
  await chooseOption(page, "Theme", "Light");
  await chooseOption(page, "Locale", "QA · RTL labels");
  await page.getByRole("switch", { name: "Reduce motion" }).check();
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: ".artifacts/screenshots/rtl-light.png",
    fullPage: true,
  });
  await page.locator(".rv-task-row").first().focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.locator(".rv-task-row").first()).toBeFocused();
});
test("all sections pass automated WCAG AA checks in dark and light", async ({
  page,
}) => {
  for (const theme of ["dark", "light"]) {
    await ready(page);
    await page.getByRole("button", { name: "Display preferences" }).click();
    await chooseOption(page, "Theme", theme === "dark" ? "Dark" : "Light");
    await page.getByRole("button", { name: "Done", exact: true }).click();
    for (const section of [
      "projects",
      "execution",
      "costs-approvals",
      "connections",
      "results-activity",
    ]) {
      await page.goto(`/#/${section}/prj_atlas`);
      await expect(page.locator("h1")).toBeVisible();
      await page.waitForTimeout(150);
      const results = await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
        .analyze();
      expect(
        results.violations.map((x) => ({
          id: x.id,
          nodes: x.nodes.map((n) => n.target),
        })),
      ).toEqual([]);
    }
  }
});
test("touch landscape, forced contrast, motion and 200/400-percent reflow equivalents", async ({
  browser,
}) => {
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
    reducedMotion: "reduce",
    forcedColors: "active",
  });
  const page = await context.newPage();
  await page.goto("http://127.0.0.1:4174/");
  await expect(page.locator("h1")).toBeVisible();
  await page
    .getByRole("combobox", { name: "Project workspace", exact: true })
    .tap();
  await expect(
    page.getByRole("combobox", { name: "Project workspace", exact: true }),
  ).toBeFocused();
  await page
    .getByRole("option", { name: "Relay / event gateway", exact: true })
    .tap();
  await expect(
    page.getByRole("heading", { name: "Relay / event gateway", exact: true }),
  ).toBeVisible();
  await page.getByLabel("Search tasks", { exact: true }).fill("response");
  await expect(page.locator(".rv-task-row")).toHaveCount(1);
  await page.setViewportSize({ width: 844, height: 390 });
  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    )
    .toBe(true);
  for (const width of [640, 320]) {
    await page.setViewportSize({ width, height: 700 });
    await expect
      .poll(() =>
        page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      )
      .toBe(true);
  }
  await context.close();
});
test("UI gallery exposes shared states and accessible dialog behavior", async ({
  page,
}) => {
  await ready(page);
  await page.getByRole("button", { name: "Display preferences" }).click();
  await page.getByRole("button", { name: "UI Elements gallery" }).click();
  const gallery = page.getByRole("dialog", {
    name: "UI Elements · development only",
  });
  await expect(gallery).toBeVisible();
  await expect(
    gallery.getByRole("button", { name: "Disabled", exact: true }),
  ).toBeDisabled();
  await expect(
    gallery.getByRole("textbox", { name: "Invalid field" }),
  ).toHaveAccessibleDescription("Example error");
  await page.keyboard.press("Escape");
  await expect(gallery).toHaveCount(0);
});
for (const section of [
  "projects",
  "execution",
  "costs-approvals",
  "connections",
  "results-activity",
])
  test(`all-section phone reflow: ${section}`, async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 700 });
    await page.goto(`/#/${section}/prj_atlas`);
    await expect(page.locator("h1")).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  });
test("combobox keyboard and open-listbox accessibility preserve nested focus", async ({
  page,
}) => {
  await ready(page);
  await page.getByRole("button", { name: "Display preferences" }).click();
  const theme = page.getByRole("combobox", { name: "Theme", exact: true });
  await theme.focus();
  await page.keyboard.press("ArrowDown");
  await expect(
    page.getByRole("listbox", { name: "Theme", exact: true }),
  ).toBeVisible();
  await expect(theme).toBeFocused();
  const audit = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
    .analyze();
  expect(
    audit.violations.map((v) => ({
      id: v.id,
      nodes: v.nodes.map((n) => n.target),
    })),
  ).toEqual([]);
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("listbox", { name: "Theme", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("dialog", { name: "Display preferences" }),
  ).toBeVisible();
  await expect(theme).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("dialog", { name: "Display preferences" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Display preferences" }),
  ).toBeFocused();
});
