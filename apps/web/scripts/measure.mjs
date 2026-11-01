// Development-only measurements of optimized demo artifacts, never provider execution.
import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import vm from "node:vm";
import { gzipSync, brotliCompressSync } from "node:zlib";
await mkdir(".artifacts", { recursive: true });
const file = (await readdir("dist")).find((x) => /^app\..*\.js$/.test(x));
if (!file) throw Error("Build production first");
const code = await readFile("dist/" + file, "utf8");
const compile = [];
for (let i = 0; i < 10; i++) {
  const start = performance.now();
  new vm.Script(code + `\n// independent parse sample ${i}`, {
    filename: file,
  });
  compile.push(performance.now() - start);
}
const server = spawn(
  process.execPath,
  [
    "node_modules/vite/bin/vite.js",
    "preview",
    "--mode",
    "demo",
    "--host",
    "127.0.0.1",
    "--port",
    "4180",
  ],
  { stdio: "ignore" },
);
let browser;
try {
  let ready = false;
  for (let n = 0; n < 100; n++) {
    try {
      if ((await fetch("http://127.0.0.1:4180")).ok) {
        ready = true;
        break;
      }
    } catch {
      /* Wait for the local static preview to become ready. */
    }
    await new Promise((r) => setTimeout(r, 50));
  }
  if (!ready) throw Error("Preview unavailable");
  browser = await chromium.launch({
    ...(process.env.ROLVYA_CHROMIUM
      ? { executablePath: process.env.ROLVYA_CHROMIUM }
      : {}),
  });
  const samples = [];
  for (const { nodes, full } of [
    { nodes: 200, full: true },
    { nodes: 1000, full: true },
    { nodes: 5000, full: true },
    { nodes: 5000, full: false },
  ]) {
    const context = await browser.newContext({
      viewport: { width: 1440, height: 1000 },
      serviceWorkers: "block",
    });
    await context.route("https://cdn.jsdelivr.net/**", (r) => r.abort());
    const page = await context.newPage();
    const cdp = await context.newCDPSession(page);
    await cdp.send("Performance.enable");
    const trace = [];
    cdp.on("Tracing.dataCollected", (e) => trace.push(...e.value));
    await cdp.send("Tracing.start", {
      categories: "v8,devtools.timeline",
      transferMode: "ReportEvents",
    });
    await page.addInitScript(() => {
      window.__layoutShifts = [];
      try {
        new PerformanceObserver((list) => {
          for (const e of list.getEntries())
            if (!e.hadRecentInput) window.__layoutShifts.push(e.value);
        }).observe({ type: "layout-shift", buffered: true });
      } catch {
        /* Wait for the local static preview to become ready. */
      }
    });
    const start = performance.now();
    await page.goto(
      `http://127.0.0.1:4180/?nodes=${nodes}${full ? "&profile-full=1" : ""}`,
    );
    await page
      .locator(".react-flow__node")
      .nth((full ? nodes : 200) - 1)
      .waitFor({ timeout: 120000 });
    const renderMs = performance.now() - start;
    const frames = await page.evaluate(
      () =>
        new Promise((resolve) => {
          const times = [];
          let before = performance.now();
          function tick(now) {
            times.push(now - before);
            before = now;
            if (times.length === 61) resolve(times.slice(1));
            else requestAnimationFrame(tick);
          }
          requestAnimationFrame(tick);
        }),
    );
    const metrics = (await cdp.send("Performance.getMetrics")).metrics;
    const complete = new Promise((resolve) =>
      cdp.once("Tracing.tracingComplete", resolve),
    );
    await cdp.send("Tracing.end");
    await complete;
    const compileEvents = trace.filter(
      (e) => /Compile|Parse/.test(e.name) && e.dur,
    );
    samples.push({
      sourceNodes: nodes,
      renderedNodes: await page.locator(".react-flow__node").count(),
      graphPreviewLimit: full ? 5000 : 200,
      fullStressHarness: full,
      navigationToRenderedNodesMs: Math.round(renderMs),
      browserScriptMs: Math.round(
        (metrics.find((x) => x.name === "ScriptDuration")?.value ?? 0) * 1000,
      ),
      heapBytes: metrics.find((x) => x.name === "JSHeapUsedSize")?.value,
      layoutCount: metrics.find((x) => x.name === "LayoutCount")?.value,
      cls: await page.evaluate(() =>
        window.__layoutShifts.reduce((a, b) => a + b, 0),
      ),
      frameMedianMs: frames.toSorted((a, b) => a - b)[30],
      framesAbove25Ms: frames.filter((x) => x > 25).length,
      traceCompileEvents: compileEvents.map((e) => ({
        name: e.name,
        durationMs: e.dur / 1000,
      })),
    });
    await context.close();
  }
  const report = {
    environment: {
      node: process.version,
      chromium: browser.version(),
      physicalDevice: false,
      fonts: "external resources blocked to measure fallback deterministically",
    },
    production: {
      file,
      bytes: Buffer.byteLength(code),
      gzipBytes: gzipSync(code).length,
      brotliBytes: brotliCompressSync(code).length,
      nodeV8ParseCompileMedianMs: compile.toSorted((a, b) => a - b)[5],
      parseMeasurement:
        "Node V8 vm.Script parse+compile, ten runs; Chromium compile trace events reported separately and may overlap",
    },
    samples,
  };
  await writeFile(
    ".artifacts/performance.json",
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
