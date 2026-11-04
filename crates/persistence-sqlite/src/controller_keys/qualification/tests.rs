use super::*;
#[test]
fn restricted_configuration_is_inspected_without_granting_execution() {
    let c = configuration();
    assert!(c.inspect().is_ok());
}
pub(super) fn configuration() -> IsolationConfiguration {
    use serde_json::json;
    let mut coding = crate::controller_keys::worker_auth::aws_fargate::fixture_workload();
    coding.coding_isolation =
        Some(crate::controller_keys::worker_auth::aws_fargate::CodingIsolation::CredentialProxyV1);
    coding.command = vec!["rolvya-coding-worker".into(), "--provider-proxy".into()];
    let mut verifier = coding.clone();
    verifier.coding_isolation = None;
    verifier.execution_role = crate::controller_keys::worker_auth::aws_fargate::RoleArn::parse(
        "arn:aws:iam::123456789012:role/verifier-execution",
    )
    .unwrap();
    verifier.command = vec![
        "/opt/rolvya/verifier".into(),
        "--policy".into(),
        "a".repeat(64),
        "--executables".into(),
        "c".repeat(64),
    ];
    let role = format!("{}-verifier", coding.task_role.as_str());
    verifier.task_role =
        crate::controller_keys::worker_auth::aws_fargate::RoleArn::parse(&role).unwrap();
    verifier.task_definition =
        crate::controller_keys::worker_auth::aws_fargate::TaskDefinitionArn::parse(&format!(
            "{}2",
            coding.task_definition.as_str()
        ))
        .unwrap();
    verifier.image_digest = format!("sha256:{}", "b".repeat(64));
    verifier.image = format!("fixture@{}", verifier.image_digest);
    let secret = |s| crate::controller_keys::worker_auth::aws_fargate::SecretArn::parse(s).unwrap();
    let mut config = IsolationConfiguration {
        version: 1,
        coding: coding.clone(),
        verifier,
        coding_secret: secret(&format!(
            "arn:aws:secretsmanager:{}:{}:secret:coding-abcdef",
            coding.cluster.region(),
            coding.cluster.account().as_str()
        )),
        verifier_secret: secret(&format!(
            "arn:aws:secretsmanager:{}:{}:secret:verifier-abcdef",
            coding.cluster.region(),
            coding.cluster.account().as_str()
        )),
        verifier_policy_sha256: "a".repeat(64),
        verifier_executables_sha256: "c".repeat(64),
        coding_identity_policy: json!({}),
        verifier_identity_policy: json!({}),
        controller_policy: json!({}),
        coding_trust: json!({}),
        verifier_trust: json!({}),
        network: NetworkConfiguration {
            controller_origin: "https://controller.example.org".into(),
            artifact_origin: "https://artifacts.example.org".into(),
            vpc: coding.vpc,
            subnets: coding.subnets,
            groups: coding.security_groups,
            ingress: vec![],
            egress: vec![],
            endpoints: vec![],
            dns_support: true,
            dns_hostnames: true,
            public_ip: false,
        },
    };
    let policies = config.required_policies();
    config.coding_identity_policy = policies.coding;
    config.verifier_identity_policy = policies.verifier;
    config.controller_policy = policies.controller;
    config.coding_trust = policies.trust.clone();
    config.verifier_trust = policies.trust;
    for (i, purpose) in [
        EndpointPurpose::Controller,
        EndpointPurpose::SecretsManager,
        EndpointPurpose::EcrApi,
        EndpointPurpose::EcrRegistry,
        EndpointPurpose::Logs,
        EndpointPurpose::Artifacts,
    ]
    .into_iter()
    .enumerate()
    {
        let group = format!("sg-{:08x}", i + 20);
        let hostname = match purpose {
            EndpointPurpose::Controller => "controller.example.org".into(),
            EndpointPurpose::Artifacts => "artifacts.example.org".into(),
            EndpointPurpose::SecretsManager => format!(
                "secretsmanager.{}.amazonaws.com",
                config.coding.cluster.region()
            ),
            EndpointPurpose::EcrApi => {
                format!("api.ecr.{}.amazonaws.com", config.coding.cluster.region())
            }
            EndpointPurpose::EcrRegistry => format!(
                "{}.dkr.ecr.{}.amazonaws.com",
                config.coding.cluster.account().as_str(),
                config.coding.cluster.region()
            ),
            EndpointPurpose::Logs => {
                format!("logs.{}.amazonaws.com", config.coding.cluster.region())
            }
            _ => unreachable!(),
        };
        config.network.egress.push(NetworkRule {
            protocol: "tcp".into(),
            from: 443,
            to: 443,
            destination_group: group.clone(),
        });
        config.network.endpoints.push(Endpoint {
            purpose,
            group,
            hostname,
        });
    }
    config
}

#[test]
fn task_identity_policy_rejects_cross_attempt_reads_and_privilege_expansion() {
    use serde_json::json;
    for action in [
        "secretsmanager:*",
        "sts:AssumeRole",
        "iam:PutRolePolicy",
        "ecs:RunTask",
        "s3:GetObject",
    ] {
        let mut c = configuration();
        c.coding_identity_policy["Statement"][0]["Action"] = json!(action);
        assert_eq!(c.inspect().unwrap_err(), E::Scope);
    }
    let mut c = configuration();
    c.coding_identity_policy["Statement"][0]["Resource"] = json!(c.verifier_secret);
    assert!(c.inspect().is_err());
}
#[test]
fn passrole_scope_and_service_constraints_cannot_be_broadened() {
    use serde_json::json;
    let mut c = configuration();
    c.controller_policy["Statement"][1]["Resource"] = json!("*");
    assert!(c.inspect().is_err());
    let mut c = configuration();
    c.controller_policy["Statement"][1]["Condition"] = json!({});
    assert!(c.inspect().is_err());
    let mut c = configuration();
    c.controller_policy["Statement"][0]["Condition"] = json!({});
    assert!(c.inspect().is_err());
}
#[test]
fn task_role_trust_rejects_other_accounts_and_unconstrained_principals() {
    use serde_json::json;
    for (key, value) in [
        ("Principal", json!("*")),
        ("Condition", json!({})),
        ("Action", json!("sts:*")),
    ] {
        let mut c = configuration();
        c.coding_trust["Statement"][0][key] = value;
        assert!(c.inspect().is_err());
    }
}
#[test]
fn independent_verifier_cannot_share_coding_role_image_or_bootstrap() {
    for n in 0..4 {
        let mut c = configuration();
        match n {
            0 => c.verifier.task_role = c.coding.task_role.clone(),
            1 => c.verifier.image_digest = c.coding.image_digest.clone(),
            2 => c.verifier_secret = c.coding_secret.clone(),
            _ => c.verifier.task_definition = c.coding.task_definition.clone(),
        };
        assert!(c.inspect().is_err());
    }
}
#[test]
fn unsafe_network_rules_and_inventory_substitution_fail_closed() {
    for n in 0..8 {
        let mut c = configuration();
        match n {
            0 => c.network.public_ip = true,
            1 => c.network.ingress.push(c.network.egress[0].clone()),
            2 => c.network.egress[0].destination_group = "0.0.0.0/0".into(),
            3 => c.network.egress[0].from = 0,
            4 => c.network.dns_support = false,
            5 => c.network.vpc = "vpc-22222222".into(),
            6 => c.network.subnets.clear(),
            _ => c.network.groups.clear(),
        };
        assert!(c.inspect().is_err());
    }
}
#[test]
fn endpoint_host_substitution_and_incomplete_egress_do_not_pass_inspection() {
    let mut c = configuration();
    c.network.endpoints[1].hostname = "attacker.example.org".into();
    assert!(c.inspect().is_err());
    let mut c = configuration();
    c.network.egress.pop();
    assert!(c.inspect().is_err());
    let mut c = configuration();
    c.network.endpoints[0].hostname = "https://credential@example.org".into();
    assert!(c.inspect().is_err());
}
#[test]
fn malformed_qualification_versions_and_unknown_fields_are_rejected() {
    let mut c = configuration();
    c.version = 2;
    assert!(c.inspect().is_err());
    let mut raw = serde_json::to_value(configuration()).unwrap();
    raw["enabled"] = serde_json::json!(true);
    assert!(serde_json::from_value::<IsolationConfiguration>(raw).is_err());
}
fn model_binding(
    provider: &str,
    cli: bool,
) -> crate::controller_keys::provider_accounts::CodingModelBinding {
    use rolvya_domain::plan::AccessMode;
    crate::controller_keys::provider_accounts::CodingModelBinding {
        connection: format!("{provider}-{}", if cli { "cli" } else { "api" }),
        provider: provider.into(),
        account: format!("{provider}-account"),
        mode: if cli {
            AccessMode::CliSession
        } else {
            AccessMode::ApiKey
        },
        integration: match (provider, cli) {
            ("openai", false) => "openai-responses-v1",
            ("anthropic", false) => "anthropic-messages-v1",
            ("openai", true) => "codex-exec-json-v1",
            _ => "claude-print-json-v1",
        }
        .into(),
        credential_connection: format!("{provider}-secret"),
        credential_record: "model-api".into(),
        credential_purpose: "model-api".into(),
        connection_version: 1,
        grant_version: 1,
        credential_version: 1,
    }
}
fn account(provider: &str, cli: bool) -> AccountObservation {
    AccountObservation {
        version: 1,
        binding: model_binding(provider, cli),
        model: "approved-model".into(),
        account_identity_sha256: "a".repeat(64),
        permission_sha256: "b".repeat(64),
        policy_source: if provider == "openai" {
            "https://developers.openai.com/codex/auth/"
        } else {
            "https://code.claude.com/docs/en/legal-and-compliance"
        }
        .into(),
        policy_revision_sha256: "c".repeat(64),
        observed_at: 100,
        expires_at: 200,
        quota_remaining: 1,
        concurrent_limit: 1,
        cli: cli.then(|| CliQualification {
            official_version: "reviewed-version".into(),
            executable_sha256: "d".repeat(64),
            authentication: CliAuthentication::OfficialRemoteLogin,
            hosted_permission_sha256: "e".repeat(64),
            account_home_sha256: format!("{:x}", sha2::Sha256::digest(provider.as_bytes())),
            refresh_serialized: true,
        }),
    }
}
#[test]
fn four_account_declarations_preserve_provider_mode_and_independent_home() {
    let a = account("openai", false);
    let b = account("anthropic", false);
    let c = account("openai", true);
    let d = account("anthropic", true);
    for a in [&a, &b, &c, &d] {
        assert!(a.inspect(&a.binding, "approved-model", 100).is_ok());
    }
    assert_ne!(
        c.cli.unwrap().account_home_sha256,
        d.cli.unwrap().account_home_sha256
    );
    assert!(a.inspect(&b.binding, "approved-model", 100).is_err());
}
#[test]
fn account_substitution_revocation_versions_expiry_and_wrong_models_deny() {
    let a = account("openai", false);
    for n in 0..5 {
        let mut binding = a.binding.clone();
        match n {
            0 => binding.credential_version += 1,
            1 => binding.grant_version += 1,
            2 => binding.account = "other".into(),
            3 => binding.connection = "other".into(),
            _ => binding.connection_version += 1,
        };
        assert!(a.inspect(&binding, "approved-model", 100).is_err());
    }
    assert!(a.inspect(&a.binding, "other-model", 100).is_err());
    assert!(a.inspect(&a.binding, "approved-model", 200).is_err());
}
#[test]
fn missing_hosted_cli_permission_refresh_or_quota_is_unavailable() {
    for n in 0..4 {
        let mut a = account("anthropic", true);
        match n {
            0 => a.cli.as_mut().unwrap().hosted_permission_sha256.clear(),
            1 => a.cli.as_mut().unwrap().refresh_serialized = false,
            2 => a.quota_remaining = 0,
            _ => a.cli = None,
        };
        assert!(a.inspect(&a.binding, "approved-model", 100).is_err());
    }
}
#[test]
fn public_documentation_reference_does_not_allow_arbitrary_or_secret_bearing_urls() {
    for url in [
        "http://code.claude.com/docs/en/authentication",
        "https://code.claude.com.attacker.org/",
        "https://credential@code.claude.com/",
        "https://code.claude.com/?api_key=value",
    ] {
        let mut a = account("anthropic", true);
        a.policy_source = url.into();
        assert!(a.inspect(&a.binding, "approved-model", 100).is_err());
    }
}
fn calculation() -> ExposureCalculation {
    use crate::controller_keys::worker_auth::aws_fargate::execution::{ExposureEnvelope, UnitRate};
    use crate::resilience::synthetic::{costs, e, money};
    let r = UnitRate {
        charge: money(0),
        units: 1,
    };
    let env = ExposureEnvelope {
        format: 1,
        evidence: e("private-price-fixture"),
        revision: 1,
        collected_at: 100,
        valid_until: 200,
        runtime_seconds: 60,
        provisioning_seconds: 10,
        termination_seconds: 10,
        cpu_unit_second: r,
        memory_mib_second: r,
        extra_gib_second: r,
        fixed: costs(),
    };
    ExposureCalculation {
        version: 1,
        coding: env.clone(),
        verifier: env,
        api: ApiExposure::OpenAi {
            model: "approved-model".into(),
            uncached_input: TokenCharge {
                tokens: 10,
                rate: r,
            },
            cached_input: TokenCharge { tokens: 0, rate: r },
            output_including_reasoning: TokenCharge {
                tokens: 10,
                rate: r,
            },
        },
        calls: 1,
        additional_ai: Some(money(0)),
        price_source_sha256: "a".repeat(64),
        region: configuration().coding.cluster.region().into(),
        observed_at: 100,
        expires_at: 200,
    }
}
#[test]
fn complete_coding_and_verification_estimate_never_becomes_enforcement_proof() {
    let c = configuration();
    let result = calculation().estimate(&c.coding, &c.verifier, 100).unwrap();
    assert!(!result.independently_enforced);
    require_coverage(result.estimated, result.estimated).unwrap();
}
#[test]
fn unknown_price_category_and_stale_region_price_are_not_zero() {
    let c = configuration();
    for n in 0..4 {
        let mut p = calculation();
        match n {
            0 => p.additional_ai = None,
            1 => p.region = "other-region".into(),
            2 => p.expires_at = 100,
            _ => p.price_source_sha256.clear(),
        };
        assert!(p.estimate(&c.coding, &c.verifier, 100).is_err());
    }
}
#[test]
fn cost_overflow_and_uncovered_individual_categories_deny() {
    use crate::resilience::synthetic::money;
    let c = configuration();
    let mut p = calculation();
    if let ApiExposure::OpenAi { uncached_input, .. } = &mut p.api {
        uncached_input.tokens = u64::MAX;
        uncached_input.rate.charge = money(i64::MAX);
    }
    assert!(p.estimate(&c.coding, &c.verifier, 100).is_err());
    let exposure = calculation()
        .estimate(&c.coding, &c.verifier, 100)
        .unwrap()
        .estimated;
    let mut reserved = exposure;
    reserved.ai_api = money(0);
    reserved.compute = money(100000);
    assert!(require_coverage(exposure, reserved).is_err());
}
#[test]
fn review_missing_coding_profile_and_shared_execution_role_are_not_qualified_configuration() {
    let mut c = configuration();
    c.coding.coding_isolation = None;
    assert!(
        c.inspect().is_err(),
        "a legacy workload cannot demonstrate coding confinement"
    );
    let mut c = configuration();
    c.verifier.execution_role = c.coding.execution_role.clone();
    assert!(
        c.inspect().is_err(),
        "independent verification cannot share the coding execution role"
    );
}
#[test]
fn review_aws_endpoint_hostnames_cannot_be_replaced_by_arbitrary_dns_names() {
    for purpose in [
        EndpointPurpose::EcrApi,
        EndpointPurpose::EcrRegistry,
        EndpointPurpose::Logs,
    ] {
        let mut c = configuration();
        c.network
            .endpoints
            .iter_mut()
            .find(|e| e.purpose == purpose)
            .unwrap()
            .hostname = "exfiltration.example.org".into();
        assert!(
            c.inspect().is_err(),
            "AWS service identity must be constrained"
        );
    }
}
#[test]
fn review_exposure_commitment_binds_both_immutable_workloads() {
    let c = configuration();
    let calculation = calculation();
    let a = calculation.estimate(&c.coding, &c.verifier, 100).unwrap();
    let mut changed = c.verifier.clone();
    changed.task_definition =
        crate::controller_keys::worker_auth::aws_fargate::TaskDefinitionArn::parse(
            "arn:aws:ecs:eu-central-1:123456789012:task-definition/other-verifier:1",
        )
        .unwrap();
    let b = calculation.estimate(&c.coding, &changed, 100).unwrap();
    assert_ne!(
        a.commitment, b.commitment,
        "an estimate commitment must identify its workloads"
    );
}
