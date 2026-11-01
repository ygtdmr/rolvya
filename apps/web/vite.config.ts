import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { transform } from "lightningcss";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
// @ts-expect-error Build-only JS module has a checked runtime contract.
import { licenseBanner } from "./scripts/licenses.mjs";
const hash = (value: string, algorithm = "sha256") =>
  createHash(algorithm).update(value).digest("base64");
export default defineConfig(({ mode, command }) => {
  const demo = mode === "demo";
  const base = process.cwd();
  const sources = [
    "node_modules/@xyflow/react/dist/style.css",
    "src/styles/tokens.css",
    "src/styles/elements.css",
    "src/styles/layout.css",
  ];
  const css =
    "/*rv-style-v1*/" +
    transform({
      filename: "rolvya.css",
      code: Buffer.from(
        sources
          .map((file) =>
            file.includes("@xyflow")
              ? "@layer vendor {" +
                readFileSync(resolve(base, file), "utf8") +
                "}"
              : readFileSync(resolve(base, file), "utf8"),
          )
          .join("\n"),
      ),
      minify: true,
      targets: {
        chrome: 120 << 16,
        firefox: 120 << 16,
        safari: (17 << 16) | (4 << 8),
      },
    }).code.toString();
  const csp = `default-src 'none'; script-src 'self'; style-src 'sha256-${hash(css)}'; style-src-attr 'none'; img-src 'self' https://cdn.jsdelivr.net; font-src https://cdn.jsdelivr.net; connect-src 'self'; worker-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'`;
  const headerCsp = `${csp}; frame-ancestors 'none'`;
  const policy: Plugin = {
    name: "rolvya-single-artifact",
    enforce: "post",
    buildStart() {
      this.emitFile({
        type: "asset",
        fileName: "manifest.webmanifest",
        source: readFileSync(
          resolve(base, "public/manifest.webmanifest"),
          "utf8",
        ),
      });
    },
    resolveId(id) {
      if (id === "virtual:rolvya-styles") return "\0" + id;
    },
    load(id) {
      if (id === "\0virtual:rolvya-styles")
        return `export default ${JSON.stringify(css)};`;
    },
    transformIndexHtml: {
      order: "pre",
      handler() {
        return command === "build"
          ? [
              {
                tag: "meta",
                attrs: {
                  "http-equiv": "Content-Security-Policy",
                  content: csp,
                },
                injectTo: "head-prepend",
              },
            ]
          : [];
      },
    },
    generateBundle: {
      order: "post",
      handler(_, bundle) {
        const app = Object.values(bundle).filter((f) => f.type === "chunk");
        if (app.length !== 1 || !app[0])
          throw Error("Exactly one application chunk is required");
        const code = app[0].code,
          name = app[0].fileName;
        const html = bundle["index.html"];
        if (html?.type !== "asset")
          throw Error("Missing HTML asset: " + Object.keys(bundle).join(","));
        const integrity = `sha384-${hash(code, "sha384")}`;
        html.source = String(html.source).replace(
          /<script\b([^>]*?)src="([^"]+)"([^>]*)>/,
          `<script$1src="$2"$3 integrity="${integrity}">`,
        );
        const version = createHash("sha256")
          .update(code)
          .digest("hex")
          .slice(0, 20);
        const sw = readFileSync(resolve(base, "public/sw.js"), "utf8")
          .replaceAll(
            "__CACHE_NAME__",
            `rolvya-shell-v1-${demo ? "demo" : "connected"}-${version}`,
          )
          .replace(
            "__SHELL_PATHS__",
            JSON.stringify([
              "/",
              "/index.html",
              "/" + name,
              "/manifest.webmanifest",
            ]),
          );
        this.emitFile({ type: "asset", fileName: "sw.js", source: sw });
      },
    },
  };
  return {
    publicDir: false,
    plugins: [react(), policy],
    define: {
      __DEMO__: JSON.stringify(demo),
      __BUILD_ID__: JSON.stringify("web-v1"),
    },
    build: {
      outDir: demo ? "dist-demo" : "dist",
      target: ["chrome120", "firefox120", "safari17.4"],
      assetsInlineLimit: 0,
      cssCodeSplit: false,
      sourcemap: false,
      minify: true,
      modulePreload: false,
      rolldownOptions: {
        output: {
          codeSplitting: false,
          entryFileNames: "app.[hash].js",
          banner: licenseBanner(base).banner,
        },
      },
    },
    server: { host: "127.0.0.1", strictPort: true },
    preview: {
      host: "127.0.0.1",
      strictPort: true,
      headers: {
        "Content-Security-Policy": headerCsp,
        "X-Content-Type-Options": "nosniff",
        "Referrer-Policy": "no-referrer",
        "X-Frame-Options": "DENY",
        "Permissions-Policy":
          "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
        "X-Robots-Tag": "noindex, nofollow",
        "Cache-Control": "no-cache",
        "X-Rolvya-Static": "public-shell-v1",
      },
    },
  };
});
