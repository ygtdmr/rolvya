//! Exhaustive specification inventory. Source inspection is never an execution receipt.
use crate::files::{Result, Tree, digest};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Row {
    id: String,
    specification: String,
    text: String,
    classification: &'static str,
    source: &'static str,
    test_source: &'static str,
    evidence_level: &'static str,
    limitation: &'static str,
}

fn mapping(n: usize) -> (&'static str, &'static str, &'static str, &'static str) {
    match n {
        1 => (
            "partially_implemented",
            "crates/persistence-sqlite/src/lib.rs",
            "crates/plan-validator/tests/validation.rs",
            "Frozen plan validation/activation exists; no installed main-AI-planner-to-dispatch acceptance journey.",
        ),
        2..=5 | 11 | 16 | 20 => (
            "implemented_but_only_simulated",
            "crates/persistence-sqlite/src/orchestration/mod.rs",
            "crates/persistence-sqlite/tests/orchestration.rs",
            "Real SQLite component fixtures; packaged service does not compose the paid orchestration loop.",
        ),
        6 | 17 | 18 | 28 => (
            "partially_implemented",
            "crates/persistence-sqlite/src/controller_keys/worker_auth/aws_fargate/execution.rs",
            "crates/persistence-sqlite/src/controller_keys/worker_auth/https/tests_secure_aws.rs",
            "SDK/TLS fixtures establish protocol behavior, but ControllerService does not compose paid scheduler/worker execution. This is a code-integration gap in addition to absent effective AWS isolation, bounded billing and live authorization.",
        ),
        7 | 19 => (
            "implemented_but_only_simulated",
            "crates/persistence-sqlite/src/controller_keys/github_app.rs",
            "crates/persistence-sqlite/src/controller_keys/github_app/tests/coding/execution.rs",
            "Trusted remote-only publication is exercised through private HTTP fixtures; no live installation or protected-repository certification.",
        ),
        8 | 9 => (
            "implemented_but_only_simulated",
            "crates/persistence-sqlite/src/finance.rs",
            "crates/persistence-sqlite/tests/finance.rs",
            "Integer ledger/admission and Owner workflows are implemented; actual provider stopping and charging bounds are unqualified.",
        ),
        27 => (
            "implemented_but_only_simulated",
            "crates/persistence-sqlite/src/controller_keys/human_auth/sessions/owner.rs",
            "crates/persistence-sqlite/src/controller_keys/human_auth/sessions/owner/tests.rs",
            "Exact-action Owner authority has real SQLite/TLS fixtures; complete installed browser approval acceptance is not demonstrated by the operator CLI prefix.",
        ),
        10 => (
            "implemented_but_externally_unqualified",
            "crates/persistence-sqlite/src/controller_keys/provider_accounts.rs",
            "crates/persistence-sqlite/src/controller_keys/provider_accounts/tests/multiple_accounts.rs",
            "Account isolation and API/CLI separation are tested; real account/hosted CLI entitlement and quota evidence is missing.",
        ),
        12 | 15 => (
            "implemented_but_only_simulated",
            "crates/persistence-sqlite/src/lib.rs",
            "crates/persistence-sqlite/tests/recovery.rs",
            "SQLite/WAL and crash fixtures are real; installed complete system restoration remains unverified.",
        ),
        13 | 14 => (
            "partially_implemented",
            "Cargo.toml",
            ".github/scripts/ci.py",
            "Repository checks cover source/doc invariants; this source inspection is not a final-head CI execution receipt.",
        ),
        21..=23 | 25 | 26 => (
            "partially_implemented",
            "crates/persistence-sqlite/src/controller_keys/human_auth.rs",
            "crates/persistence-sqlite/src/controller_keys/operations/tests/tls.rs",
            "Real HumanAuth/session/governance fixtures; installed full identity journey, genuine enrollment issuer and operational email are not certified.",
        ),
        24 => (
            "missing_implementation",
            "docs/decisions/0030-project-activity-audit-log.md",
            "crates/persistence-sqlite/src/controller_keys/human_auth/sessions/owner/panel.rs",
            "Bounded activity reads exist; approved independent audit archives/checkpoints, retention and staged storage migration are not implemented as a complete operational system.",
        ),
        29..=32 | 37 => (
            "partially_implemented",
            "apps/web/src/app/App.tsx",
            "apps/web/tests/panel.spec.ts",
            "Working four-file panel and browser fixtures; installed release journey, final icons, physical devices and manual screen-reader certification remain unverified.",
        ),
        33..=36 => (
            "partially_implemented",
            "crates/persistence-sqlite/src/controller_keys.rs",
            "crates/persistence-sqlite/tests/secret_broker.rs",
            "Protected key/Broker/backup recovery exists; general KEK rotation with retained historical backups and independent off-host custody remains incomplete.",
        ),
        _ => unreachable!("closed requirement catalogue"),
    }
}

fn rows(root: &Path, file: &str, prefix: &str, count: usize) -> Result<Vec<Row>> {
    let bytes = Tree::open(root)?.read(file, 512 * 1024)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "invalid specification")?;
    let mut rows = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split('|').map(str::trim).collect();
        if fields.len() < 4 || !fields[1].starts_with(prefix) {
            continue;
        }
        let id = fields[1];
        let n: usize = id
            .strip_prefix(prefix)
            .ok_or("invalid criterion")?
            .parse()
            .map_err(|_| "invalid criterion")?;
        if n == 0 || n > count || id != format!("{prefix}{n:03}") {
            return Err("unexpected specification identity".into());
        }
        let requirement = if prefix == "PR-" {
            n
        } else {
            fields
                .iter()
                .rev()
                .find_map(|f| {
                    f.find("PR-")
                        .and_then(|p| f.get(p + 3..p + 6))
                        .and_then(|v| v.parse().ok())
                })
                .unwrap_or(match n {
                    18 | 22 => 18,
                    19 => 19,
                    20 | 21 => 8,
                    36 => 13,
                    41..=48 => 22,
                    49..=61 => 23,
                    62..=112 => 24,
                    113..=144 => 25,
                    145..=149 => 10,
                    150..=152 => 28,
                    153..=164 => 29,
                    165..=167 => 33,
                    168..=170 => 34,
                    171..=173 => 35,
                    174..=177 => 36,
                    178..=184 => 37,
                    _ => 1,
                })
        };
        if !(1..=37).contains(&requirement) {
            return Err("unknown requirement reference".into());
        }
        let (mut classification, mut source, mut test_source, mut limitation) =
            mapping(requirement);
        if prefix == "AC-" {
            if n == 14 {
                source = "crates/persistence-sqlite/src/controller_keys/secret_broker.rs";
                test_source = "crates/persistence-sqlite/src/controller_keys/github_app/tests/coding/proxy_tests.rs";
            }
            if n == 150 {
                classification = "implemented_but_only_simulated";
                source = "crates/persistence-sqlite/src/orchestration/mod.rs";
                test_source = "crates/persistence-sqlite/tests/orchestration.rs";
                limitation = "Real SQLite R06/R08/R09 synthetic dependency-aware component evidence; no live provider or installed scheduler composition is claimed.";
            }
            if matches!(n, 44 | 48 | 152) {
                classification = "explicitly_out_of_scope_under_approved_decision";
                limitation = "Federation/mobile voice are not mandatory v0.1; later remote providers follow AWS under ADRs 0033/0040/0041.";
            }
            if (67..=112).contains(&n) {
                classification = "missing_implementation";
                limitation = "Approved activity archive/checkpoint, storage migration and specified retry evidence is not provided by a SQLite snapshot or a task retry fixture.";
            }
            if n == 177 {
                classification = "missing_implementation";
                limitation = "General KEK rotation and retained-version recovery has no complete operational implementation/test.";
            }
            if n == 151 {
                classification = "partially_implemented";
                limitation = "Installed controller scheduler/worker composition is missing, and no real AWS operation is authorized. Both implementation and external qualification remain required; fixtures cannot establish first-live-provider acceptance.";
            }
        }
        if !root.join(source).is_file() || !root.join(test_source).is_file() {
            return Err(format!("coverage source reference missing for {id}"));
        }
        if rows
            .insert(
                n,
                Row {
                    id: id.into(),
                    specification: file.into(),
                    text: fields[2..fields.len() - 1].join(" | "),
                    classification,
                    source,
                    test_source,
                    evidence_level: "source_inspection_not_a_test_execution_receipt",
                    limitation,
                },
            )
            .is_some()
        {
            return Err("duplicate criterion".into());
        }
    }
    if rows.len() != count || !(1..=count).all(|n| rows.contains_key(&n)) {
        return Err("incomplete approved specification coverage".into());
    }
    Ok(rows.into_values().collect())
}

pub(crate) fn generate(root: &Path) -> Result<serde_json::Value> {
    let mut all = rows(root, "docs/product/requirements.md", "PR-", 37)?;
    all.extend(rows(
        root,
        "docs/product/acceptance-criteria.md",
        "AC-",
        184,
    )?);
    let specification_digest =
        digest(&Tree::open(root)?.read("docs/product/acceptance-criteria.md", 512 * 1024)?);
    Ok(
        serde_json::json!({"version":1,"criteriaSha256":specification_digest,"requirements":37,"acceptanceCriteria":184,"rows":all,"note":"Every row is a source assessment. Test-source links locate relevant fixtures and are not claims that the complete criterion passed. No missing mandatory implementation is waived."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_approved_scope_is_exhaustive_and_missing_code_remains_visible() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let value = generate(root).unwrap();
        assert_eq!(value["rows"].as_array().unwrap().len(), 221);
        for id in ["PR-001", "PR-024", "AC-177"] {
            let row = value["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == id)
                .unwrap();
            assert_ne!(
                row["classification"],
                "implemented_and_independently_tested"
            );
        }
    }
}
