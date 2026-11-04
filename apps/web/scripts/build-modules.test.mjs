import { test } from "node:test";
import assert from "node:assert/strict";
import { renderedPackages } from "./build-modules.mjs";
test("nested and distinct installed versions retain their exact module directories", () => {
  const rows = renderedPackages(
    {
      "/app/node_modules/actual/lib/a.js": { renderedLength: 1 },
      "/app/node_modules/outer/node_modules/actual/lib/b.js": {
        renderedLength: 1,
      },
    },
    "/app",
  );
  assert.deepEqual(
    rows.filter((p) => p.name === "actual").map((p) => p.directory),
    ["node_modules/actual", "node_modules/outer/node_modules/actual"],
  );
  assert.ok(!rows.some((p) => p.name === "outer"));
});
test("unrendered modules and type-only dependencies are not inferred as deployed", () => {
  assert.equal(
    renderedPackages(
      { "/app/node_modules/unused/a.js": { renderedLength: 0 } },
      "/app",
    ).length,
    1,
  );
});
test("module paths outside the approved npm installation fail closed", () => {
  assert.throws(() =>
    renderedPackages(
      { "/other/node_modules/actual/a.js": { renderedLength: 2 } },
      "/app",
    ),
  );
  assert.throws(() =>
    renderedPackages(
      { "/app/node_modules/../actual/a.js": { renderedLength: 2 } },
      "/app",
    ),
  );
});
