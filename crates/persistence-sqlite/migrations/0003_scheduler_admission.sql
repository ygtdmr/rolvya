-- R06: SQLite authority only; no launch occurs in this migration or transaction.
CREATE TABLE scheduler_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 request_id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK (kind IN ('admit','dispatch','validate','fence','recover')),
 actor TEXT NOT NULL,
 recorded_at INTEGER NOT NULL CHECK (recorded_at>=0),
 payload TEXT NOT NULL CHECK (json_valid(payload) AND length(payload) BETWEEN 1 AND 32768),
 UNIQUE(project_id,request_id), UNIQUE(sequence,project_id)
) STRICT;
CREATE TABLE task_attempts (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 plan_id TEXT NOT NULL, version INTEGER NOT NULL, task_id TEXT NOT NULL,
 epoch INTEGER NOT NULL CHECK(epoch>0),
 financial_event INTEGER NOT NULL UNIQUE,
 admission_event INTEGER NOT NULL UNIQUE,
 capability_id TEXT NOT NULL UNIQUE,
 admitted_at INTEGER NOT NULL CHECK(admitted_at>=0),
 lease_until INTEGER NOT NULL CHECK(lease_until>admitted_at AND lease_until<=admitted_at+90),
 PRIMARY KEY(project_id,attempt_id),
 UNIQUE(project_id,plan_id,version,task_id,epoch),
 UNIQUE(project_id,plan_id,version,task_id,attempt_id,epoch),
 FOREIGN KEY(project_id,plan_id,version,task_id) REFERENCES tasks(project_id,plan_id,version,task_id),
 FOREIGN KEY(financial_event,project_id) REFERENCES financial_events(sequence,project_id),
 FOREIGN KEY(admission_event,project_id) REFERENCES scheduler_events(sequence,project_id)
) STRICT;
CREATE TABLE task_fences (
 project_id TEXT NOT NULL, plan_id TEXT NOT NULL, version INTEGER NOT NULL, task_id TEXT NOT NULL,
 epoch INTEGER NOT NULL CHECK(epoch>=0), current_attempt TEXT,
 PRIMARY KEY(project_id,plan_id,version,task_id),
 FOREIGN KEY(project_id,plan_id,version,task_id) REFERENCES tasks(project_id,plan_id,version,task_id),
 FOREIGN KEY(project_id,plan_id,version,task_id,current_attempt,epoch)
 REFERENCES task_attempts(project_id,plan_id,version,task_id,attempt_id,epoch) DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE TABLE attempt_states (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('admitted','fenced','succeeded')),
 fenced_at INTEGER CHECK(fenced_at>=0),
 CHECK((state='fenced')=(fenced_at IS NOT NULL)),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id)
) STRICT;
CREATE TABLE dispatch_outbox (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 UNIQUE(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id)
) STRICT;
CREATE TABLE dispatch_claims (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 event_sequence INTEGER NOT NULL UNIQUE,
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES dispatch_outbox(project_id,attempt_id),
 FOREIGN KEY(event_sequence,project_id) REFERENCES scheduler_events(sequence,project_id)
) STRICT;
CREATE TABLE task_acceptances (
 project_id TEXT NOT NULL, plan_id TEXT NOT NULL, version INTEGER NOT NULL, task_id TEXT NOT NULL,
 attempt_id TEXT, epoch INTEGER,
 fingerprint TEXT NOT NULL CHECK(length(fingerprint)=64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
 manifest_sha256 TEXT NOT NULL CHECK(length(manifest_sha256)=64 AND manifest_sha256 NOT GLOB '*[^0-9a-f]*'),
 evidence TEXT NOT NULL,
 event_sequence INTEGER NOT NULL UNIQUE,
 CHECK((attempt_id IS NULL)=(epoch IS NULL)),
 PRIMARY KEY(project_id,plan_id,version,task_id),
 FOREIGN KEY(project_id,plan_id,version,task_id) REFERENCES tasks(project_id,plan_id,version,task_id),
 FOREIGN KEY(project_id,plan_id,version,task_id,attempt_id,epoch)
 REFERENCES task_attempts(project_id,plan_id,version,task_id,attempt_id,epoch),
 FOREIGN KEY(event_sequence,project_id) REFERENCES scheduler_events(sequence,project_id)
) STRICT;
CREATE TABLE attempt_recovery (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 not_before INTEGER NOT NULL CHECK(not_before>=0),
 event_sequence INTEGER NOT NULL UNIQUE,
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id),
 FOREIGN KEY(event_sequence,project_id) REFERENCES scheduler_events(sequence,project_id)
) STRICT;
CREATE INDEX attempts_task ON task_attempts(project_id,plan_id,version,task_id,epoch);
CREATE INDEX pending_dispatch ON dispatch_outbox(project_id,sequence);
CREATE TRIGGER attempt_requires_active_leaf BEFORE INSERT ON task_attempts WHEN NOT EXISTS (
 SELECT 1 FROM tasks t JOIN plan_versions p USING(project_id,plan_id,version)
 WHERE t.project_id=NEW.project_id AND t.plan_id=NEW.plan_id AND t.version=NEW.version
 AND t.task_id=NEW.task_id AND t.kind='executable_leaf' AND p.state='active')
 OR NOT EXISTS (SELECT 1 FROM financial_events e JOIN liability_revisions r ON r.event_sequence=e.sequence
 WHERE e.sequence=NEW.financial_event AND e.project_id=NEW.project_id AND e.kind='reserve' AND r.liability_id=NEW.attempt_id AND r.version=1)
 OR EXISTS (SELECT 1 FROM task_attempts a JOIN attempt_states s USING(project_id,attempt_id)
 WHERE a.project_id=NEW.project_id AND a.plan_id=NEW.plan_id AND a.version=NEW.version AND a.task_id=NEW.task_id AND s.state='admitted')
BEGIN SELECT RAISE(ABORT,'attempt requires active leaf and unique reserved authority'); END;
CREATE TRIGGER fence_initial BEFORE INSERT ON task_fences WHEN NEW.epoch<>0 OR NEW.current_attempt IS NOT NULL
BEGIN SELECT RAISE(ABORT,'initialize unowned fence'); END;
CREATE TRIGGER fence_advance BEFORE UPDATE ON task_fences WHEN
 NEW.project_id<>OLD.project_id OR NEW.plan_id<>OLD.plan_id OR NEW.version<>OLD.version OR NEW.task_id<>OLD.task_id
 OR NEW.epoch<>OLD.epoch+1 OR (NEW.current_attempt IS NULL AND OLD.current_attempt IS NULL)
BEGIN SELECT RAISE(ABORT,'invalid fence transition'); END;
CREATE TRIGGER fence_no_delete BEFORE DELETE ON task_fences
BEGIN SELECT RAISE(ABORT,'preserve fence history'); END;
CREATE TRIGGER attempt_state_transition BEFORE UPDATE ON attempt_states WHEN
 NEW.project_id<>OLD.project_id OR NEW.attempt_id<>OLD.attempt_id OR OLD.state<>'admitted' OR NEW.state NOT IN ('fenced','succeeded')
BEGIN SELECT RAISE(ABORT,'invalid attempt transition'); END;
CREATE TRIGGER attempt_state_no_delete BEFORE DELETE ON attempt_states
BEGIN SELECT RAISE(ABORT,'preserve attempt state'); END;
CREATE TRIGGER preserve_accepted_task BEFORE UPDATE OF state ON task_runtime WHEN OLD.state='succeeded' AND NEW.state<>'succeeded'
 AND EXISTS (SELECT 1 FROM task_acceptances a WHERE a.project_id=OLD.project_id AND a.plan_id=OLD.plan_id AND a.version=OLD.version AND a.task_id=OLD.task_id)
BEGIN SELECT RAISE(ABORT,'preserve validated completed work'); END;
CREATE TRIGGER immutable_scheduler_events_update BEFORE UPDATE ON scheduler_events
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_scheduler_events_delete BEFORE DELETE ON scheduler_events
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_task_attempts_update BEFORE UPDATE ON task_attempts
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_task_attempts_delete BEFORE DELETE ON task_attempts
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_dispatch_outbox_update BEFORE UPDATE ON dispatch_outbox
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_dispatch_outbox_delete BEFORE DELETE ON dispatch_outbox
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_dispatch_claims_update BEFORE UPDATE ON dispatch_claims
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_dispatch_claims_delete BEFORE DELETE ON dispatch_claims
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_task_acceptances_update BEFORE UPDATE ON task_acceptances
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_task_acceptances_delete BEFORE DELETE ON task_acceptances
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_attempt_recovery_update BEFORE UPDATE ON attempt_recovery
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
CREATE TRIGGER immutable_attempt_recovery_delete BEFORE DELETE ON attempt_recovery
BEGIN SELECT RAISE(ABORT,'scheduler history is append only'); END;
