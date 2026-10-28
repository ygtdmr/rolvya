-- S2-04. No provider attestation/transport is enabled by schema installation.
CREATE TABLE worker_security_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 enrollment_id TEXT NOT NULL, request_id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('prepared','bootstrap','renew','message','revoked','retired','denied')),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 UNIQUE(enrollment_id,request_id,kind),
 UNIQUE(sequence,project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id),
 FOREIGN KEY(enrollment_id,project_id,attempt_id) REFERENCES worker_enrollments(enrollment_id,project_id,attempt_id)
) STRICT;
CREATE TABLE worker_enrollments (
 enrollment_id TEXT PRIMARY KEY CHECK(length(enrollment_id)=32 AND enrollment_id NOT GLOB '*[^0-9a-f]*'),
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL, worker_id TEXT NOT NULL,
 provider TEXT NOT NULL, account TEXT NOT NULL, pool TEXT NOT NULL,
 execution_identity TEXT NOT NULL,
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 16384),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 state TEXT NOT NULL CHECK(state IN ('pending','active','revoked')),
 security_epoch INTEGER NOT NULL CHECK(security_epoch>0),
 current_generation INTEGER NOT NULL CHECK(current_generation BETWEEN 0 AND 64),
 revision INTEGER NOT NULL CHECK(revision>=0),
 created_at INTEGER NOT NULL CHECK(created_at>=0), valid_until INTEGER NOT NULL CHECK(valid_until>created_at),
 UNIQUE(project_id,attempt_id), UNIQUE(provider,account,execution_identity),
 UNIQUE(enrollment_id,project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES dispatch_operations(project_id,attempt_id),
 CHECK((state='pending')=(current_generation=0) OR state='revoked')
) STRICT;
CREATE TABLE worker_credentials (
 credential_id TEXT PRIMARY KEY CHECK(length(credential_id)=32 AND credential_id NOT GLOB '*[^0-9a-f]*'),
 enrollment_id TEXT NOT NULL REFERENCES worker_enrollments(enrollment_id),
 generation INTEGER NOT NULL CHECK(generation BETWEEN 1 AND 64),
 token_lookup BLOB NOT NULL UNIQUE CHECK(length(token_lookup)=32),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 8192),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 issued_at INTEGER NOT NULL CHECK(issued_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at>issued_at AND expires_at<=issued_at+900),
 UNIQUE(enrollment_id,generation), UNIQUE(credential_id,enrollment_id),
 FOREIGN KEY(enrollment_id) REFERENCES worker_enrollments(enrollment_id)
) STRICT;
CREATE TABLE worker_bootstrap_challenges (
 challenge_id TEXT PRIMARY KEY CHECK(length(challenge_id)=32 AND challenge_id NOT GLOB '*[^0-9a-f]*'),
 enrollment_id TEXT NOT NULL UNIQUE REFERENCES worker_enrollments(enrollment_id),
 preparation_request TEXT NOT NULL UNIQUE,
 commitment BLOB NOT NULL CHECK(length(commitment)=32),
 issued_at INTEGER NOT NULL CHECK(issued_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at>issued_at AND expires_at<=issued_at+300),
 state TEXT NOT NULL CHECK(state IN ('pending','consumed','revoked')),
 credential_id TEXT,
 CHECK((state='consumed')=(credential_id IS NOT NULL)),
 FOREIGN KEY(credential_id,enrollment_id) REFERENCES worker_credentials(credential_id,enrollment_id)
) STRICT;
CREATE TABLE worker_credential_requests (
 enrollment_id TEXT NOT NULL, request_id TEXT NOT NULL,
 credential_id TEXT NOT NULL, project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 message_id TEXT NOT NULL, digest TEXT NOT NULL CHECK(length(digest)=64 AND digest NOT GLOB '*[^0-9a-f]*'),
 event_sequence INTEGER NOT NULL,
 PRIMARY KEY(enrollment_id,request_id),
 FOREIGN KEY(credential_id,enrollment_id) REFERENCES worker_credentials(credential_id,enrollment_id),
 FOREIGN KEY(enrollment_id,project_id,attempt_id) REFERENCES worker_enrollments(enrollment_id,project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id,message_id) REFERENCES worker_messages(project_id,attempt_id,message_id),
 FOREIGN KEY(event_sequence,project_id,attempt_id) REFERENCES worker_security_events(sequence,project_id,attempt_id)
) STRICT;
CREATE INDEX worker_enrollment_scope ON worker_enrollments(project_id,state,enrollment_id);
CREATE INDEX worker_credential_lifetime ON worker_credentials(enrollment_id,expires_at,generation);
CREATE INDEX worker_security_kind ON worker_security_events(enrollment_id,kind,sequence);
CREATE TABLE worker_revocations (
 enrollment_id TEXT PRIMARY KEY REFERENCES worker_enrollments(enrollment_id),
 request_id TEXT NOT NULL UNIQUE, evidence TEXT NOT NULL,
 expected_security_epoch INTEGER NOT NULL CHECK(expected_security_epoch>0),
 security_epoch INTEGER NOT NULL CHECK(security_epoch>0),
 fencing_epoch INTEGER CHECK(fencing_epoch>0),
 event_sequence INTEGER NOT NULL REFERENCES worker_security_events(sequence)
) STRICT;
CREATE TRIGGER worker_revocation_no_update BEFORE UPDATE ON worker_revocations BEGIN SELECT RAISE(ABORT,'immutable worker revocation'); END;
CREATE TRIGGER worker_revocation_no_delete BEFORE DELETE ON worker_revocations BEGIN SELECT RAISE(ABORT,'preserve worker revocation'); END;
CREATE TRIGGER worker_enrollment_initial BEFORE INSERT ON worker_enrollments WHEN
 NEW.state<>'pending' OR NEW.security_epoch<>1 OR NEW.current_generation<>0 OR NEW.revision<>0
 OR NOT EXISTS(SELECT 1 FROM dispatch_operations d JOIN attempt_states s USING(project_id,attempt_id)
 WHERE d.project_id=NEW.project_id AND d.attempt_id=NEW.attempt_id AND d.state='active' AND s.state='admitted')
BEGIN SELECT RAISE(ABORT,'verified pending enrollment required'); END;
CREATE TRIGGER worker_enrollment_transition BEFORE UPDATE ON worker_enrollments WHEN
 NEW.enrollment_id<>OLD.enrollment_id OR NEW.project_id<>OLD.project_id OR NEW.attempt_id<>OLD.attempt_id
 OR NEW.worker_id<>OLD.worker_id OR NEW.provider<>OLD.provider OR NEW.account<>OLD.account OR NEW.pool<>OLD.pool
 OR NEW.execution_identity<>OLD.execution_identity OR NEW.record<>OLD.record OR NEW.record_mac<>OLD.record_mac
 OR NEW.created_at<>OLD.created_at OR NEW.valid_until<>OLD.valid_until OR NEW.revision<>OLD.revision+1
 OR OLD.state='revoked' OR NEW.state='pending'
 OR (NEW.state='active' AND (NEW.security_epoch<>OLD.security_epoch OR NEW.current_generation<>OLD.current_generation+1
 OR NOT EXISTS(SELECT 1 FROM worker_credentials c WHERE c.enrollment_id=NEW.enrollment_id AND c.generation=NEW.current_generation)))
 OR (NEW.state='revoked' AND (NEW.security_epoch<>OLD.security_epoch+1 OR NEW.current_generation<>OLD.current_generation))
BEGIN SELECT RAISE(ABORT,'invalid worker authority transition'); END;
CREATE TRIGGER worker_enrollment_no_delete BEFORE DELETE ON worker_enrollments BEGIN SELECT RAISE(ABORT,'preserve worker identity'); END;
CREATE TRIGGER worker_credential_generation BEFORE INSERT ON worker_credentials WHEN NOT EXISTS(
 SELECT 1 FROM worker_enrollments e WHERE e.enrollment_id=NEW.enrollment_id AND e.state IN ('pending','active')
 AND NEW.generation=e.current_generation+1 AND NEW.issued_at>=e.created_at AND NEW.expires_at<=e.valid_until)
BEGIN SELECT RAISE(ABORT,'current scoped generation required'); END;
CREATE TRIGGER worker_credential_no_update BEFORE UPDATE ON worker_credentials BEGIN SELECT RAISE(ABORT,'immutable worker credential'); END;
CREATE TRIGGER worker_credential_no_delete BEFORE DELETE ON worker_credentials BEGIN SELECT RAISE(ABORT,'preserve worker credential history'); END;
CREATE TRIGGER worker_bootstrap_initial BEFORE INSERT ON worker_bootstrap_challenges WHEN NEW.state<>'pending' OR NEW.credential_id IS NOT NULL
 OR NOT EXISTS(SELECT 1 FROM worker_enrollments e WHERE e.enrollment_id=NEW.enrollment_id AND e.state='pending'
 AND NEW.issued_at=e.created_at AND NEW.expires_at<=e.valid_until)
BEGIN SELECT RAISE(ABORT,'pending scoped bootstrap required'); END;
CREATE TRIGGER worker_bootstrap_transition BEFORE UPDATE ON worker_bootstrap_challenges WHEN
 NEW.challenge_id<>OLD.challenge_id OR NEW.enrollment_id<>OLD.enrollment_id OR NEW.preparation_request<>OLD.preparation_request
 OR NEW.commitment<>OLD.commitment OR NEW.issued_at<>OLD.issued_at OR NEW.expires_at<>OLD.expires_at
 OR OLD.state<>'pending' OR NEW.state='pending'
BEGIN SELECT RAISE(ABORT,'one-use bootstrap'); END;
CREATE TRIGGER worker_bootstrap_no_delete BEFORE DELETE ON worker_bootstrap_challenges BEGIN SELECT RAISE(ABORT,'preserve bootstrap evidence'); END;
CREATE TRIGGER worker_security_no_update BEFORE UPDATE ON worker_security_events BEGIN SELECT RAISE(ABORT,'append-only worker security audit'); END;
CREATE TRIGGER worker_security_no_delete BEFORE DELETE ON worker_security_events BEGIN SELECT RAISE(ABORT,'append-only worker security audit'); END;
CREATE TRIGGER worker_request_no_update BEFORE UPDATE ON worker_credential_requests BEGIN SELECT RAISE(ABORT,'immutable authenticated request'); END;
CREATE TRIGGER worker_request_no_delete BEFORE DELETE ON worker_credential_requests BEGIN SELECT RAISE(ABORT,'preserve authenticated request'); END;
CREATE TRIGGER worker_request_limit BEFORE INSERT ON worker_credential_requests WHEN
 (SELECT count(*) FROM worker_credential_requests WHERE enrollment_id=NEW.enrollment_id)>=8192
BEGIN SELECT RAISE(ABORT,'authenticated request history bound'); END;
-- Retirement invalidates every credential generation. It neither releases money
-- nor establishes remote termination. Existing R06/R08 evidence remains authority.
CREATE TRIGGER retire_worker_credentials AFTER UPDATE OF state ON attempt_states WHEN NEW.state<>'admitted'
BEGIN
 INSERT INTO worker_security_events(project_id,attempt_id,enrollment_id,request_id,kind,recorded_at)
 SELECT project_id,attempt_id,enrollment_id,'attempt-retired','retired',coalesce(NEW.fenced_at,unixepoch())
 FROM worker_enrollments WHERE project_id=NEW.project_id AND attempt_id=NEW.attempt_id AND state<>'revoked';
 UPDATE worker_enrollments SET state='revoked',security_epoch=security_epoch+1,revision=revision+1
 WHERE project_id=NEW.project_id AND attempt_id=NEW.attempt_id AND state<>'revoked';
 UPDATE worker_bootstrap_challenges SET state='revoked' WHERE state='pending' AND enrollment_id IN
 (SELECT enrollment_id FROM worker_enrollments WHERE project_id=NEW.project_id AND attempt_id=NEW.attempt_id);
END;
