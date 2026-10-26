-- R09 simulation evidence only; R06/R08 remain attempt/dispatch/message authority.
CREATE TABLE synthetic_jobs (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 started_at INTEGER NOT NULL CHECK(started_at>=0),
 specification TEXT NOT NULL CHECK(json_valid(specification) AND length(specification)<=32768),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id)
) STRICT;
CREATE TABLE synthetic_artifacts (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 content BLOB NOT NULL CHECK(length(content) BETWEEN 1 AND 8192),
 sha256 TEXT NOT NULL CHECK(length(sha256)=64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES synthetic_jobs(project_id,attempt_id)
) STRICT;
CREATE TABLE synthetic_worker_events (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 8),
 due_at INTEGER NOT NULL CHECK(due_at>=0),
 kind TEXT NOT NULL CHECK(kind IN ('message','disconnect')),
 payload BLOB NOT NULL CHECK(length(payload) BETWEEN 1 AND 8192),
 message_id TEXT, digest TEXT NOT NULL CHECK(length(digest)=64),
 delivery TEXT NOT NULL CHECK(delivery IN ('pending','delivered','rejected')),
 rejection TEXT,
 CHECK((delivery='rejected')=(rejection IS NOT NULL)),
 PRIMARY KEY(project_id,attempt_id,ordinal),
 FOREIGN KEY(project_id,attempt_id) REFERENCES synthetic_jobs(project_id,attempt_id)
) STRICT;
CREATE INDEX synthetic_due_events ON synthetic_worker_events(project_id,delivery,due_at,attempt_id,ordinal);
CREATE TABLE synthetic_validations (
 project_id TEXT NOT NULL, plan_id TEXT NOT NULL, version INTEGER NOT NULL, task_id TEXT NOT NULL,
 validation_id TEXT NOT NULL,
 attempt_id TEXT,
 manifest BLOB NOT NULL CHECK(length(manifest) BETWEEN 1 AND 65536),
 sha256 TEXT NOT NULL CHECK(length(sha256)=64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,validation_id),
 UNIQUE(project_id,plan_id,version,task_id,validation_id),
 FOREIGN KEY(project_id,plan_id,version,task_id) REFERENCES tasks(project_id,plan_id,version,task_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id)
) STRICT;
CREATE INDEX synthetic_task_evidence ON synthetic_validations(project_id,plan_id,version,task_id);
CREATE INDEX orchestration_runtime_state ON task_runtime(project_id,plan_id,version,state,task_id);
CREATE INDEX orchestration_task_kind ON tasks(project_id,plan_id,version,kind,task_id);
CREATE TRIGGER synthetic_delivery_once BEFORE UPDATE ON synthetic_worker_events WHEN
 OLD.delivery<>'pending' OR NEW.delivery NOT IN ('delivered','rejected')
 OR NEW.project_id<>OLD.project_id OR NEW.attempt_id<>OLD.attempt_id OR NEW.ordinal<>OLD.ordinal
 OR NEW.due_at<>OLD.due_at OR NEW.kind<>OLD.kind OR NEW.payload<>OLD.payload
 OR NEW.message_id IS NOT OLD.message_id OR NEW.digest<>OLD.digest
BEGIN SELECT RAISE(ABORT,'immutable synthetic event or invalid delivery'); END;
CREATE TRIGGER synthetic_event_no_delete BEFORE DELETE ON synthetic_worker_events BEGIN SELECT RAISE(ABORT,'preserve synthetic evidence'); END;
CREATE TRIGGER immutable_synthetic_jobs_update BEFORE UPDATE ON synthetic_jobs BEGIN SELECT RAISE(ABORT,'synthetic evidence is append only'); END;
CREATE TRIGGER immutable_synthetic_jobs_delete BEFORE DELETE ON synthetic_jobs BEGIN SELECT RAISE(ABORT,'synthetic evidence is append only'); END;
CREATE TRIGGER immutable_synthetic_artifacts_update BEFORE UPDATE ON synthetic_artifacts BEGIN SELECT RAISE(ABORT,'synthetic evidence is append only'); END;
CREATE TRIGGER immutable_synthetic_artifacts_delete BEFORE DELETE ON synthetic_artifacts BEGIN SELECT RAISE(ABORT,'synthetic evidence is append only'); END;
CREATE TRIGGER immutable_synthetic_validations_update BEFORE UPDATE ON synthetic_validations BEGIN SELECT RAISE(ABORT,'synthetic evidence is append only'); END;
CREATE TRIGGER immutable_synthetic_validations_delete BEFORE DELETE ON synthetic_validations BEGIN SELECT RAISE(ABORT,'synthetic evidence is append only'); END;
