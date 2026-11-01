import { pathToFileURL } from "node:url";
import { readdir, lstat, readFile } from "node:fs/promises";
import { join, extname } from "node:path";
import { createHash } from "node:crypto";
import { gzipSync, brotliCompressSync } from "node:zlib";
import { parse } from "acorn";
import { parse as parseHtml } from "parse5";
const fail = (message) => {
  throw Error(`Distribution contract: ${message}`);
};
async function inventory(root, prefix = "") {
  const out = [];
  for (const name of await readdir(join(root, prefix))) {
    const relative = prefix ? `${prefix}/${name}` : name;
    const stat = await lstat(join(root, relative));
    if (stat.isSymbolicLink()) fail(`symlink ${relative}`);
    if (stat.isDirectory()) out.push(...(await inventory(root, relative)));
    else if (stat.isFile()) out.push(relative);
    else fail(`special file ${relative}`);
  }
  return out.sort();
}
export async function auditDist(root) {
  const files = await inventory(root);
  if (files.some((f) => extname(f).toLowerCase() === ".css"))
    fail("emitted CSS stylesheet");
  const executable = files.filter((f) => /\.(?:[cm]?js)$/i.test(f));
  const apps = executable.filter((f) => /^app\.[\w-]{8,}\.js$/.test(f));
  if (executable.length !== 2 || apps.length !== 1 || !files.includes("sw.js"))
    fail("exactly one hashed app JS and one standalone sw.js required");
  const app = apps[0];
  for (const file of files)
    if (!["index.html", "manifest.webmanifest", "sw.js", app].includes(file))
      fail(`unapproved asset ${file}`);
  const [html, code, sw, manifestText] = await Promise.all(
    ["index.html", app, "sw.js", "manifest.webmanifest"].map((f) =>
      readFile(join(root, f), "utf8"),
    ),
  );
  if (Buffer.byteLength(code) > 4 * 1024 * 1024)
    fail("application JS exceeds the bounded audit size");
  if (Buffer.byteLength(html) > 40000) fail("HTML exceeds bounded shell size");
  // Inert parsing with the same noscript rules as a JS-enabled browser. There
  // is no execution or resource loader; regex and scripting-disabled DOMs can
  // miss executable tags that the production browser sees.
  const nodes = [],
    pending = [
      parseHtml(html, { scriptingEnabled: true, sourceCodeLocationInfo: true }),
    ];
  while (pending.length) {
    const node = pending.pop();
    if (node.tagName) nodes.push(node);
    pending.push(...(node.childNodes ?? []));
    if (node.content) pending.push(node.content);
  }
  const attributes = (node) =>
    Object.fromEntries((node.attrs ?? []).map((a) => [a.name, a.value]));
  if (
    /unsafe-inline|unsafe-eval/i.test(html) ||
    nodes.some(
      (node) =>
        node.tagName === "style" ||
        (node.tagName === "link" &&
          (attributes(node).rel ?? "")
            .toLowerCase()
            .split(/\s+/)
            .includes("stylesheet")),
    ) ||
    /sourceMappingURL/.test(code)
  )
    fail("inline style/script policy, external stylesheet, or source map");
  const tags = nodes.filter((node) => node.tagName === "script");
  if (
    tags.length !== 1 ||
    tags[0].childNodes.some((node) => (node.value ?? "").trim())
  )
    fail("one external module script, no inline script");
  const attrs = attributes(tags[0]);
  if (
    attrs.src !== "/" + app ||
    attrs.type !== "module" ||
    attrs.integrity !==
      `sha384-${createHash("sha384").update(code).digest("base64")}`
  )
    fail("broken app reference or integrity");
  const metas = nodes.filter(
    (node) =>
      node.tagName === "meta" &&
      attributes(node)["http-equiv"]?.toLowerCase() ===
        "content-security-policy",
  );
  if (
    metas.length !== 1 ||
    metas[0].parentNode?.tagName !== "head" ||
    !(
      metas[0].sourceCodeLocation?.startOffset <
      tags[0].sourceCodeLocation?.startOffset
    )
  )
    fail("CSP must be in head before executable content");
  const policy = attributes(metas[0]).content;
  if (
    !policy ||
    /unsafe-inline|unsafe-eval/.test(policy) ||
    !policy.includes("style-src-attr 'none'") ||
    !policy.includes("connect-src 'self'") ||
    !policy.includes("object-src 'none'")
  )
    fail("missing strict CSP");
  const css = [];
  const stack = [parse(code, { ecmaVersion: "latest", sourceType: "module" })];
  while (stack.length) {
    const node = stack.pop();
    if (!node || typeof node !== "object") continue;
    if (
      [
        "ImportDeclaration",
        "ImportExpression",
        "ExportAllDeclaration",
      ].includes(node.type) ||
      (node.type === "ExportNamedDeclaration" && node.source)
    )
      fail("external JS chunk/import");
    if (
      node.type === "CallExpression" &&
      ["eval", "importScripts"].includes(node.callee?.name)
    )
      fail("runtime code loading/evaluation");
    if (
      node.type === "NewExpression" &&
      ["Function", "Worker", "SharedWorker"].includes(node.callee?.name)
    )
      fail("unapproved executable constructor");
    const text =
      node.type === "Literal" && typeof node.value === "string"
        ? node.value
        : node.type === "TemplateLiteral" && node.expressions.length === 0
          ? node.quasis.map((q) => q.value.cooked).join("")
          : null;
    if (text !== null) {
      if (text.startsWith("/*rv-style-v1*/")) css.push(text);
      if (
        /data:(?:image|font|application\/(?:font|x-font))|<svg\b|d09GMg|d09GRg|PHN2Zy/i.test(
          text,
        )
      )
        fail("embedded font/icon payload");
    }
    for (const value of Object.values(node)) {
      if (Array.isArray(value)) stack.push(...value);
      else if (value && typeof value === "object") stack.push(value);
    }
  }
  if (css.length !== 1 || !css[0].includes(".rv-app"))
    fail("missing single compiled application stylesheet");
  const cssHash = createHash("sha256").update(css[0]).digest("base64");
  if (!policy.includes(`'sha256-${cssHash}'`))
    fail("style hash does not authorize actual compiled CSS");
  if (
    /__SHELL_PATHS__|__CACHE_NAME__|importScripts|\beval\s*\(/.test(sw) ||
    !sw.includes(app)
  )
    fail("invalid standalone SW version/static inventory");
  const manifest = JSON.parse(manifestText);
  if (
    manifest.id !== "/" ||
    manifest.start_url !== "/" ||
    manifest.scope !== "/" ||
    manifest.name !== "Rolvya — Connected Control" ||
    !Array.isArray(manifest.icons)
  )
    fail("invalid scoped manifest");
  if (manifest.icons.length)
    fail("brand icon provenance is unverified; cannot ship invented icons");
  return {
    files,
    jsBytes: Buffer.byteLength(code),
    gzipBytes: gzipSync(code).length,
    brotliBytes: brotliCompressSync(code).length,
    cssBytes: Buffer.byteLength(css[0]),
    cssHash,
    brandIcons:
      "UNVERIFIED — favicon/installability release gate remains blocked",
  };
}
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href)
  console.log(
    JSON.stringify(
      await auditDist(
        process.env.DIST_DIR ??
          (process.argv[2] && !process.argv[2].startsWith("--")
            ? process.argv[2]
            : "dist"),
      ),
      null,
      2,
    ),
  );
