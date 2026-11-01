import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
export function licenseBanner(root = process.cwd()) {
  const lock = JSON.parse(
    readFileSync(resolve(root, "package-lock.json"), "utf8"),
  );
  const sections = [];
  const inventory = [];
  for (const [path, record] of Object.entries(lock.packages)) {
    if (!path || record.dev || record.devOptional) continue;
    const directory = resolve(root, path);
    if (!existsSync(directory)) {
      if (record.optional) continue;
      throw Error(`Missing runtime dependency ${path}`);
    }
    const pkg = JSON.parse(
      readFileSync(resolve(directory, "package.json"), "utf8"),
    );
    if (
      !/^(MIT|ISC|BSD-2-Clause|BSD-3-Clause|Apache-2\.0|\(MIT OR Apache-2\.0\))$/.test(
        pkg.license ?? "",
      )
    )
      throw Error(`Unreviewed runtime license: ${pkg.name}: ${pkg.license}`);
    const file = [
      "LICENSE",
      "LICENSE.md",
      "LICENSE.txt",
      "license",
      "license.txt",
    ].find((p) => existsSync(resolve(directory, p)));
    if (!file) throw Error(`Missing license notice: ${pkg.name}`);
    inventory.push({
      name: pkg.name,
      version: pkg.version,
      license: pkg.license,
    });
    sections.push(
      `${pkg.name} ${pkg.version}\n${readFileSync(resolve(directory, file), "utf8")}`,
    );
  }
  const own = readFileSync(resolve(root, "../../LICENSE"), "utf8");
  return {
    inventory,
    banner:
      `/*! Rolvya and runtime third-party notices\n${own}\n${sections.join("\n\n")}\nExternal assets: Inter and IBM Plex Mono (SIL OFL 1.1), Lucide (ISC, derived Feather MIT). See apps/web/THIRD_PARTY_NOTICES.md in the source.\n*/`
        .replaceAll("*/", "* /")
        .replace(/\* \/$/, "*/"),
  };
}
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href)
  console.log(JSON.stringify(licenseBanner().inventory, null, 2));
