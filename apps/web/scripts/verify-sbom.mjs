// Build-only schema validation. AJV is pinned by the npm lock; never shipped.
import Ajv from "ajv";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
const schema = JSON.parse(
  readFileSync(
    fileURLToPath(new URL("./vendor/spdx-2.3.schema.json", import.meta.url)),
    "utf8",
  ),
);
const validate = new Ajv({ allErrors: true, schemaId: "auto" }).compile(schema);
const file = process.argv[2];
if (!file || process.argv.length !== 3)
  throw Error("one SPDX document required");
const bytes = readFileSync(file);
if (
  bytes.length > 8 * 1024 * 1024 ||
  !validate(JSON.parse(bytes.toString("utf8")))
)
  throw Error("SPDX 2.3 schema validation failed");
console.log("SPDX 2.3 schema valid");
