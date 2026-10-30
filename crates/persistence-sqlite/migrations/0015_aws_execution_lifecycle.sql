-- S2-06D: provider receipts are evidence, never a second worker or financial authority.
CREATE TABLE aws_launch_requests (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 account_ref TEXT NOT NULL,account_version INTEGER NOT NULL,
 client_token TEXT NOT NULL CHECK(length(client_token)=64),
 request_digest TEXT NOT NULL CHECK(length(request_digest)=64),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 task_role TEXT NOT NULL UNIQUE,secret_arn TEXT NOT NULL UNIQUE,
 created_at INTEGER NOT NULL CHECK(created_at>=0),
 PRIMARY KEY(project_id,attempt_id),UNIQUE(client_token),
 FOREIGN KEY(project_id,attempt_id) REFERENCES dispatch_operations(project_id,attempt_id),
 FOREIGN KEY(project_id,account_ref,account_version) REFERENCES aws_infrastructure_account_versions(project_id,account_ref,version),
 CHECK(json_extract(record,'$.binding.task.plan.project_id')=project_id),
 CHECK(json_extract(record,'$.binding.attempt_id')=attempt_id),
 CHECK(json_extract(record,'$.account_ref')=account_ref),
 CHECK(json_extract(record,'$.account_version')=account_version),
 CHECK(json_extract(record,'$.client_token')=client_token),
 CHECK(json_extract(record,'$.workload.task_role')=task_role),
 CHECK(json_extract(record,'$.bootstrap.secret')=secret_arn)
) STRICT;
CREATE TABLE aws_execution_events (
 project_id TEXT NOT NULL,attempt_id TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0),
 kind TEXT NOT NULL CHECK(kind IN ('prepared','submitting','receipt','unknown','inspection','stop_requested','stop_ack','stop_unknown','cleanup_requested','cleanup_observed','reconciliation_required')),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 PRIMARY KEY(project_id,attempt_id,revision),
 FOREIGN KEY(project_id,attempt_id) REFERENCES aws_launch_requests(project_id,attempt_id)
) STRICT;
CREATE INDEX aws_execution_event_kind ON aws_execution_events(project_id,attempt_id,kind,revision);
CREATE TRIGGER aws_execution_revision BEFORE INSERT ON aws_execution_events
WHEN NEW.revision!=coalesce((SELECT max(revision) FROM aws_execution_events WHERE project_id=NEW.project_id AND attempt_id=NEW.attempt_id),0)+1
BEGIN SELECT RAISE(ABORT,'AWS execution revision rejected'); END;
-- Every adapter opening records a new paused boot. Restoring the DB cannot
-- restore external AWS reality; no production clearance exists in this slice.
CREATE TABLE aws_execution_recovery (
 boot_id TEXT PRIMARY KEY CHECK(length(boot_id)=32),
 installation TEXT NOT NULL CHECK(length(installation)=32),
 opened_at INTEGER NOT NULL CHECK(opened_at>=0),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32)
) STRICT;
CREATE TABLE aws_lifecycle_credential_uses (
 project_id TEXT NOT NULL,account_ref TEXT NOT NULL,consent_generation INTEGER NOT NULL,
 request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 64),
 connection_id TEXT NOT NULL,record_id TEXT NOT NULL,version INTEGER NOT NULL,
 operation TEXT NOT NULL CHECK(operation IN ('launch_execution','stop_execution','reconcile_execution')),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,request_id,operation),
 FOREIGN KEY(project_id,account_ref,consent_generation) REFERENCES aws_account_consents(project_id,account_ref,generation),
 FOREIGN KEY(project_id,connection_id,record_id,version) REFERENCES secret_versions(project_id,connection_id,record_id,version)
) STRICT;
CREATE TRIGGER aws_launch_no_update BEFORE UPDATE ON aws_launch_requests BEGIN SELECT RAISE(ABORT,'AWS launch immutable'); END;
CREATE TRIGGER aws_launch_no_delete BEFORE DELETE ON aws_launch_requests BEGIN SELECT RAISE(ABORT,'AWS launch retained'); END;
CREATE TRIGGER aws_execution_no_update BEFORE UPDATE ON aws_execution_events BEGIN SELECT RAISE(ABORT,'AWS execution evidence immutable'); END;
CREATE TRIGGER aws_execution_no_delete BEFORE DELETE ON aws_execution_events BEGIN SELECT RAISE(ABORT,'AWS execution evidence retained'); END;
CREATE TRIGGER aws_recovery_no_update BEFORE UPDATE ON aws_execution_recovery BEGIN SELECT RAISE(ABORT,'AWS recovery evidence immutable'); END;
CREATE TRIGGER aws_recovery_no_delete BEFORE DELETE ON aws_execution_recovery BEGIN SELECT RAISE(ABORT,'AWS recovery evidence retained'); END;
CREATE TRIGGER aws_lifecycle_use_no_update BEFORE UPDATE ON aws_lifecycle_credential_uses BEGIN SELECT RAISE(ABORT,'AWS credential evidence immutable'); END;
CREATE TRIGGER aws_lifecycle_use_no_delete BEFORE DELETE ON aws_lifecycle_credential_uses BEGIN SELECT RAISE(ABORT,'AWS credential evidence retained'); END;
CREATE TABLE aws_sdk_receipts (
 project_id TEXT NOT NULL,attempt_id TEXT NOT NULL,revision INTEGER NOT NULL,
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 PRIMARY KEY(project_id,attempt_id,revision),
 FOREIGN KEY(project_id,attempt_id,revision) REFERENCES aws_execution_events(project_id,attempt_id,revision)
) STRICT;
CREATE TRIGGER aws_sdk_receipt_no_update BEFORE UPDATE ON aws_sdk_receipts BEGIN SELECT RAISE(ABORT,'AWS receipt immutable'); END;
CREATE TRIGGER aws_sdk_receipt_no_delete BEFORE DELETE ON aws_sdk_receipts BEGIN SELECT RAISE(ABORT,'AWS receipt retained'); END;
CREATE TRIGGER aws_launch_role_exclusive BEFORE INSERT ON aws_launch_requests
WHEN EXISTS(SELECT 1 FROM aws_attempt_roles r WHERE (r.task_role=NEW.task_role OR r.secret_arn=NEW.secret_arn) AND (r.project_id!=NEW.project_id OR r.attempt_id!=NEW.attempt_id OR r.task_role!=NEW.task_role OR r.secret_arn!=NEW.secret_arn))
BEGIN SELECT RAISE(ABORT,'AWS launch role already allocated'); END;
CREATE TRIGGER aws_bootstrap_role_exclusive BEFORE INSERT ON aws_attempt_roles
WHEN EXISTS(SELECT 1 FROM aws_launch_requests r WHERE (r.task_role=NEW.task_role OR r.secret_arn=NEW.secret_arn) AND (r.project_id!=NEW.project_id OR r.attempt_id!=NEW.attempt_id OR r.task_role!=NEW.task_role OR r.secret_arn!=NEW.secret_arn))
BEGIN SELECT RAISE(ABORT,'AWS bootstrap allocation mismatch'); END;
