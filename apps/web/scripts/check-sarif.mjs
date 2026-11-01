import { readFile, readdir } from "node:fs/promises";
import { join } from "node:path";
export async function checkSarif(directory) {
  const files = (await readdir(directory)).filter((f) => f.endsWith(".sarif"));
  if (!files.length) throw Error("No CodeQL SARIF analysis produced");
  let results = 0;
  for (const file of files) {
    const report = JSON.parse(await readFile(join(directory, file), "utf8"));
    if (!report.runs?.length) throw Error("Missing analysis runs");
    for (const run of report.runs) {
      if (
        !run.tool?.driver?.name?.includes("CodeQL") ||
        run.invocations?.some((x) => x.executionSuccessful === false)
      )
        throw Error("Incomplete CodeQL execution");
      for (const result of run.results ?? []) {
        results++;
        console.error(
          `${result.ruleId}: ${result.message?.text ?? "CodeQL finding"}`,
        );
      }
    }
  }
  if (results)
    throw Error(`${results} CodeQL findings require review and correction`);
  console.log(`${files.length} CodeQL SARIF file(s), zero findings`);
}
if (process.argv[1]?.endsWith("/check-sarif.mjs"))
  await checkSarif(process.argv[2] ?? ".artifacts/codeql");
