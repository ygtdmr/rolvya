-- Extend the existing AWS operation journal: one coding and one independent
-- verifier execution per admitted attempt. Preserve every historical byte/MAC.
DROP TRIGGER aws_bootstrap_role_exclusive;
CREATE TABLE aws_launch_requests_g104 (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 purpose TEXT NOT NULL CHECK(purpose IN ('coding','verifier')),
 account_ref TEXT NOT NULL,account_version INTEGER NOT NULL,
 client_token TEXT NOT NULL CHECK(length(client_token)=64),
 request_digest TEXT NOT NULL CHECK(length(request_digest)=64),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 task_role TEXT NOT NULL UNIQUE,secret_arn TEXT NOT NULL UNIQUE,
 created_at INTEGER NOT NULL CHECK(created_at>=0),
 PRIMARY KEY(project_id,attempt_id,purpose),UNIQUE(client_token),
 FOREIGN KEY(project_id,attempt_id) REFERENCES dispatch_operations(project_id,attempt_id),
 FOREIGN KEY(project_id,account_ref,account_version) REFERENCES aws_infrastructure_account_versions(project_id,account_ref,version),
 CHECK((purpose='coding' AND json_type(record,'$.verifier') IS NULL) OR (purpose='verifier' AND json_type(record,'$.verifier')='object')),
 CHECK(json_extract(record,'$.binding.task.plan.project_id')=project_id),
 CHECK(json_extract(record,'$.binding.attempt_id')=attempt_id),
 CHECK(json_extract(record,'$.account_ref')=account_ref),
 CHECK(json_extract(record,'$.account_version')=account_version),
 CHECK(json_extract(record,'$.client_token')=client_token),
 CHECK(json_extract(record,'$.workload.task_role')=task_role),
 CHECK(json_extract(record,'$.bootstrap.secret')=secret_arn)
) STRICT;
CREATE TABLE aws_execution_events_g104 (
 project_id TEXT NOT NULL,attempt_id TEXT NOT NULL,
 purpose TEXT NOT NULL CHECK(purpose IN ('coding','verifier')),revision INTEGER NOT NULL CHECK(revision>0),
 kind TEXT NOT NULL CHECK(kind IN ('prepared','submitting','receipt','unknown','inspection','stop_requested','stop_ack','stop_unknown','cleanup_requested','cleanup_observed','reconciliation_required')),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 PRIMARY KEY(project_id,attempt_id,purpose,revision),
 FOREIGN KEY(project_id,attempt_id,purpose) REFERENCES aws_launch_requests_g104(project_id,attempt_id,purpose)
) STRICT;

CREATE TABLE aws_sdk_receipts_g104 (
 project_id TEXT NOT NULL,attempt_id TEXT NOT NULL,
 purpose TEXT NOT NULL CHECK(purpose IN ('coding','verifier')),revision INTEGER NOT NULL,
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 PRIMARY KEY(project_id,attempt_id,purpose,revision),
 FOREIGN KEY(project_id,attempt_id,purpose,revision) REFERENCES aws_execution_events_g104(project_id,attempt_id,purpose,revision)
) STRICT;

INSERT INTO aws_launch_requests_g104 SELECT project_id,attempt_id,'coding',account_ref,account_version,client_token,request_digest,record,record_mac,task_role,secret_arn,created_at FROM aws_launch_requests;
INSERT INTO aws_execution_events_g104 SELECT project_id,attempt_id,'coding',revision,kind,recorded_at,record,record_mac FROM aws_execution_events;
INSERT INTO aws_sdk_receipts_g104 SELECT project_id,attempt_id,'coding',revision,record,record_mac FROM aws_sdk_receipts;
DROP TABLE aws_sdk_receipts;
DROP TABLE aws_execution_events;
DROP TABLE aws_launch_requests;
ALTER TABLE aws_launch_requests_g104 RENAME TO aws_launch_requests;
ALTER TABLE aws_execution_events_g104 RENAME TO aws_execution_events;
ALTER TABLE aws_sdk_receipts_g104 RENAME TO aws_sdk_receipts;

CREATE INDEX aws_execution_event_kind ON aws_execution_events(project_id,attempt_id,purpose,kind,revision);
CREATE TRIGGER aws_execution_revision BEFORE INSERT ON aws_execution_events
WHEN NEW.revision!=coalesce((SELECT max(revision) FROM aws_execution_events WHERE project_id=NEW.project_id AND attempt_id=NEW.attempt_id AND purpose=NEW.purpose),0)+1
BEGIN SELECT RAISE(ABORT,'AWS execution revision rejected'); END;
CREATE TRIGGER aws_launch_role_exclusive BEFORE INSERT ON aws_launch_requests
WHEN EXISTS(SELECT 1 FROM aws_attempt_roles r WHERE (r.task_role=NEW.task_role OR r.secret_arn=NEW.secret_arn) AND (NEW.purpose!='coding' OR r.project_id!=NEW.project_id OR r.attempt_id!=NEW.attempt_id OR r.task_role!=NEW.task_role OR r.secret_arn!=NEW.secret_arn))
BEGIN SELECT RAISE(ABORT,'AWS launch role already allocated'); END;
CREATE TRIGGER aws_bootstrap_role_exclusive BEFORE INSERT ON aws_attempt_roles
WHEN EXISTS(SELECT 1 FROM aws_launch_requests r WHERE (r.task_role=NEW.task_role OR r.secret_arn=NEW.secret_arn) AND (r.purpose!='coding' OR r.project_id!=NEW.project_id OR r.attempt_id!=NEW.attempt_id OR r.task_role!=NEW.task_role OR r.secret_arn!=NEW.secret_arn))
BEGIN SELECT RAISE(ABORT,'AWS bootstrap allocation mismatch'); END;
CREATE TRIGGER aws_launch_requests_update BEFORE UPDATE ON aws_launch_requests BEGIN SELECT RAISE(ABORT,'AWS evidence immutable'); END;
CREATE TRIGGER aws_launch_requests_delete BEFORE DELETE ON aws_launch_requests BEGIN SELECT RAISE(ABORT,'AWS evidence immutable'); END;
CREATE TRIGGER aws_execution_events_update BEFORE UPDATE ON aws_execution_events BEGIN SELECT RAISE(ABORT,'AWS evidence immutable'); END;
CREATE TRIGGER aws_execution_events_delete BEFORE DELETE ON aws_execution_events BEGIN SELECT RAISE(ABORT,'AWS evidence immutable'); END;
CREATE TRIGGER aws_sdk_receipts_update BEFORE UPDATE ON aws_sdk_receipts BEGIN SELECT RAISE(ABORT,'AWS evidence immutable'); END;
CREATE TRIGGER aws_sdk_receipts_delete BEFORE DELETE ON aws_sdk_receipts BEGIN SELECT RAISE(ABORT,'AWS evidence immutable'); END;
-- Protected delivery copy only; existing coding_verifier_executions/requests/reports
-- remain the sole verifier authority/evidence, and AWS events the lifecycle journal.
CREATE TABLE aws_verifier_bootstrap (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,purpose TEXT NOT NULL CHECK(purpose='verifier'),
 data_nonce BLOB NOT NULL CHECK(length(data_nonce)=24),wrap_nonce BLOB NOT NULL CHECK(length(wrap_nonce)=24),
 wrapped BLOB NOT NULL CHECK(length(wrapped)=48),ciphertext BLOB NOT NULL CHECK(length(ciphertext) BETWEEN 17 AND 4112),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id,purpose) REFERENCES aws_launch_requests(project_id,attempt_id,purpose)
) STRICT;
CREATE TRIGGER aws_verifier_bootstrap_update BEFORE UPDATE ON aws_verifier_bootstrap BEGIN SELECT RAISE(ABORT,'immutable verifier custody'); END;
CREATE TRIGGER aws_verifier_bootstrap_delete BEFORE DELETE ON aws_verifier_bootstrap BEGIN SELECT RAISE(ABORT,'preserve verifier custody'); END;
