-- S2-06C. Consent, protected delivery and possession evidence; existing worker and
-- financial records remain authoritative. No plaintext secret or ambient grant.
CREATE TABLE aws_account_proposals (
 proposal_id TEXT PRIMARY KEY CHECK(length(proposal_id)=32),
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 actor TEXT NOT NULL REFERENCES human_users(user_id),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 created_at INTEGER NOT NULL CHECK(created_at>=0), expires_at INTEGER NOT NULL CHECK(expires_at>created_at AND expires_at<=created_at+600)
) STRICT;
CREATE TABLE aws_account_consents (
 project_id TEXT NOT NULL, account_ref TEXT NOT NULL, generation INTEGER NOT NULL CHECK(generation>0),
 proposal_id TEXT NOT NULL UNIQUE REFERENCES aws_account_proposals(proposal_id),
 actor TEXT NOT NULL REFERENCES human_users(user_id),
 role_version INTEGER NOT NULL CHECK(role_version>0), security_epoch INTEGER NOT NULL CHECK(security_epoch>0),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32), recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,account_ref,generation), FOREIGN KEY(project_id) REFERENCES projects(project_id)
) STRICT;
CREATE TABLE aws_account_consent_heads (
 project_id TEXT NOT NULL, account_ref TEXT NOT NULL, generation INTEGER NOT NULL CHECK(generation>0),
 PRIMARY KEY(project_id,account_ref), FOREIGN KEY(project_id,account_ref,generation) REFERENCES aws_account_consents(project_id,account_ref,generation)
) STRICT;
CREATE TABLE aws_controller_checks (
 project_id TEXT NOT NULL, account_ref TEXT NOT NULL, consent_generation INTEGER NOT NULL,
 request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 64),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 8192),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 observed_at INTEGER NOT NULL CHECK(observed_at>=0), expires_at INTEGER NOT NULL CHECK(expires_at>observed_at AND expires_at<=observed_at+60),
 PRIMARY KEY(project_id,account_ref,request_id),
 FOREIGN KEY(project_id,account_ref,consent_generation) REFERENCES aws_account_consents(project_id,account_ref,generation)
) STRICT;
CREATE INDEX aws_controller_check_current ON aws_controller_checks(project_id,account_ref,observed_at DESC);
CREATE TABLE aws_attempt_roles (
 project_id TEXT NOT NULL,attempt_id TEXT NOT NULL,account_ref TEXT NOT NULL,account_version INTEGER NOT NULL,
 task_role TEXT NOT NULL UNIQUE CHECK(length(task_role) BETWEEN 20 AND 2048),
 secret_arn TEXT NOT NULL UNIQUE CHECK(length(secret_arn) BETWEEN 20 AND 2048),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768), record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id,account_ref,account_version) REFERENCES aws_execution_bindings(project_id,attempt_id,account_ref,account_version)
) STRICT;
CREATE TABLE aws_protected_challenges (
 challenge_id TEXT PRIMARY KEY CHECK(length(challenge_id)=32),
 project_id TEXT NOT NULL,attempt_id TEXT NOT NULL,
 enrollment_id TEXT NOT NULL REFERENCES worker_enrollments(enrollment_id),
 operation TEXT NOT NULL CHECK(operation IN ('bootstrap','renew')),
 previous_credential TEXT REFERENCES worker_credentials(credential_id),
 generation INTEGER NOT NULL CHECK(generation BETWEEN 1 AND 64),
 request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 64),
 secret_version TEXT NOT NULL UNIQUE CHECK(length(secret_version)=32),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 32768), record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 key_id BLOB NOT NULL CHECK(length(key_id)=16),key_version INTEGER NOT NULL CHECK(key_version>0),
 data_nonce BLOB NOT NULL CHECK(length(data_nonce)=24), wrap_nonce BLOB NOT NULL CHECK(length(wrap_nonce)=24 AND data_nonce<>wrap_nonce),
 wrapped_dek BLOB NOT NULL CHECK(length(wrapped_dek)=48),ciphertext BLOB NOT NULL CHECK(length(ciphertext)=80),
 created_at INTEGER NOT NULL CHECK(created_at>=0),expires_at INTEGER NOT NULL CHECK(expires_at>created_at AND expires_at<=created_at+300),
 state TEXT NOT NULL CHECK(state IN ('pending','delivered','consumed','revoked','uncertain')),
 revision INTEGER NOT NULL CHECK(revision>0),consumed_credential TEXT REFERENCES worker_credentials(credential_id),
 UNIQUE(project_id,attempt_id,request_id),UNIQUE(enrollment_id,operation,generation),UNIQUE(key_id,key_version,wrap_nonce),
 FOREIGN KEY(enrollment_id,project_id,attempt_id) REFERENCES worker_enrollments(enrollment_id,project_id,attempt_id),
 FOREIGN KEY(previous_credential,enrollment_id) REFERENCES worker_credentials(credential_id,enrollment_id),
 FOREIGN KEY(consumed_credential,enrollment_id) REFERENCES worker_credentials(credential_id,enrollment_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES aws_attempt_roles(project_id,attempt_id),
 CHECK((operation='bootstrap' AND previous_credential IS NULL AND generation=1) OR (operation='renew' AND previous_credential IS NOT NULL AND generation>1)),
 CHECK((state='consumed')=(consumed_credential IS NOT NULL))
) STRICT;
CREATE INDEX aws_protected_enrollment ON aws_protected_challenges(enrollment_id,state,generation);
CREATE TABLE aws_delivery_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 challenge_id TEXT NOT NULL REFERENCES aws_protected_challenges(challenge_id),
 request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 64),
 kind TEXT NOT NULL CHECK(kind IN ('prepared','metadata','policy','published','publish_requested','delivered','consumed','revoked','uncertain','cleanup_requested','cleanup_observed')),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),reference TEXT NOT NULL CHECK(length(reference) BETWEEN 1 AND 64),
 UNIQUE(challenge_id,request_id,kind,reference)
) STRICT;
CREATE TABLE aws_credential_uses (
 project_id TEXT NOT NULL, account_ref TEXT NOT NULL,consent_generation INTEGER NOT NULL,
 request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 64),
 connection_id TEXT NOT NULL,record_id TEXT NOT NULL,version INTEGER NOT NULL,
 operation TEXT NOT NULL CHECK(operation IN ('verify_controller','inspect_execution','protected_delivery')),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,request_id,operation),
 FOREIGN KEY(project_id,account_ref,consent_generation) REFERENCES aws_account_consents(project_id,account_ref,generation),
 FOREIGN KEY(project_id,connection_id,record_id,version) REFERENCES secret_versions(project_id,connection_id,record_id,version)
) STRICT;
CREATE TRIGGER aws_consent_head_update BEFORE UPDATE ON aws_account_consent_heads
WHEN NEW.project_id!=OLD.project_id OR NEW.account_ref!=OLD.account_ref OR NEW.generation!=OLD.generation+1
BEGIN SELECT RAISE(ABORT,'AWS consent transition rejected'); END;
CREATE TRIGGER aws_protected_transition BEFORE UPDATE ON aws_protected_challenges
WHEN NEW.challenge_id!=OLD.challenge_id OR NEW.project_id!=OLD.project_id OR NEW.attempt_id!=OLD.attempt_id OR NEW.enrollment_id!=OLD.enrollment_id OR NEW.operation!=OLD.operation OR NEW.previous_credential IS NOT OLD.previous_credential OR NEW.generation!=OLD.generation OR NEW.request_id!=OLD.request_id OR NEW.secret_version!=OLD.secret_version OR NEW.record!=OLD.record OR NEW.record_mac!=OLD.record_mac OR NEW.key_id!=OLD.key_id OR NEW.key_version!=OLD.key_version OR NEW.data_nonce!=OLD.data_nonce OR NEW.wrap_nonce!=OLD.wrap_nonce OR NEW.wrapped_dek!=OLD.wrapped_dek OR NEW.ciphertext!=OLD.ciphertext OR NEW.created_at!=OLD.created_at OR NEW.expires_at!=OLD.expires_at OR NEW.revision!=OLD.revision+1 OR NOT ((OLD.state IN ('pending','uncertain') AND NEW.state IN ('delivered','revoked','uncertain')) OR (OLD.state='delivered' AND NEW.state IN ('consumed','revoked','uncertain')))
BEGIN SELECT RAISE(ABORT,'AWS proof lifecycle rejected'); END;
CREATE TRIGGER aws_protected_no_delete BEFORE DELETE ON aws_protected_challenges BEGIN SELECT RAISE(ABORT,'AWS proof retained'); END;
CREATE TRIGGER aws_roles_no_update BEFORE UPDATE ON aws_attempt_roles BEGIN SELECT RAISE(ABORT,'AWS role binding immutable'); END;
CREATE TRIGGER aws_roles_no_delete BEFORE DELETE ON aws_attempt_roles BEGIN SELECT RAISE(ABORT,'AWS role binding retained'); END;
CREATE TRIGGER aws_consent_no_update BEFORE UPDATE ON aws_account_consents BEGIN SELECT RAISE(ABORT,'AWS consent immutable'); END;
CREATE TRIGGER aws_consent_no_delete BEFORE DELETE ON aws_account_consents BEGIN SELECT RAISE(ABORT,'AWS consent retained'); END;
CREATE TRIGGER aws_consent_heads_no_delete BEFORE DELETE ON aws_account_consent_heads BEGIN SELECT RAISE(ABORT,'AWS consent retained'); END;
CREATE TRIGGER aws_proposals_no_update BEFORE UPDATE ON aws_account_proposals BEGIN SELECT RAISE(ABORT,'AWS proposal immutable'); END;
CREATE TRIGGER aws_proposals_no_delete BEFORE DELETE ON aws_account_proposals BEGIN SELECT RAISE(ABORT,'AWS proposal retained'); END;
CREATE TRIGGER aws_controller_no_update BEFORE UPDATE ON aws_controller_checks BEGIN SELECT RAISE(ABORT,'AWS observation immutable'); END;
CREATE TRIGGER aws_controller_no_delete BEFORE DELETE ON aws_controller_checks BEGIN SELECT RAISE(ABORT,'AWS observation retained'); END;
CREATE TRIGGER aws_delivery_no_update BEFORE UPDATE ON aws_delivery_events BEGIN SELECT RAISE(ABORT,'AWS audit append only'); END;
CREATE TRIGGER aws_delivery_no_delete BEFORE DELETE ON aws_delivery_events BEGIN SELECT RAISE(ABORT,'AWS audit retained'); END;
CREATE TRIGGER aws_use_no_update BEFORE UPDATE ON aws_credential_uses BEGIN SELECT RAISE(ABORT,'AWS use append only'); END;
CREATE TRIGGER aws_use_no_delete BEFORE DELETE ON aws_credential_uses BEGIN SELECT RAISE(ABORT,'AWS use retained'); END;
CREATE TABLE aws_delivery_checks (
 project_id TEXT NOT NULL,attempt_id TEXT NOT NULL,request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 64),
 inspection_revision INTEGER NOT NULL CHECK(inspection_revision>0),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record) BETWEEN 1 AND 8192),record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 observed_at INTEGER NOT NULL CHECK(observed_at>=0),expires_at INTEGER NOT NULL CHECK(expires_at>observed_at AND expires_at<=observed_at+15),
 PRIMARY KEY(project_id,attempt_id,request_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES aws_attempt_roles(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id,inspection_revision) REFERENCES aws_task_inspections(project_id,attempt_id,revision)
) STRICT;
CREATE TRIGGER aws_delivery_checks_no_update BEFORE UPDATE ON aws_delivery_checks BEGIN SELECT RAISE(ABORT,'AWS policy evidence immutable'); END;
CREATE TRIGGER aws_delivery_checks_no_delete BEFORE DELETE ON aws_delivery_checks BEGIN SELECT RAISE(ABORT,'AWS policy evidence retained'); END;
CREATE TRIGGER aws_proof_initial BEFORE INSERT ON aws_protected_challenges WHEN NEW.state!='pending' OR NEW.revision!=1 OR NEW.consumed_credential IS NOT NULL BEGIN SELECT RAISE(ABORT,'AWS proof initial state rejected'); END;
