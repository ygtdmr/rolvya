-- Host operations are not human roles, provider grants or a financial journal.
CREATE TABLE controller_operator_requests (
 request_id TEXT PRIMARY KEY CHECK(length(request_id)=64),
 kind TEXT NOT NULL CHECK(kind IN ('setup','enrollment','binding','key_export')),
 payload TEXT NOT NULL CHECK(length(payload)<=16384 AND json_valid(payload)),
 commitment BLOB NOT NULL CHECK(length(commitment)=32),
 service_uid INTEGER NOT NULL CHECK(service_uid>0),
 created_at INTEGER NOT NULL CHECK(created_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at>created_at AND expires_at<=created_at+600)
) STRICT;
CREATE TABLE controller_operation_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 request_id TEXT NOT NULL CHECK(length(request_id) BETWEEN 1 AND 64),
 kind TEXT NOT NULL CHECK(kind IN ('setup_started','setup_verified','enrollment_prepared','enrollment_verified','binding_prepared','backup_prepared','backup_verified','backup_cleanup_started','backup_retired','key_export_prepared','key_export_verified','restore_paused','key_recovered')),
 commitment BLOB NOT NULL CHECK(length(commitment)=32),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 UNIQUE(request_id,kind)
) STRICT;
CREATE TABLE controller_backup_points (
 request_id TEXT PRIMARY KEY CHECK(length(request_id)=64),
 class TEXT NOT NULL CHECK(class IN ('frequent','daily')),
 manifest TEXT NOT NULL CHECK(length(manifest)<=131072 AND json_valid(manifest)),
 envelope_sha256 TEXT NOT NULL CHECK(length(envelope_sha256)=64),
 destination TEXT NOT NULL CHECK(length(destination)=64),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0)
) STRICT;
CREATE TABLE controller_restore_holds (
 restore_id TEXT PRIMARY KEY CHECK(length(restore_id)=64),
 backup_id TEXT NOT NULL CHECK(length(backup_id)=64),
 manifest_sha256 TEXT NOT NULL CHECK(length(manifest_sha256)=64),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0)
) STRICT;
CREATE TABLE controller_mail_operations (
 operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=32),
 message_sha256 TEXT NOT NULL CHECK(length(message_sha256)=64),
 provider_id TEXT NOT NULL CHECK(length(provider_id) BETWEEN 1 AND 128),
 purpose TEXT NOT NULL CHECK(purpose IN ('login','verify_email','owner_action')),
 created_at INTEGER NOT NULL CHECK(created_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at>created_at AND expires_at<=created_at+600)
) STRICT;
CREATE TABLE controller_mail_delivery (
 delivery_id TEXT PRIMARY KEY CHECK(length(delivery_id)=64),
 operation_id TEXT NOT NULL UNIQUE REFERENCES controller_mail_operations(operation_id),
 evidence TEXT NOT NULL CHECK(length(evidence)<=8192 AND json_valid(evidence)),
 signature BLOB NOT NULL CHECK(length(signature)=64),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0)
) STRICT;
CREATE INDEX controller_operation_kind ON controller_operation_events(kind,sequence);
CREATE INDEX controller_backup_time ON controller_backup_points(class,recorded_at);
CREATE TRIGGER controller_operator_update BEFORE UPDATE ON controller_operator_requests BEGIN SELECT RAISE(ABORT,'immutable host request'); END;
CREATE TRIGGER controller_operator_delete BEFORE DELETE ON controller_operator_requests BEGIN SELECT RAISE(ABORT,'retained host request'); END;
CREATE TRIGGER controller_operation_update BEFORE UPDATE ON controller_operation_events BEGIN SELECT RAISE(ABORT,'immutable host evidence'); END;
CREATE TRIGGER controller_operation_delete BEFORE DELETE ON controller_operation_events BEGIN SELECT RAISE(ABORT,'retained host evidence'); END;
CREATE TRIGGER controller_backup_update BEFORE UPDATE ON controller_backup_points BEGIN SELECT RAISE(ABORT,'immutable recovery point'); END;
CREATE TRIGGER controller_backup_delete BEFORE DELETE ON controller_backup_points BEGIN SELECT RAISE(ABORT,'retained recovery point'); END;
CREATE TRIGGER controller_restore_update BEFORE UPDATE ON controller_restore_holds BEGIN SELECT RAISE(ABORT,'restore requires reconciliation'); END;
CREATE TRIGGER controller_restore_delete BEFORE DELETE ON controller_restore_holds BEGIN SELECT RAISE(ABORT,'restore requires reconciliation'); END;
CREATE TRIGGER controller_mail_update BEFORE UPDATE ON controller_mail_operations BEGIN SELECT RAISE(ABORT,'immutable mail operation'); END;
CREATE TRIGGER controller_mail_delete BEFORE DELETE ON controller_mail_operations BEGIN SELECT RAISE(ABORT,'retained mail operation'); END;
CREATE TRIGGER controller_delivery_update BEFORE UPDATE ON controller_mail_delivery BEGIN SELECT RAISE(ABORT,'immutable mail delivery'); END;
CREATE TRIGGER controller_delivery_delete BEFORE DELETE ON controller_mail_delivery BEGIN SELECT RAISE(ABORT,'retained mail delivery'); END;

-- Complete asynchronous Owner mail only after independently verified durable delivery.
-- All original field/transition protections remain; migration 0010 is unchanged.
DROP TRIGGER human_owner_challenge_guard;
CREATE TRIGGER human_owner_challenge_guard BEFORE UPDATE ON human_owner_challenges
WHEN NEW.challenge_id IS NOT OLD.challenge_id OR NEW.action_id IS NOT OLD.action_id OR NEW.kind IS NOT OLD.kind
 OR NEW.browser IS NOT OLD.browser OR NEW.issued_at IS NOT OLD.issued_at OR NEW.expires_at IS NOT OLD.expires_at OR NEW.client_key IS NOT OLD.client_key OR NEW.commitment IS NOT OLD.commitment OR NEW.request_id IS NOT OLD.request_id
 OR NEW.failures<OLD.failures OR NEW.failures>OLD.failures+1
 OR NOT ((OLD.state='queued' AND NEW.state='dispatching') OR (OLD.state='dispatching' AND NEW.state IN ('accepted','delivered','uncertain','failed')) OR (OLD.state='accepted' AND NEW.state='delivered' AND EXISTS(SELECT 1 FROM controller_mail_delivery d JOIN controller_mail_operations m ON m.operation_id=d.operation_id WHERE m.operation_id=OLD.challenge_id AND m.purpose='owner_action')) OR (OLD.state='delivered' AND NEW.state IN ('delivered','consumed','exhausted')))
BEGIN SELECT RAISE(ABORT,'invalid Owner challenge transition'); END;

-- Nonsecret independent identity proof provenance, not another authentication ledger.
CREATE TABLE controller_operator_evidence (
 request_id TEXT PRIMARY KEY REFERENCES controller_operator_requests(request_id),
 proof TEXT NOT NULL CHECK(length(proof)<=8192 AND json_valid(proof)),
 verifier BLOB NOT NULL CHECK(length(verifier)=32),
 evidence_sha256 TEXT NOT NULL CHECK(length(evidence_sha256)=64),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0)
) STRICT;
CREATE TRIGGER controller_evidence_update BEFORE UPDATE ON controller_operator_evidence BEGIN SELECT RAISE(ABORT,'immutable operator proof'); END;
CREATE TRIGGER controller_evidence_delete BEFORE DELETE ON controller_operator_evidence BEGIN SELECT RAISE(ABORT,'retained operator proof'); END;
