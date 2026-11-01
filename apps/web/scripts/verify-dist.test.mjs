import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { auditDist } from "./verify-dist.mjs";
test("an emitted CSS file or second executable is a hard distribution failure", async () => {
  const root = await mkdtemp(join(tmpdir(), "rolvya-dist-"));
  try {
    await writeFile(join(root, "extra.css"), "body{color:red}");
    await assert.rejects(auditDist(root), /CSS|stylesheet/);
  } finally {
    await rm(root, { recursive: true });
  }
});
import { createHash } from "node:crypto";
async function validDist(root) {
  const app = "app.abcdefgh.js",
    css = "/*rv-style-v1*/.rv-app{color:red}",
    code = `const css=${JSON.stringify(css)};`;
  await writeFile(join(root, app), code);
  await writeFile(
    join(root, "index.html"),
    `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; connect-src 'self'; object-src 'none'; style-src-attr 'none'; style-src 'sha256-${createHash("sha256").update(css).digest("base64")}'"><script type="module" src="/${app}" integrity="sha384-${createHash("sha384").update(code).digest("base64")}"></script>`,
  );
  await writeFile(join(root, "sw.js"), `const app='${app}';`);
  await writeFile(
    join(root, "manifest.webmanifest"),
    JSON.stringify({
      id: "/",
      scope: "/",
      start_url: "/",
      name: "Rolvya — Connected Control",
      icons: [],
    }),
  );
  return app;
}
test("valid single-JS CSS/hash/SRI inventory passes recursively", async () => {
  const root = await mkdtemp(join(tmpdir(), "rolvya-dist-"));
  try {
    await validDist(root);
    assert.equal((await auditDist(root)).files.length, 4);
  } finally {
    await rm(root, { recursive: true });
  }
});
test("browser-valid unusual script endings and unclosed scripts cannot bypass inventory", async () => {
  const root = await mkdtemp(join(tmpdir(), "rolvya-dist-"));
  try {
    await validDist(root);
    const html = await readFile(join(root, "index.html"), "utf8");
    for (const extra of [
      "<script>globalThis.unapproved = true;</script\t\n bar>",
      "<script>globalThis.unapproved = true;</script extra>",
      "<script>globalThis.unapproved = true;",
      '<noscript><p title="</noscript><script src=evil.js></script>">',
    ]) {
      await writeFile(join(root, "index.html"), html + extra);
      await assert.rejects(auditDist(root), /one external module script/);
    }
  } finally {
    await rm(root, { recursive: true });
  }
});
test("CSP metadata must be in head before the executable script", async () => {
  const root = await mkdtemp(join(tmpdir(), "rolvya-dist-"));
  try {
    await validDist(root);
    const html = await readFile(join(root, "index.html"), "utf8");
    const boundary = html.indexOf("<script");
    const meta = html.slice(0, boundary);
    const script = html.slice(boundary);
    for (const moved of [
      `<head>${script}${meta}</head>`,
      `<head>${script}</head><body>${meta}</body>`,
    ]) {
      await writeFile(join(root, "index.html"), moved);
      await assert.rejects(auditDist(root), /CSP/);
    }
  } finally {
    await rm(root, { recursive: true });
  }
});
for (const extra of [
  "vendor.js",
  "extra.mjs",
  "app.abcdefgh.js.map",
  "font.woff2",
  "icon.svg",
])
  test(`distribution rejects unexpected ${extra}`, async () => {
    const root = await mkdtemp(join(tmpdir(), "rolvya-dist-"));
    try {
      await validDist(root);
      await writeFile(join(root, extra), "unapproved");
      await assert.rejects(auditDist(root));
    } finally {
      await rm(root, { recursive: true });
    }
  });
test("CSS mutation and broken SRI are rejected", async () => {
  const root = await mkdtemp(join(tmpdir(), "rolvya-dist-"));
  try {
    const app = await validDist(root);
    await writeFile(join(root, app), "const changed=true;");
    await assert.rejects(auditDist(root), /integrity/);
  } finally {
    await rm(root, { recursive: true });
  }
});
