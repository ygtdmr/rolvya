-- Evidence for the existing attempt/outbox and R09 ledger, never another admission.
CREATE TABLE coding_work (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 work_id TEXT NOT NULL UNIQUE CHECK(length(work_id)=64),
 body BLOB NOT NULL CHECK(length(body) BETWEEN 1 AND 65536),
 seal BLOB NOT NULL CHECK(length(seal)=32),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES dispatch_operations(project_id,attempt_id)
) STRICT;
CREATE TABLE coding_deliveries (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL, request_id TEXT NOT NULL,
 digest TEXT NOT NULL CHECK(length(digest)=64),
 response_sha256 TEXT NOT NULL CHECK(length(response_sha256)=64),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,attempt_id,request_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES coding_work(project_id,attempt_id)
) STRICT;
CREATE TABLE coding_verifier_executions (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 execution_id TEXT NOT NULL UNIQUE,
 body BLOB NOT NULL CHECK(length(body) BETWEEN 1 AND 16384),
 seal BLOB NOT NULL CHECK(length(seal)=32),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES coding_artifacts(project_id,attempt_id)
) STRICT;
CREATE TABLE coding_verifier_requests (
 execution_id TEXT NOT NULL REFERENCES coding_verifier_executions(execution_id),
 request_id TEXT NOT NULL, digest TEXT NOT NULL CHECK(length(digest)=64),
 response_sha256 TEXT NOT NULL CHECK(length(response_sha256)=64),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(execution_id,request_id)
) STRICT;
CREATE TABLE coding_verifier_reports (
 execution_id TEXT PRIMARY KEY REFERENCES coding_verifier_executions(execution_id),
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 request_id TEXT NOT NULL, report_sha256 TEXT NOT NULL CHECK(length(report_sha256)=64),
 envelope BLOB NOT NULL CHECK(length(envelope) BETWEEN 1 AND 49152),
 seal BLOB NOT NULL CHECK(length(seal)=32),
 FOREIGN KEY(project_id,attempt_id) REFERENCES coding_artifacts(project_id,attempt_id),
 FOREIGN KEY(execution_id,request_id) REFERENCES coding_verifier_requests(execution_id,request_id)
) STRICT;
CREATE TABLE coding_verifier_revocations (
 execution_id TEXT PRIMARY KEY REFERENCES coding_verifier_executions(execution_id),
 reason TEXT NOT NULL CHECK(reason IN ('compromised','expired','interrupted','restore')),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 seal BLOB NOT NULL CHECK(length(seal)=32)
) STRICT;
CREATE TRIGGER coding_work_update BEFORE UPDATE ON coding_work BEGIN SELECT RAISE(ABORT,'immutable work'); END;
CREATE TRIGGER coding_work_delete BEFORE DELETE ON coding_work BEGIN SELECT RAISE(ABORT,'preserve work'); END;
CREATE TRIGGER coding_deliveries_update BEFORE UPDATE ON coding_deliveries BEGIN SELECT RAISE(ABORT,'immutable delivery'); END;
CREATE TRIGGER coding_deliveries_delete BEFORE DELETE ON coding_deliveries BEGIN SELECT RAISE(ABORT,'preserve delivery'); END;
CREATE TRIGGER coding_deliveries_limit BEFORE INSERT ON coding_deliveries WHEN (SELECT count(*) FROM coding_deliveries WHERE project_id=NEW.project_id AND attempt_id=NEW.attempt_id)>=1024 BEGIN SELECT RAISE(ABORT,'delivery bound'); END;
CREATE TRIGGER coding_verifier_executions_update BEFORE UPDATE ON coding_verifier_executions BEGIN SELECT RAISE(ABORT,'immutable verifier'); END;
CREATE TRIGGER coding_verifier_executions_delete BEFORE DELETE ON coding_verifier_executions BEGIN SELECT RAISE(ABORT,'preserve verifier'); END;
CREATE TRIGGER coding_verifier_requests_update BEFORE UPDATE ON coding_verifier_requests BEGIN SELECT RAISE(ABORT,'immutable verifier request'); END;
CREATE TRIGGER coding_verifier_requests_delete BEFORE DELETE ON coding_verifier_requests BEGIN SELECT RAISE(ABORT,'preserve verifier request'); END;
CREATE TRIGGER coding_verifier_requests_limit BEFORE INSERT ON coding_verifier_requests WHEN (SELECT count(*) FROM coding_verifier_requests WHERE execution_id=NEW.execution_id)>=1024 BEGIN SELECT RAISE(ABORT,'verifier request bound'); END;
CREATE TRIGGER coding_verifier_reports_update BEFORE UPDATE ON coding_verifier_reports BEGIN SELECT RAISE(ABORT,'immutable authenticated report'); END;
CREATE TRIGGER coding_verifier_reports_delete BEFORE DELETE ON coding_verifier_reports BEGIN SELECT RAISE(ABORT,'preserve authenticated report'); END;
CREATE TRIGGER coding_verifier_revocations_update BEFORE UPDATE ON coding_verifier_revocations BEGIN SELECT RAISE(ABORT,'immutable revocation'); END;
CREATE TRIGGER coding_verifier_revocations_delete BEFORE DELETE ON coding_verifier_revocations BEGIN SELECT RAISE(ABORT,'preserve revocation'); END;
-- A restored database cannot by itself roll remote execution back. Operators can
-- only restrict this guard here; no production resume/activation path is provided.
CREATE TABLE coding_recovery_holds (
 project_id TEXT PRIMARY KEY REFERENCES projects(project_id),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 seal BLOB NOT NULL CHECK(length(seal)=32)
) STRICT;
CREATE TRIGGER coding_recovery_holds_update BEFORE UPDATE ON coding_recovery_holds BEGIN SELECT RAISE(ABORT,'immutable recovery hold'); END;
CREATE TRIGGER coding_recovery_holds_delete BEFORE DELETE ON coding_recovery_holds BEGIN SELECT RAISE(ABORT,'preserve recovery hold'); END;
