use super::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityImage {
    pub fingerprint: String,
    pub nodes: i64,
    pub edges: i64,
    pub ceiling: i64,
    pub positions: [i128; 12],
    pub uncertain: i64,
    pub attempts: i64,
    pub outbox: i64,
    pub effects: i64,
    pub fences: Vec<(String, i64)>,
    pub runtime: Vec<(String, String)>,
    pub accepted: Vec<(String, String)>,
}
impl AuthorityImage {
    pub async fn read(store: &PlanStore, key: &PlanKey) -> Self {
        store.check_integrity().await.unwrap();
        let plan = store
            .load_active(&key.project_id, &Authority::default(), Limits::default())
            .await
            .unwrap();
        assert_eq!(plan.content.key, *key);
        let nodes = sqlx::query_scalar(
            "SELECT count(*) FROM tasks WHERE project_id=? AND plan_id=? AND version=?",
        )
        .bind(key.project_id.as_str())
        .bind(key.plan_id.as_str())
        .bind(key.version.get() as i64)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        let edges = sqlx::query_scalar(
            "SELECT count(*) FROM task_dependencies WHERE project_id=? AND plan_id=? AND version=?",
        )
        .bind(key.project_id.as_str())
        .bind(key.plan_id.as_str())
        .bind(key.version.get() as i64)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        let mut tx = store.pool.begin().await.unwrap();
        let ceiling:i64=sqlx::query_scalar("SELECT new_ceiling FROM budget_approvals WHERE project_id=? ORDER BY version DESC LIMIT 1").bind(key.project_id.as_str()).fetch_one(&mut *tx).await.unwrap();
        let rows=sqlx::query("SELECT p.category,p.settled_minor,p.held_minor,p.provisional_minor FROM liability_revisions r JOIN financial_positions p ON p.event_sequence=r.event_sequence WHERE r.project_id=? AND NOT EXISTS(SELECT 1 FROM liability_revisions n WHERE n.project_id=r.project_id AND n.liability_id=r.liability_id AND n.version>r.version)").bind(key.project_id.as_str()).fetch_all(&mut *tx).await.unwrap();
        let mut positions = [0i128; 12];
        for r in rows {
            let cat = match r.get::<String, _>(0).as_str() {
                "ai_api" => 0,
                "compute" => 1,
                "storage" => 2,
                "transfer" => 3,
                _ => panic!("unknown category"),
            };
            for balance in 0..3 {
                positions[balance * 4 + cat] += i128::from(r.get::<i64, _>(balance + 1));
            }
        }
        let unknown:i64=sqlx::query_scalar("SELECT count(*) FROM liability_revisions r WHERE r.project_id=? AND r.uncertain=1 AND NOT EXISTS(SELECT 1 FROM liability_revisions n WHERE n.project_id=r.project_id AND n.liability_id=r.liability_id AND n.version>r.version)").bind(key.project_id.as_str()).fetch_one(&mut *tx).await.unwrap();
        let partial:i64=sqlx::query_scalar("SELECT count(*) FROM liability_revisions r WHERE r.project_id=? AND (SELECT count(*) FROM financial_positions p WHERE p.event_sequence=r.event_sequence)<>4").bind(key.project_id.as_str()).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(partial, 0);
        let attempts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM task_attempts WHERE project_id=?")
                .bind(key.project_id.as_str())
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        let outbox: i64 =
            sqlx::query_scalar("SELECT count(*) FROM dispatch_outbox WHERE project_id=?")
                .bind(key.project_id.as_str())
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(attempts, outbox);
        let unreserved:i64=sqlx::query_scalar("SELECT count(*) FROM task_attempts a LEFT JOIN financial_events e ON e.sequence=a.financial_event AND e.project_id=a.project_id AND e.kind='reserve' LEFT JOIN liability_revisions l ON l.event_sequence=e.sequence AND l.liability_id=a.attempt_id WHERE a.project_id=? AND l.event_sequence IS NULL").bind(key.project_id.as_str()).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(unreserved, 0);
        let parents:i64=sqlx::query_scalar("SELECT count(*) FROM task_attempts a JOIN tasks t USING(project_id,plan_id,version,task_id) WHERE a.project_id=? AND t.kind='group'").bind(key.project_id.as_str()).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(parents, 0);
        let effects: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM synthetic_operations WHERE project_id=? AND state<>'rejected'",
        )
        .bind(key.project_id.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert!(effects <= attempts);
        let fences = sqlx::query_as(
            "SELECT task_id,epoch FROM task_fences WHERE project_id=? ORDER BY task_id",
        )
        .bind(key.project_id.as_str())
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        let accepted=sqlx::query_as("SELECT task_id,manifest_sha256 FROM task_acceptances WHERE project_id=? ORDER BY task_id").bind(key.project_id.as_str()).fetch_all(&mut *tx).await.unwrap();
        let runtime = sqlx::query_as(
            "SELECT task_id,state FROM task_runtime WHERE project_id=? ORDER BY task_id",
        )
        .bind(key.project_id.as_str())
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        let unvalidated: i64 = sqlx::query_scalar("SELECT count(*) FROM task_runtime r WHERE r.project_id=? AND r.state='succeeded' AND NOT EXISTS(SELECT 1 FROM task_acceptances a WHERE a.project_id=r.project_id AND a.plan_id=r.plan_id AND a.version=r.version AND a.task_id=r.task_id)")
            .bind(key.project_id.as_str()).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(unvalidated, 0, "runtime success without trusted acceptance");
        tx.commit().await.unwrap();
        let s = store.financial_snapshot(&key.project_id).await.unwrap();
        assert_eq!(s.ceiling.minor_units(), ceiling);
        assert_eq!(s.uncertain_liabilities, unknown as u64);
        let breakdowns = [s.settled, s.held, s.provisional];
        for (i, b) in breakdowns.iter().enumerate() {
            for (j, m) in [b.ai_api, b.compute, b.storage, b.transfer]
                .into_iter()
                .enumerate()
            {
                assert_eq!(i128::from(m.minor_units()), positions[i * 4 + j]);
            }
        }
        let exposure: i128 = positions.iter().sum();
        assert_eq!(s.over_ceiling, exposure > i128::from(ceiling));
        assert_eq!(
            i128::from(s.headroom.minor_units()),
            (i128::from(ceiling) - exposure).max(0)
        );
        Self {
            fingerprint: plan.revision.fingerprint().into(),
            nodes,
            edges,
            ceiling,
            positions,
            uncertain: unknown,
            attempts,
            outbox,
            effects,
            fences,
            runtime,
            accepted,
        }
    }
    pub fn preserves(&self, before: &Self) {
        assert_eq!(self.fingerprint, before.fingerprint);
        assert_eq!(self.nodes, before.nodes);
        assert_eq!(self.edges, before.edges);
        assert!(self.attempts >= before.attempts);
        for (task, epoch) in &before.fences {
            let current = self.fences.iter().find(|(id, _)| id == task).unwrap();
            assert!(current.1 >= *epoch, "fencing epoch regressed");
        }
        for accepted in &before.accepted {
            assert!(
                self.accepted.contains(accepted),
                "accepted evidence disappeared"
            );
        }
    }
}
