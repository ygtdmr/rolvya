import { test, expect, privateInput } from "./human-fixture";
import AxeBuilder from "@axe-core/playwright";
// No auth traces/automatic screenshots: they retain passwords/cookies/CSRF.
// Genuine CSP and accessibility assertions remain active.
test.use({ ignoreHTTPSErrors: true, trace: "off", screenshot: "off" });
test("real Rust password, scoped plan, durable SSE and committed logout", async ({
  page,
  context,
  controller,
}) => {
  await page.addInitScript(() =>
    document.addEventListener("securitypolicyviolation", () =>
      Reflect.set(window, "cspViolation", true),
    ),
  );
  await controller.signIn(page);
  await expect(
    page.getByText("Bounded panel task", { exact: true }).first(),
  ).toBeVisible();
  await expect(page.locator(".rv-sidebar nav a")).toHaveCount(5);
  await expect(page.getByText("DEMO WORKSPACE", { exact: true })).toHaveCount(
    0,
  );
  expect(
    (await context.cookies())
      .filter((c) => c.name === "__Host-rolvya")
      .map((c) => ({
        secure: c.secure,
        httpOnly: c.httpOnly,
        sameSite: c.sameSite,
        path: c.path,
      })),
  ).toEqual([{ secure: true, httpOnly: true, sameSite: "Strict", path: "/" }]);
  expect(await page.evaluate(() => document.cookie.includes("rolvya"))).toBe(
    false,
  );
  await controller.command("emit");
  await expect(
    page.getByText("Awaiting verification", { exact: true }).first(),
  ).toBeVisible();
  await page.getByRole("button", { name: "Session", exact: true }).click();
  await page.getByRole("menuitem", { name: "Sign out", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Sign in to Rolvya" }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      async () =>
        (await fetch("/api/control/v1/projects", { cache: "no-store" })).status,
    ),
  ).toBe(401);
  await expect(
    page.getByText("Bounded panel task", { exact: true }),
  ).toHaveCount(0);
  expect(
    await page.evaluate(() => Reflect.get(window, "cspViolation")),
  ).toBeUndefined();
});
test("TOTP primary proof cannot read projects before enrolled factor verification", async ({
  page,
  controller,
}) => {
  await page.goto(controller.info.url);
  await page.getByLabel("Email address").fill(controller.info.email);
  await privateInput(
    page.getByLabel("Password", { exact: true }),
    controller.info.password,
  );
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByLabel("Authenticator code")).toBeVisible();
  expect(
    await page.evaluate(
      async () => (await fetch("/api/control/v1/projects")).status,
    ),
  ).toBe(401);
  await privateInput(
    page.getByLabel("Authenticator code"),
    controller.info.totp!,
  );
  await page.getByRole("button", { name: "Verify and sign in" }).click();
  await expect(
    page.getByText("Bounded panel task", { exact: true }).first(),
  ).toBeVisible();
});
test("real Viewer cannot read another project or authorize money", async ({
  page,
  controller,
}) => {
  await controller.signIn(page, true);
  expect(
    await page.evaluate(
      async () =>
        (await fetch("/api/control/v1/projects/project-b/snapshot")).status,
    ),
  ).toBe(403);
  await page
    .locator(".rv-sidebar")
    .getByRole("link", { name: /Costs/ })
    .click();
  await expect(
    page.getByText("Financial view unavailable", { exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      async () =>
        (
          await fetch("/api/control/v1/projects/project-a/approve", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: "{}",
          })
        ).status,
    ),
  ).not.toBe(200);
  await page.goto(controller.info.url + "/#/projects/project-b");
  await expect(
    page.getByRole("heading", { name: "Project access denied" }),
  ).toBeVisible();
  await expect(
    page.getByText("Bounded panel task", { exact: true }),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Choose an authorized project" })
    .click();
  await expect(
    page.getByRole("heading", { name: "project-a", exact: true }),
  ).toBeVisible();
});
test("open SSE revocation clears protected browser state", async ({
  page,
  controller,
}) => {
  await controller.signIn(page);
  await controller.command("revoke");
  await expect(
    page.getByRole("heading", { name: "Sign in to Rolvya" }),
  ).toBeVisible();
  await expect(
    page.getByText("Bounded panel task", { exact: true }),
  ).toHaveCount(0);
  expect(
    await page.evaluate(
      async () => (await fetch("/api/control/v1/projects")).status,
    ),
  ).toBe(401);
});
test("clean SSE rollover revalidates REST without another journal event", async ({
  page,
  controller,
}) => {
  let calls = 0,
    snapshots = 0;
  await page.route(
    "**/api/control/v1/projects/project-a/snapshot",
    async (r) => {
      snapshots++;
      await r.continue();
    },
  );
  await page.route("**/api/control/v1/projects/project-a/events", async (r) => {
    if (calls++ === 0)
      await r.fulfill({
        headers: {
          "Content-Type": "text/event-stream",
          "Cache-Control": "no-store",
        },
        body: ": keepalive\n\n",
      });
    else await r.continue();
  });
  await controller.signIn(page);
  await expect.poll(() => snapshots).toBeGreaterThan(1);
  await expect(page.locator("[data-ui=connectivity-banner]")).toContainText(
    "Snapshot current",
  );
});
test("login accessibility and mobile composition", async ({
  page,
  controller,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(controller.info.url);
  await expect(
    page.getByRole("heading", { name: "Sign in to Rolvya" }),
  ).toBeVisible();
  await page.getByLabel("Email address").focus();
  await page.keyboard.press("Tab");
  await expect(page.getByLabel("Password", { exact: true })).toBeFocused();
  expect(
    (
      await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
        .analyze()
    ).violations,
  ).toEqual([]);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  for (const theme of ["Light", "Dark"]) {
    await page.getByRole("combobox", { name: "Theme", exact: true }).click();
    await page.getByRole("option", { name: theme, exact: true }).click();
    expect(
      (
        await new AxeBuilder({ page })
          .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
          .analyze()
      ).violations,
    ).toEqual([]);
    await page.screenshot({
      path: `.artifacts/login-mobile-${theme.toLowerCase()}.png`,
    });
  }
  await page.setViewportSize({ width: 320, height: 640 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});
test("HTTP cannot establish trust or fall back to demo", async ({ page }) => {
  await page.goto("http://127.0.0.1:4173/");
  await expect(
    page
      .getByText("A trusted HTTPS connection is required to sign in.", {
        exact: true,
      })
      .first(),
  ).toBeVisible();
  await expect(page.getByText("DEMO WORKSPACE", { exact: true })).toHaveCount(
    0,
  );
});
test("keyboard skip link preserves route and task scope", async ({ page }) => {
  await page.goto("/#/execution/prj_atlas?task=A");
  await expect(page.locator("h1")).toBeVisible();
  await page.getByRole("link", { name: "Skip to main content" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#main")).toBeFocused();
  await expect(page).toHaveURL(/execution\/prj_atlas\?task=A/);
});
test("real renewal and cross-tab logout share authority without sharing credentials", async ({
  page,
  context,
  controller,
}) => {
  await controller.signIn(page);
  const sibling = await context.newPage();
  await sibling.goto(controller.info.url + "/#/projects/project-a");
  await expect(
    sibling.getByText("Bounded panel task", { exact: true }).first(),
  ).toBeVisible();
  await page.getByRole("button", { name: "Session", exact: true }).click();
  await page
    .getByRole("menuitem", { name: "Renew session", exact: true })
    .click();
  await expect(
    page.getByText("Bounded panel task", { exact: true }).first(),
  ).toBeVisible();
  expect(
    await page.evaluate(async () =>
      Number(
        (await (await fetch("/api/control/v1/auth/session")).json()).generation,
      ),
    ),
  ).toBe(2);
  // Keep the sibling alive across at least two real server SSE revocation polls;
  // an immediate DOM assertion misses a stale-generation stream disconnect.
  await sibling.waitForTimeout(2500);
  await expect(
    sibling.getByText("Bounded panel task", { exact: true }).first(),
  ).toBeVisible();
  await page.getByRole("button", { name: "Session", exact: true }).click();
  await page.getByRole("menuitem", { name: "Sign out", exact: true }).click();
  await expect(
    sibling.getByRole("heading", { name: "Sign in to Rolvya" }),
  ).toBeVisible();
  await sibling.close();
});
test("verified private email OTP uses the actual challenge and browser confirmation UI", async ({
  page,
  controller,
}) => {
  await page.goto(controller.info.url);
  await page.getByRole("button", { name: "Other sign-in methods" }).click();
  await page.getByLabel("Email address").fill(controller.info.email);
  await page
    .getByRole("combobox", { name: "Email method", exact: true })
    .click();
  await page
    .getByRole("option", { name: "One-time email code", exact: true })
    .click();
  const delivered = page.waitForResponse(
    (r) => r.url().endsWith("/auth/email") && r.request().method() === "POST",
  );
  await page
    .getByRole("button", { name: "Continue with email", exact: true })
    .click();
  const reference = String((await (await delivered).json()).challenge);
  await privateInput(
    page.getByLabel("Email code"),
    await controller.deliveredMessage(reference),
  );
  await page.getByRole("button", { name: "Verify and sign in" }).click();
  await expect(
    page.getByText("Bounded panel task", { exact: true }).first(),
  ).toBeVisible();
});
test("Magic Link opening cannot authenticate until explicit same-browser confirmation", async ({
  page,
  controller,
}) => {
  await page.goto(controller.info.url);
  await page.getByRole("button", { name: "Other sign-in methods" }).click();
  await page.getByLabel("Email address").fill(controller.info.email);
  const delivered = page.waitForResponse(
    (r) => r.url().endsWith("/auth/email") && r.request().method() === "POST",
  );
  await page
    .getByRole("button", { name: "Continue with email", exact: true })
    .click();
  const reference = String((await (await delivered).json()).challenge);
  const token = await controller.deliveredMessage(reference);
  // No trace or URL-bearing diagnostics are retained for this private fixture.
  try {
    await page.goto(
      controller.info.url + "/#email-confirm=" + reference + "." + token,
      { timeout: 5000 },
    );
  } catch {
    throw new Error("Private confirmation page unavailable.");
  }
  await expect(
    page.getByRole("heading", { name: "Confirm your sign-in" }),
  ).toBeVisible();
  expect(await page.evaluate(() => location.hash)).toBe("");
  expect(
    await page.evaluate(
      async () => (await fetch("/api/control/v1/projects")).status,
    ),
  ).toBe(401);
  await page
    .getByRole("button", { name: "Confirm sign-in", exact: true })
    .click();
  await expect(
    page.getByText("Bounded panel task", { exact: true }).first(),
  ).toBeVisible();
});
test("untrusted repository text is never executable browser markup", async ({
  page,
  controller,
}) => {
  const text = '<img src=x onerror="window.pwned=true">';
  await controller.signIn(page);
  // Obtain the authorized redacted data through the real browser before this
  // display-only adversarial fixture substitutes repository text. Authentication,
  // membership, cookies and capabilities are not supplied by this fixture.
  const snapshot = await page.evaluate(async () => {
    const response = await fetch(
      "/api/control/v1/projects/project-a/snapshot",
      { cache: "no-store" },
    );
    if (!response.ok)
      throw new Error("Authorized fixture snapshot unavailable");
    return await response.json();
  });
  snapshot.tasks[0].title = text;
  await page.route("**/api/control/v1/projects/project-a/snapshot", (route) =>
    route.fulfill({ json: snapshot, headers: { "Cache-Control": "no-store" } }),
  );
  await page
    .locator("[data-ui=connectivity-banner]")
    .getByRole("button", { name: "Refresh", exact: true })
    .click();
  await expect(page.getByText(text, { exact: true }).last()).toBeVisible();
  expect(
    await page.evaluate(() => Reflect.get(window, "pwned")),
  ).toBeUndefined();
});
test("explicit SSE project denial clears private state without demo fallback", async ({
  page,
  controller,
}) => {
  await page.route("**/api/control/v1/projects/project-a/events", (route) =>
    route.fulfill({
      status: 403,
      json: { schema: 1, error: "project_access_denied" },
    }),
  );
  await controller.signIn(page, false, false);
  await expect(
    page.getByRole("heading", { name: "Project access denied" }),
  ).toBeVisible();
  await expect(
    page.getByText("Bounded panel task", { exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText("DEMO WORKSPACE", { exact: true })).toHaveCount(
    0,
  );
});
