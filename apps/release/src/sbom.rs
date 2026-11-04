//! SPDX 2.3 inventory for the exact controller target closure and rendered browser packages.
use crate::{
    files::{Result, Tree, digest},
    process::output,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};

fn expression(raw: &str) -> Result<String> {
    // Legacy Cargo spelling is an alternative license, not an SPDX operator.
    let value = raw.replace(" / ", " OR ").replace('/', " OR ");
    if !matches!(
        value.as_str(),
        "MIT"
            | "MIT OR Apache-2.0"
            | "Apache-2.0 OR MIT"
            | "Apache-2.0"
            | "ISC"
            | "Unicode-3.0"
            | "Apache-2.0 OR ISC OR MIT"
            | "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT"
            | "CC0-1.0 OR MIT-0 OR Apache-2.0"
            | "Zlib"
            | "MIT AND BSD-3-Clause"
            | "Unlicense OR MIT"
            | "Apache-2.0 AND ISC"
            | "Apache-2.0 OR BSL-1.0"
            | "BSD-3-Clause"
            | "(MIT OR Apache-2.0) AND Unicode-3.0"
            | "CDLA-Permissive-2.0"
            | "BSD-2-Clause"
            | "(MIT OR Apache-2.0)"
    ) {
        return Err("unreviewed dependency license expression".into());
    }
    Ok(value)
}

fn closure(root: &Path, edges: &str) -> Result<BTreeSet<(String, String)>> {
    let text = output(
        root,
        "cargo",
        &[
            "tree",
            "--locked",
            "-p",
            "rolvya-controller",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--edges",
            edges,
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],
    )?;
    text.lines()
        .map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next().ok_or("invalid cargo tree")?;
            let version = parts
                .next()
                .and_then(|v| v.strip_prefix('v'))
                .ok_or("invalid cargo version")?;
            Ok((name.to_owned(), version.to_owned()))
        })
        .collect()
}

fn notices(directory: &Path) -> Result<String> {
    let tree = Tree::open(directory)?;
    let mut paths = BTreeSet::new();
    collect_notices(directory, directory, 0, &mut 0, &mut paths)?;
    let mut all = String::new();
    for name in paths {
        let bytes = tree.read(&name, 512 * 1024)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| "non-text license")?;
        all.push_str(&format!("\n{name}\n{text}\n"));
        if all.len() > 2 * 1024 * 1024 {
            return Err("license evidence too large".into());
        }
    }
    Ok(all)
}

fn collect_notices(
    root: &Path,
    directory: &Path,
    depth: usize,
    count: &mut usize,
    paths: &mut BTreeSet<String>,
) -> Result<()> {
    if depth > 12 {
        return Err("license directory depth exceeded".into());
    }
    for item in fs::read_dir(directory).map_err(|_| "license directory unavailable")? {
        let item = item.map_err(|_| "license metadata unavailable")?;
        *count += 1;
        if *count > 32768 {
            return Err("license inventory file limit".into());
        }
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| "invalid license path")?;
        let lower = name.to_ascii_lowercase();
        let kind = item
            .file_type()
            .map_err(|_| "license metadata unavailable")?;
        if kind.is_symlink() {
            return Err("linked license source forbidden".into());
        }
        if kind.is_dir() {
            collect_notices(root, &item.path(), depth + 1, count, paths)?;
        } else if ["license", "notice", "copying", "copyright"]
            .iter()
            .any(|p| lower.starts_with(p))
            || item
                .path()
                .strip_prefix(root)
                .map_err(|_| "license path escaped source")?
                .components()
                .any(|p| {
                    p.as_os_str().to_str().is_some_and(|n| {
                        ["license", "notice", "copying", "copyright"]
                            .iter()
                            .any(|prefix| n.to_ascii_lowercase().starts_with(prefix))
                    })
                })
        {
            let relative = item
                .path()
                .strip_prefix(root)
                .map_err(|_| "license path escaped source")?
                .to_str()
                .ok_or("invalid license path")?
                .to_owned();
            paths.insert(relative);
        }
    }
    Ok(())
}

pub(crate) fn generate(
    root: &Path,
    dist: &Path,
    commit: &str,
    created: &str,
) -> Result<(Value, String)> {
    let metadata: Value = serde_json::from_str(&output(
        root,
        "cargo",
        &[
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            "x86_64-unknown-linux-gnu",
        ],
    )?)
    .map_err(|_| "invalid Cargo metadata")?;
    let all = closure(root, "normal,build")?;
    let normal = closure(root, "normal")?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or("missing Cargo packages")?;
    let mut inventory = Vec::new();
    let mut relationships = Vec::new();
    let mut notice = String::from(
        "Rolvya release candidate — third-party notices\nThe controller closure includes normal, procedural macro and build dependencies; this does not assert that every crate is linked at runtime. No third-party executables are bundled. External fonts/icons are fetched separately and have system/label fallbacks.\n",
    );
    let mut found = BTreeSet::new();
    for (name, version) in &all {
        let matching: Vec<_> = packages
            .iter()
            .filter(|p| p["name"] == *name && p["version"] == *version)
            .collect();
        if matching.len() != 1 {
            return Err("ambiguous Cargo component".into());
        }
        let p = matching[0];
        let id = format!("SPDXRef-cargo-{name}-{version}");
        let license = expression(p["license"].as_str().ok_or("missing Cargo license")?)?;
        let directory = Path::new(
            p["manifest_path"]
                .as_str()
                .ok_or("missing package directory")?,
        )
        .parent()
        .ok_or("missing package directory")?;
        let first_party = p["source"].is_null();
        let mut text = if first_party {
            String::from_utf8(Tree::open(root)?.read("LICENSE", 32768)?)
                .map_err(|_| "invalid MIT license")?
        } else {
            notices(directory)?
        };
        if text.is_empty() {
            text = match (name.as_str(), version.as_str()) {
                ("base64-simd" | "vsimd", "0.8.0") => {
                    include_str!("../licenses/simd-MIT.txt").into()
                }
                ("crc-catalog", "2.5.0") => include_str!("../licenses/crc-catalog-MIT.txt").into(),
                _ => return Err("missing redistribution notices".into()),
            };
        }
        notice.push_str(&format!(
            "\n===== Cargo {name} {version} — {license} =====\n{text}\n"
        ));
        let checksum = if first_party {
            commit.to_owned()
        } else {
            // Cargo registry extraction does not promise a .cargo-checksum.json
            // file (that belongs to vendored sources). Hash the actual cached
            // archive verified by the preceding locked Cargo build instead.
            if p["source"].as_str() != Some("registry+https://github.com/rust-lang/crates.io-index")
            {
                return Err("unreviewed registry source".into());
            }
            let registry = directory
                .ancestors()
                .nth(3)
                .ok_or("missing registry cache")?;
            let cache = registry.join("cache").join(
                directory
                    .parent()
                    .and_then(Path::file_name)
                    .ok_or("missing registry identity")?,
            );
            digest(&Tree::open(&cache)?.read(&format!("{name}-{version}.crate"), 16 * 1024 * 1024)?)
        };
        inventory.push(json!({"SPDXID":id,"name":name,"versionInfo":version,"downloadLocation":if first_party {format!("git+https://github.com/ygtdmr/rolvya@{commit}")} else {format!("https://crates.io/api/v1/crates/{name}/{version}/download")},
            "filesAnalyzed":false,"licenseConcluded":"NOASSERTION","licenseDeclared":license,"copyrightText":"NOASSERTION",
            "comment":format!("{}; source integrity {}", if normal.contains(&(name.clone(),version.clone())) {"controller normal dependency closure (may include compile-time procedural macros)"} else {"controller build-only dependency closure"},checksum),
            "externalRefs":[{"referenceCategory":"PACKAGE-MANAGER","referenceType":"purl","referenceLocator":format!("pkg:cargo/{name}@{version}")}]}));
        if !first_party && let Some(item) = inventory.last_mut() {
            item["checksums"] = json!([{"algorithm":"SHA256","checksumValue":checksum}]);
        }
        relationships.push(json!({"spdxElementId":"SPDXRef-controller","relationshipType":"DEPENDS_ON","relatedSpdxElement":id}));
        found.insert((name.clone(), version.clone()));
    }
    if found != all
        || all
            .iter()
            .any(|(n, _)| matches!(n.as_str(), "rsa" | "sqlx-mysql"))
    {
        return Err("unexpected or incomplete SQLite production closure".into());
    }
    let web = Tree::open(&root.join("apps/web"))?;
    let bundle: Value =
        serde_json::from_slice(&web.read(".artifacts/bundle-production.json", 65536)?)
            .map_err(|_| "missing browser module provenance")?;
    let app = bundle["application"]
        .as_str()
        .ok_or("missing application identity")?;
    if bundle["version"] != 1
        || !crate::manifest::payload_allowed(&format!("web/{app}"))
        || digest(&Tree::open(&dist.join("web"))?.read(app, 4 * 1024 * 1024)?)
            != bundle["sha256"].as_str().ok_or("missing browser hash")?
    {
        return Err("stale browser module provenance".into());
    }
    let lock: Value = serde_json::from_slice(&web.read("package-lock.json", 2 * 1024 * 1024)?)
        .map_err(|_| "invalid npm lock")?;
    for record in bundle["packages"]
        .as_array()
        .ok_or("missing browser packages")?
    {
        let name = record["name"].as_str().ok_or("invalid npm component")?;
        let location = record["directory"]
            .as_str()
            .ok_or("missing npm installation directory")?;
        if name.starts_with("@types/")
            || name.contains("..")
            || name.split('/').count() > 2
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"@/_-".contains(&b))
        {
            return Err("invalid rendered npm package".into());
        }
        if !location.starts_with("node_modules/")
            || location.split('/').any(|p| {
                p.is_empty()
                    || p == "."
                    || p == ".."
                    || !p
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"@._-".contains(&b))
            })
        {
            return Err("invalid npm installation directory".into());
        }
        let directory = root.join("apps/web").join(location);
        let p: Value =
            serde_json::from_slice(&Tree::open(&directory)?.read("package.json", 65536)?)
                .map_err(|_| "invalid npm package")?;
        if p["name"] != name {
            return Err("npm component substitution".into());
        }
        let version = p["version"].as_str().ok_or("missing npm version")?;
        if lock["packages"][location]["version"] != version {
            return Err("npm lock/component mismatch".into());
        }
        let license = expression(p["license"].as_str().ok_or("missing npm license")?)?;
        let text = notices(&directory)?;
        if text.is_empty() {
            return Err("missing npm notices".into());
        }
        notice.push_str(&format!(
            "\n===== npm {name} {version} — {license} =====\n{text}\n"
        ));
        let id = format!(
            "SPDXRef-npm-{}-{version}-{}",
            name.replace(['@', '/'], "-"),
            &digest(location.as_bytes())[..12]
        );
        inventory.push(json!({"SPDXID":id,"name":name,"versionInfo":version,"downloadLocation":format!("https://www.npmjs.com/package/{name}/v/{version}"),"filesAnalyzed":false,"licenseConcluded":"NOASSERTION","licenseDeclared":license,"copyrightText":"NOASSERTION","comment":"rendered browser module or compiled React Flow stylesheet; development-only packages excluded"}));
        relationships.push(json!({"spdxElementId":"SPDXRef-browser","relationshipType":"DEPENDS_ON","relatedSpdxElement":id}));
    }
    for (id, name) in [
        ("SPDXRef-controller", "rolvya-controller"),
        ("SPDXRef-browser", "rolvya-browser"),
    ] {
        inventory.push(json!({"SPDXID":id,"name":name,"versionInfo":"0.1.0-rc.1","downloadLocation":"NOASSERTION","filesAnalyzed":false,"licenseDeclared":"MIT","licenseConcluded":"NOASSERTION","copyrightText":"NOASSERTION"}));
        relationships.push(json!({"spdxElementId":"SPDXRef-DOCUMENT","relationshipType":"DESCRIBES","relatedSpdxElement":id}));
    }
    let mut files = Vec::new();
    for (i, f) in Tree::open(dist)?
        .inventory()?
        .iter()
        .filter(|f| f.path.starts_with("bin/") || f.path.starts_with("web/"))
        .enumerate()
    {
        let id = format!("SPDXRef-file-{i}");
        files.push(json!({"SPDXID":id,"fileName":format!("./{}",f.path),"checksums":[{"algorithm":"SHA256","checksumValue":f.sha256}],"licenseConcluded":"NOASSERTION","copyrightText":"NOASSERTION"}));
        relationships.push(json!({"spdxElementId":if f.path.starts_with("bin/"){"SPDXRef-controller"}else{"SPDXRef-browser"},"relationshipType":"CONTAINS","relatedSpdxElement":id}));
    }
    Ok((
        json!({"spdxVersion":"SPDX-2.3","dataLicense":"CC0-1.0","SPDXID":"SPDXRef-DOCUMENT","name":"Rolvya candidate dependency inventory","documentNamespace":format!("https://rolvya.invalid/spdx/{commit}/{}",digest(&Tree::open(dist)?.read("bin/rolvya-controller",crate::files::LIMIT)?)),"creationInfo":{"created":created,"creators":["Tool: rolvya-release-0.1.0"]},"packages":inventory,"files":files,"relationships":relationships}),
        notice,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_copyleft_missing_and_malformed_licenses_are_not_approved() {
        for value in [
            "",
            "GPL-3.0",
            "SEE LICENSE",
            "MIT OR Proprietary",
            "MIT;evil",
        ] {
            assert!(expression(value).is_err());
        }
        assert_eq!(expression("MIT/Apache-2.0").unwrap(), "MIT OR Apache-2.0");
        assert_eq!(
            expression("Apache-2.0 AND ISC").unwrap(),
            "Apache-2.0 AND ISC"
        );
    }
    #[test]
    fn nested_vendored_licenses_and_license_directories_are_preserved() {
        let d = tempfile::tempdir().unwrap();
        for path in [
            "src/polyfill/once_cell/LICENSE-MIT",
            "third_party/fiat/LICENSE",
            "LICENSES/Apache-2.0.txt",
        ] {
            let file = d.path().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, path).unwrap();
        }
        let text = notices(d.path()).unwrap();
        for expected in [
            "src/polyfill/once_cell/LICENSE-MIT",
            "third_party/fiat/LICENSE",
            "LICENSES/Apache-2.0.txt",
        ] {
            assert!(text.contains(expected));
        }
    }
}
