-- AWS inspection/correlation evidence only. No cloud effect or worker permission.
CREATE TABLE aws_infrastructure_account_versions (
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 account_ref TEXT NOT NULL CHECK(length(account_ref) BETWEEN 1 AND 64),
 version INTEGER NOT NULL CHECK(version>0),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,account_ref,version),
 CHECK(json_extract(record,'$.format')=1),
 CHECK(json_extract(record,'$.project')=project_id),
 CHECK(json_extract(record,'$.account_ref')=account_ref),
 CHECK(json_extract(record,'$.version')=version)
) STRICT;
CREATE TABLE aws_infrastructure_accounts (
 project_id TEXT NOT NULL, account_ref TEXT NOT NULL,
 current_version INTEGER NOT NULL CHECK(current_version>0),
 PRIMARY KEY(project_id,account_ref),
 FOREIGN KEY(project_id,account_ref,current_version) REFERENCES aws_infrastructure_account_versions(project_id,account_ref,version)
) STRICT;
CREATE TRIGGER aws_account_head_guard BEFORE UPDATE ON aws_infrastructure_accounts
WHEN NEW.project_id!=OLD.project_id OR NEW.account_ref!=OLD.account_ref OR NEW.current_version!=OLD.current_version+1
BEGIN SELECT RAISE(ABORT,'AWS account version transition rejected'); END;
CREATE TABLE aws_execution_bindings (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 account_ref TEXT NOT NULL, account_version INTEGER NOT NULL,
 task_arn TEXT NOT NULL CHECK(length(task_arn) BETWEEN 20 AND 2048),
 request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 64),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,attempt_id), UNIQUE(task_arn), UNIQUE(project_id,request_id),
 UNIQUE(project_id,attempt_id,account_ref,account_version),
 FOREIGN KEY(project_id,attempt_id) REFERENCES dispatch_operations(project_id,attempt_id),
 FOREIGN KEY(project_id,account_ref,account_version) REFERENCES aws_infrastructure_account_versions(project_id,account_ref,version),
 CHECK(json_extract(record,'$.format')=1),
 CHECK(json_extract(record,'$.expected.binding.task.plan.project_id')=project_id),
 CHECK(json_extract(record,'$.expected.binding.attempt_id')=attempt_id),
 CHECK(json_extract(record,'$.expected.account_ref')=account_ref),
 CHECK(json_extract(record,'$.expected.account_version')=account_version),
 CHECK(json_extract(record,'$.expected.task')=task_arn)
) STRICT;
CREATE TABLE aws_task_inspections (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision>0),
 account_ref TEXT NOT NULL, account_version INTEGER NOT NULL,
 request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 64),
 observed_at INTEGER NOT NULL CHECK(observed_at>=0),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 8192),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 PRIMARY KEY(project_id,attempt_id,revision), UNIQUE(project_id,attempt_id,request_id),
 FOREIGN KEY(project_id,attempt_id,account_ref,account_version) REFERENCES aws_execution_bindings(project_id,attempt_id,account_ref,account_version),
 FOREIGN KEY(project_id,account_ref,account_version) REFERENCES aws_infrastructure_account_versions(project_id,account_ref,version),
 CHECK(json_extract(record,'$.format')=1),
 CHECK(json_extract(record,'$.revision')=revision),
 CHECK(json_extract(record,'$.project')=project_id),
 CHECK(json_extract(record,'$.attempt')=attempt_id),
 CHECK(json_extract(record,'$.observed_at')=observed_at),
 CHECK(json_extract(record,'$.account_ref')=account_ref),
 CHECK(json_extract(record,'$.account_version')=account_version)
) STRICT;
CREATE TRIGGER aws_inspection_revision BEFORE INSERT ON aws_task_inspections
WHEN NEW.revision!=coalesce((SELECT max(revision)+1 FROM aws_task_inspections WHERE project_id=NEW.project_id AND attempt_id=NEW.attempt_id),1)
 OR NEW.observed_at<coalesce((SELECT max(observed_at) FROM aws_task_inspections WHERE project_id=NEW.project_id AND attempt_id=NEW.attempt_id),0)
BEGIN SELECT RAISE(ABORT,'AWS inspection ordering rejected'); END;
CREATE TRIGGER aws_account_version_no_update BEFORE UPDATE ON aws_infrastructure_account_versions BEGIN SELECT RAISE(ABORT,'AWS account history immutable'); END;
CREATE TRIGGER aws_account_version_no_delete BEFORE DELETE ON aws_infrastructure_account_versions BEGIN SELECT RAISE(ABORT,'AWS account history retained'); END;
CREATE TRIGGER aws_account_head_no_delete BEFORE DELETE ON aws_infrastructure_accounts BEGIN SELECT RAISE(ABORT,'AWS account history retained'); END;
CREATE TRIGGER aws_binding_no_update BEFORE UPDATE ON aws_execution_bindings BEGIN SELECT RAISE(ABORT,'AWS execution binding immutable'); END;
CREATE TRIGGER aws_binding_no_delete BEFORE DELETE ON aws_execution_bindings BEGIN SELECT RAISE(ABORT,'AWS execution binding retained'); END;
CREATE TRIGGER aws_inspection_no_update BEFORE UPDATE ON aws_task_inspections BEGIN SELECT RAISE(ABORT,'AWS inspection immutable'); END;
CREATE TRIGGER aws_inspection_no_delete BEFORE DELETE ON aws_task_inspections BEGIN SELECT RAISE(ABORT,'AWS reconciliation evidence retained'); END;
