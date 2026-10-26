//! Cross-layer R11 regressions reproduced by independent read-only reviewers.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct UncertainAbsent {
    provider: SyntheticProvider,
    after_launch: bool,
    launches: AtomicUsize,
}
impl UncertainAbsent {
    async fn observation(
        &self,
        command: &LaunchCommand,
    ) -> Result<ProviderObservation, WorkerError> {
        let mut observation = self.provider.inspect(command).await?;
        observation.unknown_exposure = true;
        observation.evidence = e("r11-uncertain-absence");
        Ok(observation)
    }
}
impl LaunchAdapter for UncertainAbsent {
    async fn inspect(&self, command: &LaunchCommand) -> Result<ProviderObservation, WorkerError> {
        if self.after_launch {
            self.provider.inspect(command).await
        } else {
            self.observation(command).await
        }
    }
    async fn launch(
        &self,
        authorization: &LaunchAuthorization,
        _: &impl Clock,
    ) -> Result<LaunchResult, WorkerError> {
        self.launches.fetch_add(1, Ordering::SeqCst);
        // A missing provider record is not evidence that an in-flight request or
        // its billing did not take effect. This adapter explicitly reports risk.
        Ok(LaunchResult::Observed(
            self.observation(authorization.command()).await?,
        ))
    }
}

#[tokio::test]
async fn uncertain_absence_before_or_after_launch_marker_blocks_and_survives_restart() {
    for after_launch in [false, true] {
        let (f, clock, lease) = admitted().await;
        let d = OutboxDispatcher::new(f.store.clone());
        let ticket = d
            .claim(
                &lease,
                &owner(),
                &SyntheticScheduler,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        let adapter = UncertainAbsent {
            provider: SyntheticProvider::new(f.store.clone()),
            after_launch,
            launches: AtomicUsize::new(0),
        };
        let result = d
            .dispatch(
                &ticket,
                &adapter,
                &SyntheticScheduler,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            adapter.launches.load(Ordering::SeqCst),
            usize::from(after_launch)
        );
        assert_eq!(result, DispatchState::Blocked);
        let before = f
            .store
            .financial_snapshot(&lease.task.plan.project_id)
            .await
            .unwrap();
        assert_eq!(before.uncertain_liabilities, 1);
        assert_eq!(before.held, costs());
        assert_eq!(before.ceiling, money(10000));
        assert_eq!(rows(&f, "synthetic_operations").await, 0);
        f.store.close().await;
        let store = PlanStore::open(&f.path).await.unwrap();
        store.check_integrity().await.unwrap();
        let adapter = UncertainAbsent {
            provider: SyntheticProvider::new(store.clone()),
            after_launch,
            launches: AtomicUsize::new(0),
        };
        let result = OutboxDispatcher::new(store.clone())
            .dispatch(
                &ticket,
                &adapter,
                &SyntheticScheduler,
                &clock,
                Limits::default(),
            )
            .await;
        assert!(!matches!(
            result,
            Ok(DispatchState::Active | DispatchState::Launching)
        ));
        assert_eq!(adapter.launches.load(Ordering::SeqCst), 0);
        assert_eq!(
            store
                .financial_snapshot(&lease.task.plan.project_id)
                .await
                .unwrap(),
            before
        );
    }
}

#[tokio::test]
async fn later_uncertainty_preserves_all_provisional_categories_and_blocks_sibling_admission() {
    let s = setup_admitted(admitted_in(base(true).await, T + 3600).await).await;
    executing(&s).await;
    let p = &s.lease.task.plan.project_id;
    s.f.store
        .apply_financial(
            &FinancialRequest {
                project: p.clone(),
                request_id: RequestId::new("r11-known-cost").unwrap(),
                actor: owner(),
                reason: "Synthetic known additional costs before later provider uncertainty".into(),
                command: FinancialCommand::Report {
                    liability: LiabilityId::new(s.lease.attempt_id.as_str()).unwrap(),
                    expected_revision: 1,
                    unreserved: costs(),
                    uncertain: false,
                    evidence: e("r11-known-cost-e"),
                },
            },
            &SyntheticFinance,
        )
        .await
        .unwrap();
    s.provider
        .observe(
            &OperationKey {
                project: p.clone(),
                attempt: s.lease.attempt_id.clone(),
            },
            1,
            ProviderState::Active,
            true,
            &e("r11-later-unknown-e"),
        )
        .await
        .unwrap();
    assert_eq!(
        s.dispatcher
            .reconcile(
                &s.lease,
                &s.provider,
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap(),
        DispatchState::Blocked
    );
    let before = s.f.store.financial_snapshot(p).await.unwrap();
    assert_eq!(before.uncertain_liabilities, 1);
    assert_eq!(before.provisional, costs());
    assert_eq!(before.held, costs());
    assert_eq!(before.ceiling, money(10000));
    assert!(
        send(&s, &message(&s, 4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
    let mut sibling = original_request(&s.f, &s.lease).await;
    sibling.task.task_id = TaskId::new("task-b").unwrap();
    sibling.request_id = RequestId::new("r11-sibling").unwrap();
    sibling.permit.id = e("r11-sibling-capability");
    assert!(matches!(
        s.f.store
            .admit(&sibling, &SyntheticScheduler, &s.clock, Limits::default())
            .await,
        Err(AdmissionError::Finance(FinanceError::UncertainExposure))
    ));
    assert_eq!(rows(&s.f, "task_attempts").await, 1);
    assert_eq!(rows(&s.f, "dispatch_outbox").await, 1);
    s.f.store.close().await;
    let store = PlanStore::open(&s.f.path).await.unwrap();
    assert_eq!(store.financial_snapshot(p).await.unwrap(), before);
    let d = OutboxDispatcher::new(store.clone());
    assert_eq!(
        d.reconcile(
            &s.lease,
            &SyntheticProvider::new(store.clone()),
            &SyntheticScheduler,
            &s.clock,
            Limits::default()
        )
        .await
        .unwrap(),
        DispatchState::Blocked
    );
    assert_eq!(store.financial_snapshot(p).await.unwrap(), before);
}
