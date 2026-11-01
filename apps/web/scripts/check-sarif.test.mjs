import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { checkSarif } from "./check-sarif.mjs";
test("CodeQL gate rejects missing, failed and finding-bearing analysis", async () => {
  const dir = await mkdtemp(join(tmpdir(), "rolvya-sarif-"));
  try {
    await assert.rejects(checkSarif(dir), /No CodeQL/);
    const report = {
      runs: [
        {
          tool: { driver: { name: "CodeQL" } },
          invocations: [{ executionSuccessful: true }],
          results: [],
        },
      ],
    };
    await writeFile(join(dir, "javascript.sarif"), JSON.stringify(report));
    await checkSarif(dir);
    report.runs[0].results = [
      {
        ruleId: "test/example",
        message: { text: "Synthetic regression finding" },
      },
    ];
    await writeFile(join(dir, "javascript.sarif"), JSON.stringify(report));
    await assert.rejects(checkSarif(dir), /findings/);
    report.runs[0].results = [];
    report.runs[0].invocations[0].executionSuccessful = false;
    await writeFile(join(dir, "javascript.sarif"), JSON.stringify(report));
    await assert.rejects(checkSarif(dir), /Incomplete/);
  } finally {
    await rm(dir, { recursive: true });
  }
});
