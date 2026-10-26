-- Ciphertext authority only. No plaintext credential/key or general-purpose export.
CREATE TABLE secret_connections (
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 connection_id TEXT NOT NULL CHECK(length(CAST(connection_id AS BLOB)) BETWEEN 1 AND 64),
 account_id TEXT NOT NULL CHECK(length(CAST(account_id AS BLOB)) BETWEEN 1 AND 64),
 provider TEXT NOT NULL CHECK(length(CAST(provider AS BLOB)) BETWEEN 1 AND 64),
 mode TEXT NOT NULL CHECK(mode IN ('api_key','cli_subscription')),
 revision INTEGER NOT NULL CHECK(revision IN (1,2)),
 state TEXT NOT NULL CHECK(state IN ('active','revoked')),
 CHECK((revision=1)=(state='active')),
 PRIMARY KEY(project_id,connection_id),
 UNIQUE(project_id,connection_id,account_id,provider,mode)
) STRICT;
CREATE TABLE secret_records (
 project_id TEXT NOT NULL, connection_id TEXT NOT NULL, record_id TEXT NOT NULL CHECK(length(CAST(record_id AS BLOB)) BETWEEN 1 AND 64),
 purpose TEXT NOT NULL CHECK(length(CAST(purpose AS BLOB)) BETWEEN 1 AND 64),
 current_version INTEGER NOT NULL CHECK(current_version>0),
 PRIMARY KEY(project_id,connection_id,record_id),
 UNIQUE(project_id,connection_id,record_id,purpose),
 FOREIGN KEY(project_id,connection_id) REFERENCES secret_connections(project_id,connection_id),
 FOREIGN KEY(project_id,connection_id,record_id,current_version) REFERENCES secret_versions(project_id,connection_id,record_id,version) DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE TABLE secret_versions (
 project_id TEXT NOT NULL, connection_id TEXT NOT NULL, record_id TEXT NOT NULL, purpose TEXT NOT NULL,
 account_id TEXT NOT NULL, provider TEXT NOT NULL, mode TEXT NOT NULL,
 version INTEGER NOT NULL CHECK(version>0),
 envelope_version INTEGER NOT NULL CHECK(envelope_version=1),
 cipher INTEGER NOT NULL CHECK(cipher=1),
 installation_id BLOB NOT NULL CHECK(length(installation_id)=16),
 origin_key_id BLOB NOT NULL CHECK(length(origin_key_id)=16),
 origin_key_version INTEGER NOT NULL CHECK(origin_key_version>0),
 wrap_key_id BLOB NOT NULL CHECK(length(wrap_key_id)=16),
 wrap_key_version INTEGER NOT NULL CHECK(wrap_key_version>0),
 data_nonce BLOB NOT NULL CHECK(length(data_nonce)=24),
 wrap_nonce BLOB NOT NULL CHECK(length(wrap_nonce)=24 AND wrap_nonce<>data_nonce),
 wrapped_dek BLOB NOT NULL CHECK(length(wrapped_dek)=48),
 ciphertext BLOB NOT NULL CHECK(length(ciphertext) BETWEEN 16 AND 65552),
 PRIMARY KEY(project_id,connection_id,record_id,version),
 UNIQUE(wrap_key_id,wrap_key_version,wrap_nonce),
 FOREIGN KEY(project_id,connection_id,record_id,purpose) REFERENCES secret_records(project_id,connection_id,record_id,purpose),
 FOREIGN KEY(project_id,connection_id,account_id,provider,mode) REFERENCES secret_connections(project_id,connection_id,account_id,provider,mode)
) STRICT;
CREATE TABLE secret_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 project_id TEXT NOT NULL REFERENCES projects(project_id), request_id TEXT NOT NULL CHECK(length(CAST(request_id AS BLOB)) BETWEEN 1 AND 64),
 connection_id TEXT NOT NULL, record_id TEXT NOT NULL,
 action INTEGER NOT NULL CHECK(action BETWEEN 1 AND 8),
 outcome TEXT NOT NULL CHECK(outcome IN ('accepted','denied','integrity_failed')),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 UNIQUE(project_id,request_id,action,outcome),
 UNIQUE(sequence,project_id,request_id)
) STRICT;
CREATE TABLE secret_requests (
 project_id TEXT NOT NULL, request_id TEXT NOT NULL,
 commitment BLOB NOT NULL CHECK(length(commitment)=32),
 commitment_key_id BLOB NOT NULL CHECK(length(commitment_key_id)=16),
 commitment_key_version INTEGER NOT NULL CHECK(commitment_key_version>0),
 event_sequence INTEGER NOT NULL UNIQUE,
 result_version INTEGER NOT NULL CHECK(result_version>0),
 PRIMARY KEY(project_id,request_id),
 FOREIGN KEY(project_id) REFERENCES projects(project_id),
 FOREIGN KEY(event_sequence,project_id,request_id) REFERENCES secret_events(sequence,project_id,request_id)
) STRICT;
CREATE INDEX secret_versions_key ON secret_versions(wrap_key_id,wrap_key_version);
CREATE TRIGGER secret_connection_transition BEFORE UPDATE ON secret_connections WHEN
 NEW.project_id<>OLD.project_id OR NEW.connection_id<>OLD.connection_id OR NEW.account_id<>OLD.account_id
 OR NEW.provider<>OLD.provider OR NEW.mode<>OLD.mode OR OLD.state<>'active' OR NEW.state<>'revoked'
 OR NEW.revision<>OLD.revision+1
BEGIN SELECT RAISE(ABORT,'invalid connection transition'); END;
CREATE TRIGGER secret_record_transition BEFORE UPDATE ON secret_records WHEN
 NEW.project_id<>OLD.project_id OR NEW.connection_id<>OLD.connection_id OR NEW.record_id<>OLD.record_id
 OR NEW.purpose<>OLD.purpose OR NEW.current_version<>OLD.current_version+1
 OR NOT EXISTS(SELECT 1 FROM secret_connections c WHERE c.project_id=OLD.project_id AND c.connection_id=OLD.connection_id AND c.state='active')
BEGIN SELECT RAISE(ABORT,'invalid secret head transition'); END;
CREATE TRIGGER secret_version_active BEFORE INSERT ON secret_versions WHEN NOT EXISTS (
 SELECT 1 FROM secret_connections c WHERE c.project_id=NEW.project_id AND c.connection_id=NEW.connection_id AND c.state='active')
BEGIN SELECT RAISE(ABORT,'connection revoked'); END;
CREATE TRIGGER secret_connection_no_delete BEFORE DELETE ON secret_connections BEGIN SELECT RAISE(ABORT,'preserve secret lifecycle'); END;
CREATE TRIGGER secret_record_no_delete BEFORE DELETE ON secret_records BEGIN SELECT RAISE(ABORT,'preserve secret lifecycle'); END;
CREATE TRIGGER secret_version_no_update BEFORE UPDATE ON secret_versions BEGIN SELECT RAISE(ABORT,'immutable encrypted version'); END;
CREATE TRIGGER secret_version_no_delete BEFORE DELETE ON secret_versions BEGIN SELECT RAISE(ABORT,'preserve encrypted history'); END;
CREATE TRIGGER secret_event_no_update BEFORE UPDATE ON secret_events BEGIN SELECT RAISE(ABORT,'append only audit'); END;
CREATE TRIGGER secret_event_no_delete BEFORE DELETE ON secret_events BEGIN SELECT RAISE(ABORT,'append only audit'); END;
CREATE TRIGGER secret_request_no_update BEFORE UPDATE ON secret_requests BEGIN SELECT RAISE(ABORT,'immutable replay identity'); END;
CREATE TRIGGER secret_request_no_delete BEFORE DELETE ON secret_requests BEGIN SELECT RAISE(ABORT,'preserve replay identity'); END;
