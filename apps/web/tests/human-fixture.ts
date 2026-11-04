import {
  test as base,
  expect,
  type Page,
  type Locator,
} from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, chmod } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
interface FixtureInfo {
  url: string;
  email: string;
  password: string;
  viewerEmail: string;
  viewerPassword: string;
  totp: string | null;
  project: string;
  fingerprint: string;
  bindingApproval?: string;
}
interface Controller {
  info: FixtureInfo;
  command: (command: "emit" | "revoke" | "advance" | "role") => Promise<void>;
  freshFactor: () => Promise<string>;
  deliveredMessage: (
    kind: "otp" | "magic_link",
  ) => Promise<{ reference: string; token: string }>;
  signIn: (
    page: Page,
    viewer?: boolean,
    expectProject?: boolean,
  ) => Promise<void>;
}
export const test = base.extend<{ controller: Controller }>({
  controller: [
    async ({ browserName }, provide, testInfo) => {
      // Never place credentials in .artifacts, traces, screenshots, argv or logs.
      const dir = await mkdtemp(
        resolve(tmpdir(), `rolvya-human-browser-${browserName}-`),
      );
      await chmod(dir, 0o700);
      const factor = testInfo.title.includes("TOTP");
      const operational = testInfo.title.includes("operational");
      const installation = testInfo.title.includes("installation bootstrap");
      const child = spawn(
        "cargo",
        [
          "test",
          "-p",
          "rolvya-persistence-sqlite",
          "--lib",
          "--locked",
          "controller_keys::human_auth::sessions::https::tests::driver::browser_https_driver",
          "--",
          "--ignored",
          "--exact",
          "--nocapture",
        ],
        {
          cwd: resolve("../.."),
          stdio: ["pipe", "ignore", "ignore"],
          env: {
            ...process.env,
            CARGO_INCREMENTAL: "0",
            ROLVYA_BROWSER_FIXTURE_DIR: dir,
            ROLVYA_BROWSER_FIXTURE_ASSETS: resolve("dist"),
            ...(operational ? { ROLVYA_BROWSER_FIXTURE_OPERATIONAL: "1" } : {}),
            ...(installation
              ? { ROLVYA_BROWSER_FIXTURE_INSTALLATION: "1" }
              : {}),
            ...(factor ? { ROLVYA_BROWSER_FIXTURE_TOTP: "1" } : {}),
          },
        },
      );
      let failed = false;
      child.once("error", () => {
        failed = true;
      });
      try {
        let info: FixtureInfo | undefined;
        for (let count = 0; count < 1800; count++) {
          if (failed || child.exitCode !== null)
            throw new Error(
              "Private Rust HTTPS fixture failed; run its focused utility test for diagnostics.",
            );
          try {
            info = JSON.parse(
              await readFile(resolve(dir, "info.json"), "utf8"),
            ) as FixtureInfo;
            break;
          } catch {
            await delay(100);
          }
        }
        if (!info || !/^https:\/\/localhost:\d+$/.test(info.url))
          throw new Error("Private Rust HTTPS fixture did not become ready.");
        let sequence = 0;
        const command = async (value: string) => {
          sequence++;
          child.stdin.write(value + "\n");
          for (let count = 0; count < 100; count++) {
            try {
              if (
                (await readFile(resolve(dir, "ack"), "utf8")) ===
                String(sequence)
              )
                return;
            } catch {
              /* pending commit */
            }
            if (child.exitCode !== null) break;
            await delay(50);
          }
          throw new Error("Private fixture state change was not acknowledged.");
        };
        await provide({
          info,
          command,
          freshFactor: async () => {
            await command("factor");
            return await readFile(resolve(dir, "message"), "utf8");
          },
          deliveredMessage: async (kind) => {
            await command("mail:" + kind);
            let value: unknown;
            try {
              value = JSON.parse(
                await readFile(resolve(dir, "message"), "utf8"),
              );
            } catch {
              throw Error("Invalid private independently delivered message.");
            }
            if (
              !value ||
              typeof value !== "object" ||
              !("reference" in value) ||
              !("token" in value) ||
              typeof value.reference !== "string" ||
              !/^[a-f0-9]{32}$/.test(value.reference) ||
              typeof value.token !== "string" ||
              !(kind === "otp" ? /^\d{8}$/ : /^[a-f0-9]{64}$/).test(value.token)
            )
              throw Error("Invalid private independently delivered message.");
            return { reference: value.reference, token: value.token };
          },
          signIn: async (page, viewer = false, expectProject = true) => {
            await page.goto(info.url + "/#/projects/" + info.project);
            await page
              .getByLabel("Email address")
              .fill(viewer ? info.viewerEmail : info.email);
            await privateInput(
              page.getByLabel("Password", { exact: true }),
              viewer ? info.viewerPassword : info.password,
            );
            await page
              .getByRole("button", { name: "Sign in", exact: true })
              .click();
            if (factor && !viewer) {
              await privateInput(
                page.getByLabel("Authenticator code"),
                info.totp!,
              );
              await page
                .getByRole("button", { name: "Verify and sign in" })
                .click();
            }
            if (expectProject)
              await expect(
                page.getByRole("heading", { name: info.project, exact: true }),
              ).toBeVisible();
          },
        });
      } finally {
        child.stdin.end("stop\n");
        const ended = await Promise.race([
          new Promise<boolean>((done) => {
            if (child.exitCode !== null || failed) done(true);
            else child.once("exit", () => done(true));
          }),
          delay(20000).then(() => false),
        ]);
        if (!ended) child.kill("SIGKILL");
        await rm(dir, { recursive: true, force: true });
      }
    },
    { timeout: 200000 },
  ],
});
export async function privateInput(field: Locator, value: string) {
  try {
    await field.fill(value, { timeout: 5000 });
  } catch {
    throw new Error("Private fixture input was not accepted.");
  }
}
export { expect };
