// Build-host module attribution, never imported by the browser application.
export function renderedPackages(modules, root) {
  const base = root.replaceAll("\\", "/") + "/";
  const packages = new Map();
  for (const [id, rendered] of Object.entries(modules)) {
    if (rendered.renderedLength === 0) continue;
    const path = id.replaceAll("\\", "/");
    const start = path.lastIndexOf("/node_modules/");
    if (start < 0) continue;
    if (!path.startsWith(base))
      throw Error("Module outside reviewed npm installation");
    const match = path.slice(start + 14).match(/^((?:@[^/]+\/)?[^/]+)\//);
    if (!match) throw Error("Ambiguous npm module identity");
    const name = match[1];
    const directory = path.slice(base.length, start + 14 + name.length);
    if (
      !directory.startsWith("node_modules/") ||
      directory
        .split("/")
        .some((p) => p === "." || p === ".." || !/^[@\w.-]+$/.test(p))
    )
      throw Error("Unsafe npm installation path");
    packages.set(directory, { name, directory });
  }
  packages.set("node_modules/@xyflow/react", {
    name: "@xyflow/react",
    directory: "node_modules/@xyflow/react",
  });
  return [...packages.values()].sort((a, b) =>
    a.directory.localeCompare(b.directory, "en"),
  );
}
