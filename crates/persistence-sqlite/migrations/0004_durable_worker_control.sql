-- R08: controller projections + immutable receipts; synthetic provider is a simulation.
CREATE TABLE worker_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('claim','prepare','observation','message','recovery','compact')),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 payload TEXT NOT NULL CHECK(json_valid(payload) AND length(payload) BETWEEN 1 AND 32768),
 UNIQUE(sequence,project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id)
) STRICT;
CREATE TABLE worker_leases (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 lease_until INTEGER NOT NULL CHECK(lease_until>=0),
 observed_at INTEGER NOT NULL CHECK(observed_at>=0),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id)
) STRICT;
INSERT INTO worker_leases SELECT project_id,attempt_id,lease_until,admitted_at FROM task_attempts;
CREATE TRIGGER initialize_worker_lease AFTER INSERT ON task_attempts
BEGIN INSERT INTO worker_leases VALUES(NEW.project_id,NEW.attempt_id,NEW.lease_until,NEW.admitted_at); END;
CREATE TRIGGER worker_lease_monotonic BEFORE UPDATE ON worker_leases WHEN
 NEW.project_id<>OLD.project_id OR NEW.attempt_id<>OLD.attempt_id OR NEW.observed_at<OLD.observed_at
 OR NEW.lease_until<OLD.lease_until OR NEW.lease_until>max(OLD.lease_until,NEW.observed_at+90)
 OR NEW.lease_until>(SELECT json_extract(e.payload,'$.request.permit.valid_until') FROM task_attempts a JOIN scheduler_events e ON e.sequence=a.admission_event WHERE a.project_id=NEW.project_id AND a.attempt_id=NEW.attempt_id)
BEGIN SELECT RAISE(ABORT,'invalid lease authority'); END;
CREATE TRIGGER worker_lease_no_delete BEFORE DELETE ON worker_leases BEGIN SELECT RAISE(ABORT,'preserve lease authority'); END;
CREATE INDEX expired_worker_leases ON worker_leases(project_id,lease_until,attempt_id);
CREATE TABLE dispatch_operations (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 command TEXT NOT NULL CHECK(json_valid(command) AND length(command) BETWEEN 1 AND 16384),
 digest TEXT NOT NULL CHECK(length(digest)=64 AND digest NOT GLOB '*[^0-9a-f]*'),
 state TEXT NOT NULL CHECK(state IN ('claimed','launching','reconcile','active','finished','no_effect','blocked')),
 revision INTEGER NOT NULL CHECK(revision>=0),
 event_sequence INTEGER NOT NULL,
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES dispatch_claims(project_id,attempt_id),
 FOREIGN KEY(event_sequence,project_id,attempt_id) REFERENCES worker_events(sequence,project_id,attempt_id)
) STRICT;
CREATE TRIGGER dispatch_operation_transition BEFORE UPDATE ON dispatch_operations WHEN
 NEW.project_id<>OLD.project_id OR NEW.attempt_id<>OLD.attempt_id OR NEW.command<>OLD.command OR NEW.digest<>OLD.digest
 OR NEW.revision<>OLD.revision+1 OR NEW.state='claimed'
 OR (NEW.state='launching' AND OLD.state<>'claimed')
 OR (OLD.state IN ('finished','no_effect') AND NEW.state NOT IN (OLD.state,'blocked'))
 OR (OLD.state='blocked' AND NEW.state NOT IN ('blocked','finished','no_effect'))
BEGIN SELECT RAISE(ABORT,'invalid dispatch transition'); END;
CREATE TRIGGER dispatch_operation_no_delete BEFORE DELETE ON dispatch_operations BEGIN SELECT RAISE(ABORT,'preserve dispatch identity'); END;
CREATE TABLE worker_sessions (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 state TEXT NOT NULL CHECK(json_valid(state) AND length(state) BETWEEN 1 AND 16384),
 last_sequence INTEGER NOT NULL CHECK(last_sequence>=0),
 event_sequence INTEGER NOT NULL,
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES dispatch_operations(project_id,attempt_id),
 FOREIGN KEY(event_sequence,project_id,attempt_id) REFERENCES worker_events(sequence,project_id,attempt_id)
) STRICT;
CREATE TRIGGER worker_session_sequence BEFORE UPDATE ON worker_sessions WHEN
 NEW.project_id<>OLD.project_id OR NEW.attempt_id<>OLD.attempt_id OR NEW.last_sequence<>OLD.last_sequence+1
BEGIN SELECT RAISE(ABORT,'invalid message sequence'); END;
CREATE TRIGGER worker_session_no_delete BEFORE DELETE ON worker_sessions BEGIN SELECT RAISE(ABORT,'preserve worker session'); END;
CREATE TABLE worker_messages (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL, message_id TEXT NOT NULL,
 sequence INTEGER NOT NULL CHECK(sequence>0),
 digest TEXT NOT NULL CHECK(length(digest)=64 AND digest NOT GLOB '*[^0-9a-f]*'),
 event_sequence INTEGER NOT NULL UNIQUE,
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,attempt_id,message_id), UNIQUE(project_id,attempt_id,sequence),
 FOREIGN KEY(project_id,attempt_id) REFERENCES worker_sessions(project_id,attempt_id),
 FOREIGN KEY(event_sequence,project_id,attempt_id) REFERENCES worker_events(sequence,project_id,attempt_id)
) STRICT;
CREATE TABLE worker_receipts (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL, message_id TEXT NOT NULL,
 response TEXT NOT NULL CHECK(json_valid(response) AND length(response) BETWEEN 1 AND 8192),
 PRIMARY KEY(project_id,attempt_id,message_id),
 FOREIGN KEY(project_id,attempt_id,message_id) REFERENCES worker_messages(project_id,attempt_id,message_id)
) STRICT;
CREATE TABLE worker_compactions (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL, message_id TEXT NOT NULL,
 event_sequence INTEGER NOT NULL,
 PRIMARY KEY(project_id,attempt_id,message_id),
 FOREIGN KEY(project_id,attempt_id,message_id) REFERENCES worker_messages(project_id,attempt_id,message_id),
 FOREIGN KEY(event_sequence,project_id,attempt_id) REFERENCES worker_events(sequence,project_id,attempt_id)
) STRICT;
CREATE TRIGGER worker_receipt_no_update BEFORE UPDATE ON worker_receipts BEGIN SELECT RAISE(ABORT,'immutable receipt'); END;
CREATE TRIGGER worker_receipt_retention BEFORE DELETE ON worker_receipts WHEN NOT EXISTS(
 SELECT 1 FROM worker_compactions c WHERE c.project_id=OLD.project_id AND c.attempt_id=OLD.attempt_id AND c.message_id=OLD.message_id)
BEGIN SELECT RAISE(ABORT,'permissioned retention required'); END;
CREATE INDEX worker_message_retention ON worker_messages(project_id,recorded_at,attempt_id,message_id);
CREATE TABLE dispatch_observations (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL, evidence_id TEXT NOT NULL,
 digest TEXT NOT NULL CHECK(length(digest)=64),
 provider_revision INTEGER NOT NULL CHECK(provider_revision>=0),
 event_sequence INTEGER NOT NULL,
 PRIMARY KEY(project_id,attempt_id,evidence_id), UNIQUE(project_id,attempt_id,provider_revision),
 FOREIGN KEY(project_id,attempt_id) REFERENCES dispatch_operations(project_id,attempt_id),
 FOREIGN KEY(event_sequence,project_id,attempt_id) REFERENCES worker_events(sequence,project_id,attempt_id)
) STRICT;
CREATE TABLE worker_recoveries (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 next_epoch INTEGER NOT NULL CHECK(next_epoch>0),
 reason TEXT NOT NULL CHECK(reason IN ('expired','risk','revoked','canceled','fenced')),
 event_sequence INTEGER NOT NULL,
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id),
 FOREIGN KEY(event_sequence,project_id,attempt_id) REFERENCES worker_events(sequence,project_id,attempt_id)
) STRICT;
-- A deterministic remote-provider stand-in. Separate transactions deliberately model
-- effects outside controller transactions. Never evidence of real cloud enforcement.
CREATE TABLE synthetic_operations (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 digest TEXT NOT NULL CHECK(length(digest)=64),
 state TEXT NOT NULL CHECK(state IN ('active','finished','rejected','cancel_ack','unknown')),
 unknown_exposure INTEGER NOT NULL CHECK(unknown_exposure IN (0,1)),
 revision INTEGER NOT NULL CHECK(revision>=0),
 evidence_id TEXT NOT NULL,
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES dispatch_operations(project_id,attempt_id)
) STRICT;
CREATE TRIGGER immutable_worker_events_update BEFORE UPDATE ON worker_events BEGIN SELECT RAISE(ABORT,'worker evidence is append only'); END;
CREATE TRIGGER immutable_worker_events_delete BEFORE DELETE ON worker_events BEGIN SELECT RAISE(ABORT,'worker evidence is append only'); END;
CREATE TRIGGER immutable_worker_messages_update BEFORE UPDATE ON worker_messages BEGIN SELECT RAISE(ABORT,'worker evidence is append only'); END;
CREATE TRIGGER immutable_worker_messages_delete BEFORE DELETE ON worker_messages BEGIN SELECT RAISE(ABORT,'worker evidence is append only'); END;
CREATE TRIGGER immutable_worker_compactions_update BEFORE UPDATE ON worker_compactions BEGIN SELECT RAISE(ABORT,'worker evidence is append only'); END;
CREATE TRIGGER immutable_worker_compactions_delete BEFORE DELETE ON worker_compactions BEGIN SELECT RAISE(ABORT,'worker evidence is append only'); END;
CREATE TRIGGER immutable_dispatch_observations_update BEFORE UPDATE ON dispatch_observations BEGIN SELECT RAISE(ABORT,'worker evidence is append only'); END;
CREATE TRIGGER immutable_dispatch_observations_delete BEFORE DELETE ON dispatch_observations BEGIN SELECT RAISE(ABORT,'worker evidence is append only'); END;
CREATE TRIGGER immutable_worker_recoveries_update BEFORE UPDATE ON worker_recoveries BEGIN SELECT RAISE(ABORT,'worker evidence is append only'); END;
CREATE TRIGGER immutable_worker_recoveries_delete BEFORE DELETE ON worker_recoveries BEGIN SELECT RAISE(ABORT,'worker evidence is append only'); END;
