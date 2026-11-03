import { test, expect, privateInput } from "./human-fixture";
import AxeBuilder from "@axe-core/playwright";
// No auth traces/automatic screenshots: they retain passwords/cookies/CSRF.
// Genuine CSP and accessibility assertions remain active.
test.use({ ignoreHTTPSErrors: true, trace: "off", screenshot: "off" });
test("installation bootstrap uses independently evidenced enrollment and exact host-approved binding", async ({
  page,
  controller,
}) => {
  await controller.signIn(page, false, false);
  await expect(
    page.getByRole("button", { name: "Session", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText(
      /contact the instance administrator for a verified host-approved binding/,
    ),
  ).toBeVisible();
  const status = await page.evaluate(async (approval) => {
    const session = await (
      await fetch("/api/control/v1/auth/session", { cache: "no-store" })
    ).json();
    return (
      await fetch("/api/control/v1/auth/project-binding", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Rolvya-CSRF": session.csrf,
        },
        body: JSON.stringify({ approval, confirm: true }),
        redirect: "error",
        cache: "no-store",
      })
    ).status;
  }, controller.info.bindingApproval);
  expect(status).toBe(200);
  await page.reload();
  await expect(
    page.getByRole("heading", { name: controller.info.project, exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Synthetic bounded task", { exact: true }).first(),
  ).toBeVisible();
  await controller.command("emit");
  await expect(
    page.getByText("Awaiting verification", { exact: true }).first(),
  ).toBeVisible();
  await page.getByRole("button", { name: "Session", exact: true }).click();
  await page.getByRole("menuitem", { name: "Sign out", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Sign in to Rolvya" }),
  ).toBeVisible();
});
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
    // Text and its surface must retain AA contrast throughout a theme change,
    // including frames before an optional animation has finished.
    const contrast = await page.evaluate(async () => {
      const luminance = (color: string) => {
        const channels = color
          .match(/[\d.]+/g)!
          .slice(0, 3)
          .map(Number);
        const linear = channels.map((value) => {
          const channel = value / 255;
          return channel <= 0.04045
            ? channel / 12.92
            : ((channel + 0.055) / 1.055) ** 2.4;
        });
        return linear[0]! * 0.2126 + linear[1]! * 0.7152 + linear[2]! * 0.0722;
      };
      const samples: number[] = [];
      for (let frame = 0; frame < 12; frame++) {
        const button = document.querySelector<HTMLButtonElement>(
          '.rv-auth-form button[type="submit"]',
        )!;
        const style = getComputedStyle(button);
        const a = luminance(style.color),
          b = luminance(style.backgroundColor);
        samples.push((Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05));
        await new Promise(requestAnimationFrame);
      }
      return Math.min(...samples);
    });
    expect(contrast).toBeGreaterThanOrEqual(4.5);
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
  // Consume immediately on headers; navigation may otherwise evict the CDP body.
  const delivered = page
    .waitForResponse(
      (r) => r.url().endsWith("/auth/email") && r.request().method() === "POST",
    )
    .then((response) => response.json());
  await page
    .getByRole("button", { name: "Continue with email", exact: true })
    .click();
  const reference = String((await delivered).challenge);
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
  // Consume immediately on headers; navigation may otherwise evict the CDP body.
  const delivered = page
    .waitForResponse(
      (r) => r.url().endsWith("/auth/email") && r.request().method() === "POST",
    )
    .then((response) => response.json());
  await page
    .getByRole("button", { name: "Continue with email", exact: true })
    .click();
  const reference = String((await delivered).challenge);
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

// Page routing must own these adversarial responses. Service Worker fetches
// bypass Playwright routes in WebKit; real authentication and PWA tests above
// retain the production worker and exercise its actual cache restrictions.
test.describe("controlled network responses", () => {
  test.use({ serviceWorkers: "block" });
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
    await page.route(
      "**/api/control/v1/projects/project-a/events",
      async (r) => {
        if (calls++ === 0)
          await r.fulfill({
            headers: {
              "Content-Type": "text/event-stream",
              "Cache-Control": "no-store",
            },
            body: ": keepalive\n\n",
          });
        else await r.continue();
      },
    );
    await controller.signIn(page);
    await expect.poll(() => snapshots).toBeGreaterThan(1);
    await expect(page.locator("[data-ui=connectivity-banner]")).toContainText(
      "Snapshot current",
    );
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
    snapshot.recoveryRequired = true;
    await page.route("**/api/control/v1/projects/project-a/snapshot", (route) =>
      route.fulfill({
        json: snapshot,
        headers: { "Cache-Control": "no-store" },
      }),
    );
    await page
      .locator("[data-ui=connectivity-banner]")
      .getByRole("button", { name: "Refresh", exact: true })
      .click();
    await expect(page.getByText(text, { exact: true }).last()).toBeVisible();
    const notice = page.locator("[data-ui=connectivity-banner]");
    await expect(notice).toContainText(
      "External state requires reconciliation",
    );
    await expect(notice).not.toContainText("Snapshot current");
    await page.setViewportSize({ width: 320, height: 700 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);

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
});

test("operational TOTP Owner inspects real R09/Git evidence and commits one exact budget increase", async ({
  page,
  controller,
}) => {
  await controller.signIn(page);
  const base = `/api/control/v1/projects/${controller.info.project}`;
  const initial = await page.evaluate(
    async (path) =>
      (await fetch(path + "/snapshot", { cache: "no-store" })).json(),
    base,
  );
  expect(initial.schema).toBe(2);
  expect(initial.attempts).toHaveLength(2);
  expect(
    initial.results.filter((r: { accepted: boolean }) => r.accepted),
  ).toHaveLength(1);
  expect(
    initial.results.filter((r: { published: boolean }) => r.published),
  ).toHaveLength(1);
  expect(BigInt(initial.finance.reserved)).toBeGreaterThan(0n);
  await page
    .locator(".rv-sidebar")
    .getByRole("link", { name: /Execution/ })
    .click();
  await expect(
    page.getByText("Raw logs have no approved redacted projection", {
      exact: false,
    }),
  ).toHaveCount(2);
  await page
    .locator(".rv-sidebar")
    .getByRole("link", { name: /Connections/ })
    .click();
  await expect(
    page.getByText("GitHub App", { exact: true }).first(),
  ).toBeVisible();
  await page
    .locator(".rv-sidebar")
    .getByRole("link", { name: /Results/ })
    .click();
  await expect(
    page
      .locator(".rv-result-evidence")
      .getByText("Independently accepted", { exact: true })
      .first(),
  ).toBeVisible();
  await expect(
    page
      .locator(".rv-result-evidence")
      .getByText("Unaccepted candidate", { exact: true })
      .first(),
  ).toBeVisible();
  await expect(
    page.getByText("Isolated reference reconciled", { exact: true }),
  ).toBeVisible();
  await page
    .locator(".rv-sidebar")
    .getByRole("link", { name: /Costs/ })
    .click();
  await page
    .getByRole("button", { name: "Increase ceiling", exact: true })
    .click();
  const dialog = page.getByRole("dialog", {
    name: "Exact-action approval preview",
  });
  const total = (BigInt(initial.finance.ceiling) + 5000n).toString();
  await dialog
    .getByLabel("New total ceiling · integer minor units")
    .fill(total);
  await dialog.getByRole("checkbox").check();
  await dialog.getByRole("button", { name: "Prepare exact approval" }).click();
  // Real journal invalidations must not discard an unfinished ceremony.
  await page.waitForTimeout(2200);
  await expect(dialog).toBeVisible();
  await privateInput(
    dialog.getByLabel("Current password · fresh Owner verification"),
    controller.info.password,
  );
  await privateInput(
    dialog.getByLabel("Fresh authenticator code"),
    await controller.freshFactor(),
  );
  await dialog.getByRole("button", { name: "Verify this action" }).click();
  await expect(dialog.getByText(/Fresh action proof:/)).toBeVisible();
  await page.waitForTimeout(2200);
  await expect(dialog).toBeVisible();
  await dialog
    .getByRole("button", { name: "Commit exact ceiling increase" })
    .click();
  await expect(dialog.getByText(/Ceiling increase committed/)).toBeVisible();
  const after = await page.evaluate(
    async (path) =>
      (await fetch(path + "/snapshot", { cache: "no-store" })).json(),
    base,
  );
  expect(after.finance.ceiling).toBe(total);
  expect(after.finance.version).toBe(initial.finance.version + 1);
  expect(after.finance.reserved).toBe(initial.finance.reserved);
  expect(after.attempts).toHaveLength(2);
  expect(after.plan.fingerprint).toBe(initial.plan.fingerprint);
  expect(after.cursor).toBeGreaterThan(initial.cursor);
  expect(after.finance.approvals[0].total).toBe(total);
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  expect(
    (
      await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
        .analyze()
    ).violations,
  ).toEqual([]);
  await page.getByRole("button", { name: "Session", exact: true }).click();
  await page.getByRole("menuitem", { name: "Sign out", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Sign in to Rolvya" }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      async (path) => (await fetch(path + "/snapshot")).status,
      base,
    ),
  ).toBe(401);
});

test("operational Viewer sees candidate versus acceptance but no financial or connection records", async ({
  page,
  controller,
}) => {
  await controller.signIn(page, true);
  const base = `/api/control/v1/projects/${controller.info.project}`;
  const snapshot = await page.evaluate(
    async (path) => (await fetch(path + "/snapshot")).json(),
    base,
  );
  expect(snapshot.finance).toBeNull();
  expect(snapshot.connections).toEqual([]);
  expect(
    snapshot.results.filter((r: { accepted: boolean }) => r.accepted),
  ).toHaveLength(1);
  for (const resource of ["approvals", "activity/finance", "connections/model"])
    expect(
      await page.evaluate(
        async (path) => (await fetch(path)).status,
        base + "/" + resource,
      ),
    ).toBe(403);
  await page
    .locator(".rv-sidebar")
    .getByRole("link", { name: /Costs/ })
    .click();
  await expect(
    page.getByText("Financial view unavailable", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Increase ceiling", exact: true }),
  ).toHaveCount(0);
});

test("operational membership version change closes an open SSE and clears the old projection", async ({
  page,
  controller,
}) => {
  await controller.signIn(page, true);
  const old = await page.evaluate(
    async (project) =>
      (await fetch(`/api/control/v1/projects/${project}/snapshot`)).json(),
    controller.info.project,
  );
  await controller.command("role");
  await expect(
    page.getByRole("heading", { name: "Project access denied" }),
  ).toBeVisible();
  await expect(page.locator(".rv-result-evidence")).toHaveCount(0);
  const current = await page.evaluate(
    async (project) =>
      (await fetch(`/api/control/v1/projects/${project}/snapshot`)).json(),
    controller.info.project,
  );
  expect(old.role).toBe("viewer");
  expect(current.role).toBe("editor");
  expect(current.eventCursor).not.toBe(old.eventCursor);
  expect(current.finance).toBeNull();
});

test.describe("controlled approval acknowledgements", () => {
  test.use({ serviceWorkers: "block" });
  test("operational lost approval response is reconciled without repeating the financial effect", async ({
    page,
    controller,
  }) => {
    await controller.signIn(page);
    const base = `/api/control/v1/projects/${controller.info.project}`;
    const initial = await page.evaluate(
      async (p) => (await fetch(p + "/snapshot")).json(),
      base,
    );
    await page
      .locator(".rv-sidebar")
      .getByRole("link", { name: /Costs/ })
      .click();
    await page
      .getByRole("button", { name: "Increase ceiling", exact: true })
      .click();
    const dialog = page.getByRole("dialog", {
      name: "Exact-action approval preview",
    });
    const total = (BigInt(initial.finance.ceiling) + 50n).toString();
    await dialog
      .getByLabel("New total ceiling · integer minor units")
      .fill(total);
    await dialog.getByRole("checkbox").check();
    await dialog
      .getByRole("button", { name: "Prepare exact approval" })
      .click();
    await privateInput(
      dialog.getByLabel("Current password · fresh Owner verification"),
      controller.info.password,
    );
    await dialog.getByRole("button", { name: "Verify this action" }).click();
    // Keep the genuine browser TLS/cookie/Fetch-Metadata request. Discard its
    // successful response before the application can observe the receipt.
    await page.evaluate(() => {
      const original = window.fetch.bind(window);
      Reflect.set(window, "budgetExecutions", 0);
      window.fetch = async (...args: Parameters<typeof fetch>) => {
        const response = await original(...args);
        if (
          typeof args[0] === "string" &&
          args[0].endsWith("/budget") &&
          typeof args[1]?.body === "string" &&
          JSON.parse(args[1].body).stage === "execute"
        ) {
          Reflect.set(
            window,
            "budgetExecutions",
            Number(Reflect.get(window, "budgetExecutions")) + 1,
          );
          if (response.status === 200) {
            await response.body?.cancel();
            throw new TypeError("Private fixture discarded acknowledgement");
          }
        }
        return response;
      };
    });
    await dialog
      .getByRole("button", { name: "Commit exact ceiling increase" })
      .click();
    await expect(dialog.getByText(/effect is unconfirmed/)).toBeVisible();
    await dialog
      .getByRole("button", { name: "Inspect recorded approval status" })
      .click();
    await expect(dialog.getByText(/Ceiling increase committed/)).toBeVisible();
    expect(
      await page.evaluate(() => Reflect.get(window, "budgetExecutions")),
    ).toBe(1);
    const after = await page.evaluate(
      async (p) => (await fetch(p + "/snapshot")).json(),
      base,
    );
    expect(after.finance.ceiling).toBe(total);
    expect(after.finance.version).toBe(initial.finance.version + 1);
    expect(after.finance.reserved).toBe(initial.finance.reserved);
    expect(after.attempts).toHaveLength(2);
  });
  test("operational browser reload during preparation cannot restore generic approval authority", async ({
    page,
    controller,
  }) => {
    await controller.signIn(page);
    await page
      .locator(".rv-sidebar")
      .getByRole("link", { name: /Costs/ })
      .click();
    await page
      .getByRole("button", { name: "Increase ceiling", exact: true })
      .click();
    const dialog = page.getByRole("dialog", {
      name: "Exact-action approval preview",
    });
    const path = `/api/control/v1/projects/${controller.info.project}/snapshot`;
    const initial = await page.evaluate(
      async (p) => (await fetch(p)).json(),
      path,
    );
    await dialog
      .getByLabel("New total ceiling · integer minor units")
      .fill((BigInt(initial.finance.ceiling) + 50n).toString());
    await dialog.getByRole("checkbox").check();
    await dialog
      .getByRole("button", { name: "Prepare exact approval" })
      .click();
    await expect(
      dialog.getByLabel("Current password · fresh Owner verification"),
    ).toBeVisible();
    await page.reload();
    await expect(
      page.getByRole("button", { name: "Increase ceiling", exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Commit exact ceiling increase" }),
    ).toHaveCount(0);
    const after = await page.evaluate(
      async (p) => (await fetch(p)).json(),
      path,
    );
    expect(after.finance.ceiling).toBe(initial.finance.ceiling);
    expect(after.finance.version).toBe(initial.finance.version);
  });
});
