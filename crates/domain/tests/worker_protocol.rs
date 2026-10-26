use rolvya_domain::worker::*;
#[test]
fn unsupported_protocol_versions_fail_closed() {
    assert_eq!(validate_version(2), Err(ProtocolError::UnsupportedVersion));
    assert!(validate_version(PROTOCOL_VERSION).is_ok());
}
use rolvya_domain::{ids::*, plan::*};
fn binding() -> WorkerBinding {
    WorkerBinding {
        task: TaskRef {
            plan: PlanKey {
                project_id: ProjectId::new("project").unwrap(),
                plan_id: PlanId::new("plan").unwrap(),
                version: PlanVersion::new(1).unwrap(),
            },
            task_id: TaskId::new("task").unwrap(),
        },
        attempt_id: AttemptId::new("attempt").unwrap(),
        worker_id: WorkerId::new("worker").unwrap(),
        epoch: 1,
        plan_hash: Digest::new("a".repeat(64)).unwrap(),
        context_hash: Digest::new("b".repeat(64)).unwrap(),
    }
}
fn capabilities() -> DeclaredCapabilities {
    DeclaredCapabilities {
        models: vec![ModelName::new("synthetic/model").unwrap()],
        efforts: vec![Effort::Medium],
        access_modes: vec![AccessMode::ApiKey],
        cpu_millis: 1000,
        memory_mib: 512,
        evidence: Some(CapabilityRef::new("synthetic-capability").unwrap()),
    }
}
fn frame(body: WorkerBody) -> WorkerMessage {
    WorkerMessage {
        protocol_version: PROTOCOL_VERSION,
        message_id: MessageId::new("message").unwrap(),
        binding: binding(),
        sequence: 1,
        body,
    }
}
#[test]
fn every_worker_lifecycle_contract_round_trips_and_validates() {
    for body in [
        WorkerBody::Register {
            capabilities: capabilities(),
        },
        WorkerBody::Claim {},
        WorkerBody::AcceptLease {
            offer_id: MessageId::new("offer").unwrap(),
        },
        WorkerBody::Heartbeat {},
        WorkerBody::Progress {
            completed: 1,
            total: 2,
            code: ProgressCode::Executing,
        },
        WorkerBody::SubmitResult {
            outcome: ExecutionOutcome::Succeeded {
                manifest_sha256: Digest::new("c".repeat(64)).unwrap(),
            },
        },
        WorkerBody::SubmitResult {
            outcome: ExecutionOutcome::Failed {
                class: FailureClass::Transient,
            },
        },
        WorkerBody::GetStopInstruction {},
        WorkerBody::StopAcknowledged {
            instruction_id: MessageId::new("stop").unwrap(),
        },
    ] {
        let original = frame(body);
        original.validate().unwrap();
        let decoded: WorkerMessage =
            serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
        assert_eq!(original, decoded);
        decoded.validate().unwrap();
    }
}
#[test]
fn strict_protocol_schema_has_no_budget_graph_git_or_acceptance_authority() {
    let base = serde_json::to_value(frame(WorkerBody::Heartbeat {})).unwrap();
    for forbidden in [
        "approve_budget",
        "create_task",
        "change_dependencies",
        "publish_git",
        "validate_result",
        "issue_token",
    ] {
        let mut data = base.clone();
        data["body"] = serde_json::json!({"kind":forbidden,"body":{}});
        assert!(serde_json::from_value::<WorkerMessage>(data).is_err());
    }
    for field in [
        "approved_ceiling",
        "dependencies",
        "credentials",
        "accepted",
    ] {
        let mut data = base.clone();
        data[field] = serde_json::json!(true);
        assert!(serde_json::from_value::<WorkerMessage>(data).is_err());
    }
    let mut data = base;
    data["body"] = serde_json::json!({"kind":"submit_result","body":{"outcome":{"kind":"succeeded","body":{"manifest_sha256":"c".repeat(64),"accepted":true}}}});
    assert!(serde_json::from_value::<WorkerMessage>(data).is_err());
}
#[test]
fn identifiers_digests_and_models_cannot_bypass_validated_deserialization() {
    for raw in ["", "../other", "UPPER", "worker\nrole", &"a".repeat(65)] {
        assert!(serde_json::from_value::<WorkerId>(serde_json::json!(raw)).is_err());
        assert!(serde_json::from_value::<MessageId>(serde_json::json!(raw)).is_err());
        assert!(serde_json::from_value::<CapabilityRef>(serde_json::json!(raw)).is_err());
    }
    for raw in ["", &"a".repeat(63), &"A".repeat(64), &"g".repeat(64)] {
        assert!(serde_json::from_value::<Digest>(serde_json::json!(raw)).is_err());
    }
    for raw in ["", "model name", "model\nsecret", &"x".repeat(129)] {
        assert!(ModelName::new(raw).is_err());
    }
    assert_eq!(
        format!("{:?}", ModelName::new("synthetic/model").unwrap()),
        "ModelName(..)"
    );
}
#[test]
fn invalid_scope_sequence_and_progress_are_rejected() {
    let mut message = frame(WorkerBody::Heartbeat {});
    message.sequence = 0;
    assert!(message.validate().is_err());
    message.sequence = 1;
    message.binding.epoch = 0;
    assert!(message.validate().is_err());
    message.binding.epoch = u64::MAX;
    assert!(message.validate().is_err());
    message.binding.epoch = 1;
    message.binding.task.plan.version = PlanVersion::new(u64::MAX).unwrap();
    assert!(message.validate().is_err());
    for (completed, total) in [(0, 0), (2, 1)] {
        assert!(
            frame(WorkerBody::Progress {
                completed,
                total,
                code: ProgressCode::Executing
            })
            .validate()
            .is_err()
        );
    }
}
#[test]
fn declared_capabilities_are_bounded_and_must_cover_the_profile() {
    let valid = capabilities();
    let profile = ExecutionProfile {
        model: valid.models[0].clone(),
        effort: Effort::Medium,
        access_mode: AccessMode::ApiKey,
        resources: ResourceRequirements {
            cpu_millis: 1000,
            memory_mib: 512,
            timeout_seconds: 60,
        },
    };
    valid.validate().unwrap();
    assert!(valid.supports(&profile));
    let mut invalid = vec![valid.clone(); 7];
    invalid[0].models.clear();
    invalid[1].models = vec![valid.models[0].clone(); MAX_MODELS + 1];
    invalid[2].models.push(valid.models[0].clone());
    invalid[3].efforts.clear();
    invalid[4].access_modes.push(AccessMode::ApiKey);
    invalid[5].cpu_millis = 0;
    invalid[6].memory_mib = 0;
    for caps in invalid {
        assert!(caps.validate().is_err());
    }
    let mut smaller = valid.clone();
    smaller.memory_mib = 511;
    assert!(!smaller.supports(&profile));
    let mut other = profile;
    other.model = ModelName::new("unsupported/model").unwrap();
    assert!(!valid.supports(&other));
}
#[test]
fn approved_worker_timings_are_separate_and_not_credentials() {
    assert_eq!(
        WorkerTiming::default(),
        WorkerTiming {
            access_token_ttl_seconds: 900,
            heartbeat_interval_seconds: 30,
            lease_duration_seconds: 90
        }
    );
}
#[test]
fn controller_responses_are_versioned_scoped_and_reject_invalid_assignment_metadata() {
    let message_id = MessageId::new("message").unwrap();
    let mut reply = ControllerMessage {
        protocol_version: PROTOCOL_VERSION,
        response_to: message_id.clone(),
        binding: binding(),
        body: ControllerBody::Acknowledged {
            receipt: WorkerReceipt {
                message_id,
                sequence: 1,
                observed_at: 100,
                kind: ReceiptKind::HeartbeatObserved,
            },
        },
    };
    reply.validate().unwrap();
    reply.protocol_version = 2;
    assert_eq!(reply.validate(), Err(ProtocolError::UnsupportedVersion));
    reply.protocol_version = PROTOCOL_VERSION;
    reply.body = ControllerBody::Stop {
        instruction: StopInstruction {
            instruction_id: MessageId::new("stop").unwrap(),
            reason: StopReason::Fenced,
            next_epoch: Some(1),
        },
    };
    assert!(reply.validate().is_err());
}
