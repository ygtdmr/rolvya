-- Provider metadata only; protected credential bytes remain in the Secret Broker.
CREATE TABLE provider_connections (
 connection_id TEXT PRIMARY KEY CHECK(length(CAST(connection_id AS BLOB)) BETWEEN 1 AND 64), owner_id TEXT REFERENCES human_users(user_id),
 provider TEXT NOT NULL CHECK(length(CAST(provider AS BLOB)) BETWEEN 1 AND 64), account TEXT NOT NULL CHECK(length(CAST(account AS BLOB)) BETWEEN 1 AND 64),
 mode TEXT NOT NULL CHECK(mode IN ('api_key','cli_session')),
 version INTEGER NOT NULL CHECK(version>0),
 UNIQUE(provider,account,mode)
) STRICT;
CREATE TABLE provider_connection_versions (
 connection_id TEXT NOT NULL REFERENCES provider_connections(connection_id),
 version INTEGER NOT NULL CHECK(version>0), record TEXT NOT NULL CHECK(json_valid(record) AND length(record)<=16384),
 mac BLOB NOT NULL CHECK(length(mac)=32), PRIMARY KEY(connection_id,version)
) STRICT;
CREATE TABLE provider_project_grant_versions (
 connection_id TEXT NOT NULL REFERENCES provider_connections(connection_id),
 project_id TEXT NOT NULL REFERENCES projects(project_id), version INTEGER NOT NULL CHECK(version>0),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record)<=16384), mac BLOB NOT NULL CHECK(length(mac)=32),
 PRIMARY KEY(connection_id,project_id,version)
) STRICT;
CREATE TABLE provider_project_grants (
 connection_id TEXT NOT NULL REFERENCES provider_connections(connection_id),
 project_id TEXT NOT NULL REFERENCES projects(project_id), version INTEGER NOT NULL CHECK(version>0),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record)<=16384), mac BLOB NOT NULL CHECK(length(mac)=32),
 PRIMARY KEY(connection_id,project_id),
 FOREIGN KEY(connection_id,project_id,version) REFERENCES provider_project_grant_versions(connection_id,project_id,version) DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE INDEX provider_project_connections ON provider_project_grants(project_id,connection_id);
CREATE TABLE provider_change_proposals (
 proposal_id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(project_id),
 actor TEXT NOT NULL REFERENCES human_users(user_id),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record)<=16384),
 mac BLOB NOT NULL CHECK(length(mac)=32), consumed INTEGER NOT NULL CHECK(consumed IN(0,1))
) STRICT;
CREATE TABLE provider_policy_revisions (
 provider TEXT NOT NULL, mode TEXT NOT NULL CHECK(mode IN ('api_key','cli_session')),
 revision TEXT NOT NULL, version INTEGER NOT NULL CHECK(version>0), PRIMARY KEY(provider,mode)
) STRICT;
CREATE TABLE provider_capacity_reservations (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 connection_id TEXT NOT NULL, connection_version INTEGER NOT NULL,
 grant_version INTEGER NOT NULL CHECK(grant_version>0), quota_window TEXT NOT NULL,
 quota_units INTEGER NOT NULL CHECK(quota_units>0), state TEXT NOT NULL CHECK(state IN ('held','released')),
 record TEXT NOT NULL CHECK(json_valid(record) AND length(record)<=8192), mac BLOB NOT NULL CHECK(length(mac)=32),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id),
 FOREIGN KEY(connection_id,connection_version) REFERENCES provider_connection_versions(connection_id,version),
 FOREIGN KEY(connection_id,project_id,grant_version) REFERENCES provider_project_grant_versions(connection_id,project_id,version)
) STRICT;
CREATE INDEX provider_account_capacity ON provider_capacity_reservations(connection_id,state,quota_window);
CREATE INDEX provider_quota_usage ON provider_capacity_reservations(connection_id,quota_window);
CREATE TABLE provider_security_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, project_id TEXT NOT NULL REFERENCES projects(project_id),
 connection_id TEXT NOT NULL, request_id TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN('consent','verified','revoked','reserved','released','blocked','refresh_begin','refresh_end')),
 reason TEXT NOT NULL, recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 UNIQUE(project_id,connection_id,request_id,kind)
) STRICT;
CREATE TABLE provider_refresh_locks (
 connection_id TEXT NOT NULL REFERENCES provider_connections(connection_id),
 operation_id TEXT NOT NULL UNIQUE, expected_version INTEGER NOT NULL CHECK(expected_version>0),
 state TEXT NOT NULL CHECK(state IN('in_progress','reconciled')),
 expires_at INTEGER NOT NULL CHECK(expires_at>=0),
 PRIMARY KEY(connection_id,operation_id)
) STRICT;
CREATE UNIQUE INDEX provider_one_refresh ON provider_refresh_locks(connection_id) WHERE state='in_progress';
CREATE TRIGGER provider_connection_identity BEFORE UPDATE ON provider_connections WHEN
 NEW.connection_id<>OLD.connection_id OR NEW.owner_id IS NOT OLD.owner_id OR NEW.provider<>OLD.provider OR NEW.account<>OLD.account OR NEW.mode<>OLD.mode OR NEW.version<>OLD.version+1
 OR NOT EXISTS(SELECT 1 FROM provider_connection_versions v WHERE v.connection_id=NEW.connection_id AND v.version=NEW.version)
BEGIN SELECT RAISE(ABORT,'versioned immutable provider identity'); END;
CREATE TRIGGER provider_connection_no_delete BEFORE DELETE ON provider_connections BEGIN SELECT RAISE(ABORT,'preserve provider account history'); END;
CREATE TRIGGER provider_version_initial BEFORE INSERT ON provider_connection_versions WHEN NEW.version<>(SELECT version+1 FROM provider_connections WHERE connection_id=NEW.connection_id) AND NEW.version<>1 BEGIN SELECT RAISE(ABORT,'next provider version required'); END;
CREATE TRIGGER provider_version_no_update BEFORE UPDATE ON provider_connection_versions BEGIN SELECT RAISE(ABORT,'immutable provider evidence'); END;
CREATE TRIGGER provider_version_no_delete BEFORE DELETE ON provider_connection_versions BEGIN SELECT RAISE(ABORT,'preserve provider evidence'); END;
CREATE TRIGGER provider_grant_guard BEFORE UPDATE ON provider_project_grants WHEN NEW.connection_id<>OLD.connection_id OR NEW.project_id<>OLD.project_id OR NEW.version<>OLD.version+1 OR NOT EXISTS(SELECT 1 FROM provider_project_grant_versions v WHERE v.connection_id=NEW.connection_id AND v.project_id=NEW.project_id AND v.version=NEW.version AND v.record=NEW.record AND v.mac=NEW.mac) BEGIN SELECT RAISE(ABORT,'versioned provider project grant'); END;
CREATE TRIGGER provider_grant_no_delete BEFORE DELETE ON provider_project_grants BEGIN SELECT RAISE(ABORT,'preserve provider grant'); END;
CREATE TRIGGER provider_grant_version_next BEFORE INSERT ON provider_project_grant_versions WHEN NEW.version<>coalesce((SELECT version+1 FROM provider_project_grants WHERE connection_id=NEW.connection_id AND project_id=NEW.project_id),1) BEGIN SELECT RAISE(ABORT,'next provider grant version required'); END;
CREATE TRIGGER provider_grant_version_no_update BEFORE UPDATE ON provider_project_grant_versions BEGIN SELECT RAISE(ABORT,'immutable scoped entitlement history'); END;
CREATE TRIGGER provider_grant_version_no_delete BEFORE DELETE ON provider_project_grant_versions BEGIN SELECT RAISE(ABORT,'preserve scoped entitlement history'); END;
CREATE TRIGGER provider_proposal_guard BEFORE UPDATE ON provider_change_proposals WHEN NEW.proposal_id<>OLD.proposal_id OR NEW.project_id<>OLD.project_id OR NEW.actor<>OLD.actor OR NEW.record<>OLD.record OR NEW.mac<>OLD.mac OR OLD.consumed<>0 OR NEW.consumed<>1 BEGIN SELECT RAISE(ABORT,'one-use provider proposal'); END;
CREATE TRIGGER provider_proposal_no_delete BEFORE DELETE ON provider_change_proposals BEGIN SELECT RAISE(ABORT,'preserve provider consent'); END;
CREATE TRIGGER provider_policy_guard BEFORE UPDATE ON provider_policy_revisions WHEN NEW.provider<>OLD.provider OR NEW.mode<>OLD.mode OR NEW.version<=OLD.version BEGIN SELECT RAISE(ABORT,'versioned provider policy'); END;
CREATE TRIGGER provider_policy_no_delete BEFORE DELETE ON provider_policy_revisions BEGIN SELECT RAISE(ABORT,'preserve provider policy'); END;
CREATE TRIGGER provider_capacity_initial BEFORE INSERT ON provider_capacity_reservations WHEN NEW.state<>'held' BEGIN SELECT RAISE(ABORT,'held provider capacity required'); END;
CREATE TRIGGER provider_capacity_guard BEFORE UPDATE ON provider_capacity_reservations WHEN NEW.project_id<>OLD.project_id OR NEW.attempt_id<>OLD.attempt_id OR NEW.connection_id<>OLD.connection_id OR NEW.connection_version<>OLD.connection_version OR NEW.grant_version<>OLD.grant_version OR NEW.quota_window<>OLD.quota_window OR NEW.quota_units<>OLD.quota_units OR NEW.record<>OLD.record OR NEW.mac<>OLD.mac OR OLD.state<>'held' OR NEW.state<>'released' BEGIN SELECT RAISE(ABORT,'one-way reconciled provider capacity release'); END;
CREATE TRIGGER provider_capacity_no_delete BEFORE DELETE ON provider_capacity_reservations BEGIN SELECT RAISE(ABORT,'preserve provider attribution'); END;
CREATE TRIGGER provider_event_no_update BEFORE UPDATE ON provider_security_events BEGIN SELECT RAISE(ABORT,'append-only provider security evidence'); END;
CREATE TRIGGER provider_event_no_delete BEFORE DELETE ON provider_security_events BEGIN SELECT RAISE(ABORT,'preserve provider security evidence'); END;
CREATE TRIGGER provider_refresh_guard BEFORE UPDATE ON provider_refresh_locks WHEN NEW.connection_id<>OLD.connection_id OR NEW.operation_id<>OLD.operation_id OR NEW.expected_version<>OLD.expected_version OR NEW.expires_at<>OLD.expires_at OR OLD.state<>'in_progress' OR NEW.state<>'reconciled' BEGIN SELECT RAISE(ABORT,'reconcile provider refresh'); END;
CREATE TRIGGER provider_refresh_no_delete BEFORE DELETE ON provider_refresh_locks BEGIN SELECT RAISE(ABORT,'preserve refresh uncertainty'); END;

CREATE TABLE provider_cli_homes(connection_id TEXT PRIMARY KEY REFERENCES provider_connections(connection_id),home_identity TEXT NOT NULL UNIQUE CHECK(length(home_identity) BETWEEN 1 AND 64)) STRICT;
CREATE TRIGGER provider_cli_home_no_update BEFORE UPDATE ON provider_cli_homes BEGIN SELECT RAISE(ABORT,'immutable isolated account home'); END;
CREATE TRIGGER provider_cli_home_no_delete BEFORE DELETE ON provider_cli_homes BEGIN SELECT RAISE(ABORT,'preserve account isolation identity'); END;
