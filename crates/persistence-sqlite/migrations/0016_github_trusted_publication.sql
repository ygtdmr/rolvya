-- G1-01: Owner-consented remote identities; encrypted credentials remain in Broker.
CREATE TABLE git_binding_proposals (
 proposal_id TEXT PRIMARY KEY CHECK(length(proposal_id)=32), project_id TEXT NOT NULL,
 actor TEXT NOT NULL, record TEXT NOT NULL CHECK(json_valid(record) AND length(record)<=32768),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32), expires_at INTEGER NOT NULL,
 FOREIGN KEY(project_id) REFERENCES projects(project_id), FOREIGN KEY(actor) REFERENCES human_users(user_id)
) STRICT;
CREATE TABLE git_bindings (
 project_id TEXT NOT NULL,version INTEGER NOT NULL CHECK(version>0),
 repository_id INTEGER NOT NULL CHECK(repository_id>0),installation_id INTEGER NOT NULL CHECK(installation_id>0),
 proposal_id TEXT NOT NULL UNIQUE,actor TEXT NOT NULL,role_version INTEGER NOT NULL,security_epoch INTEGER NOT NULL,
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record)<=32768),record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 PRIMARY KEY(project_id,version), FOREIGN KEY(project_id) REFERENCES projects(project_id),
 FOREIGN KEY(proposal_id) REFERENCES git_binding_proposals(proposal_id), FOREIGN KEY(actor) REFERENCES human_users(user_id),
 CHECK(json_extract(record,'$.binding.project')=project_id), CHECK(json_extract(record,'$.version')=version),
 CHECK(json_extract(record,'$.binding.repository')=repository_id),CHECK(json_extract(record,'$.binding.installation')=installation_id)
) STRICT;
CREATE TABLE git_binding_heads (
 project_id TEXT PRIMARY KEY,version INTEGER NOT NULL,
 FOREIGN KEY(project_id,version) REFERENCES git_bindings(project_id,version)
) STRICT;
CREATE TABLE git_publications (
 project_id TEXT NOT NULL,attempt_id TEXT NOT NULL,operation_id TEXT NOT NULL CHECK(length(operation_id)=64),
 binding_version INTEGER NOT NULL,repository_id INTEGER NOT NULL,target_ref TEXT NOT NULL,
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record)<=2097152),record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 PRIMARY KEY(project_id,operation_id),UNIQUE(project_id,attempt_id),UNIQUE(repository_id,target_ref),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id),
 FOREIGN KEY(project_id,binding_version) REFERENCES git_bindings(project_id,version),
 CHECK(json_extract(record,'$.lease.attempt_id')=attempt_id),CHECK(json_extract(record,'$.target')=target_ref),
 CHECK(json_extract(record,'$.repository')=repository_id),CHECK(json_extract(record,'$.operation')=operation_id)
) STRICT;
CREATE TABLE git_publication_events (
 project_id TEXT NOT NULL,operation_id TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0),
 kind TEXT NOT NULL CHECK(kind IN ('prepared','submitting','published','unknown','blocked','pr_prepared','pr_submitting','pr_created','pr_unknown')),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record)<=4096),record_mac BLOB NOT NULL CHECK(length(record_mac)=32),recorded_at INTEGER NOT NULL,
 PRIMARY KEY(project_id,operation_id,revision),FOREIGN KEY(project_id,operation_id) REFERENCES git_publications(project_id,operation_id)
) STRICT;
CREATE TRIGGER git_event_sequence BEFORE INSERT ON git_publication_events
WHEN NEW.revision!=coalesce((SELECT max(revision) FROM git_publication_events WHERE project_id=NEW.project_id AND operation_id=NEW.operation_id),0)+1
BEGIN SELECT RAISE(ABORT,'Git event revision rejected'); END;
CREATE TABLE git_webhook_deliveries (
 project_id TEXT NOT NULL,binding_version INTEGER NOT NULL,delivery_id TEXT NOT NULL CHECK(length(delivery_id) BETWEEN 1 AND 100),
 body_sha256 TEXT NOT NULL CHECK(length(body_sha256)=64),decision TEXT NOT NULL CHECK(decision IN ('unrelated','recheck','revoked')),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),recorded_at INTEGER NOT NULL,
 PRIMARY KEY(project_id,binding_version,delivery_id),FOREIGN KEY(project_id,binding_version) REFERENCES git_bindings(project_id,version)
) STRICT;
CREATE INDEX git_webhook_digest ON git_webhook_deliveries(project_id,body_sha256);
CREATE INDEX git_webhook_restriction ON git_webhook_deliveries(project_id,binding_version,decision);
CREATE TABLE git_credential_uses (
 sequence INTEGER PRIMARY KEY,project_id TEXT NOT NULL,binding_version INTEGER NOT NULL,
 purpose TEXT NOT NULL CHECK(purpose IN ('read','publication','pull_request','webhook')),recorded_at INTEGER NOT NULL,
 FOREIGN KEY(project_id,binding_version) REFERENCES git_bindings(project_id,version)
) STRICT;
CREATE TRIGGER git_proposal_no_update BEFORE UPDATE ON git_binding_proposals BEGIN SELECT RAISE(ABORT,'Git proposal immutable'); END;
CREATE TRIGGER git_proposal_no_delete BEFORE DELETE ON git_binding_proposals BEGIN SELECT RAISE(ABORT,'Git proposal retained'); END;
CREATE TRIGGER git_binding_no_update BEFORE UPDATE ON git_bindings BEGIN SELECT RAISE(ABORT,'Git binding immutable'); END;
CREATE TRIGGER git_binding_no_delete BEFORE DELETE ON git_bindings BEGIN SELECT RAISE(ABORT,'Git binding retained'); END;
CREATE TRIGGER git_head_version BEFORE UPDATE ON git_binding_heads WHEN NEW.project_id!=OLD.project_id OR NEW.version!=OLD.version+1 BEGIN SELECT RAISE(ABORT,'Git binding version rejected'); END;
CREATE TRIGGER git_head_no_delete BEFORE DELETE ON git_binding_heads BEGIN SELECT RAISE(ABORT,'Git binding retained'); END;
CREATE TRIGGER git_publication_no_update BEFORE UPDATE ON git_publications BEGIN SELECT RAISE(ABORT,'Git intent immutable'); END;
CREATE TRIGGER git_publication_no_delete BEFORE DELETE ON git_publications BEGIN SELECT RAISE(ABORT,'Git intent retained'); END;
CREATE TRIGGER git_event_no_update BEFORE UPDATE ON git_publication_events BEGIN SELECT RAISE(ABORT,'Git event immutable'); END;
CREATE TRIGGER git_event_no_delete BEFORE DELETE ON git_publication_events BEGIN SELECT RAISE(ABORT,'Git event retained'); END;
CREATE TRIGGER git_webhook_no_update BEFORE UPDATE ON git_webhook_deliveries BEGIN SELECT RAISE(ABORT,'Git delivery immutable'); END;
CREATE TRIGGER git_webhook_no_delete BEFORE DELETE ON git_webhook_deliveries BEGIN SELECT RAISE(ABORT,'Git delivery retained'); END;
CREATE TRIGGER git_credential_no_update BEFORE UPDATE ON git_credential_uses BEGIN SELECT RAISE(ABORT,'Git audit immutable'); END;
CREATE TRIGGER git_credential_no_delete BEFORE DELETE ON git_credential_uses BEGIN SELECT RAISE(ABORT,'Git audit retained'); END;
