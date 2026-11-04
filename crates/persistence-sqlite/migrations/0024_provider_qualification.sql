-- Evidence only: existing attempts, financial positions and approvals remain authoritative.
CREATE TABLE provider_qualification_evidence (
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 request_id TEXT NOT NULL,
 scope_sha256 TEXT NOT NULL CHECK(length(scope_sha256)=64),
 domain TEXT NOT NULL,
 record TEXT NOT NULL CHECK(length(record)<=16384 AND json_valid(record)),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 observed_at INTEGER NOT NULL CHECK(observed_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at>observed_at AND expires_at-observed_at<=86400),
 PRIMARY KEY(project_id,request_id)
) STRICT;
CREATE INDEX provider_qualification_scope ON provider_qualification_evidence(project_id,scope_sha256,domain,observed_at);
CREATE TABLE provider_qualification_revocations (
 project_id TEXT NOT NULL,
 evidence_request TEXT NOT NULL,
 request_id TEXT NOT NULL,
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 record_mac BLOB NOT NULL CHECK(length(record_mac)=32),
 PRIMARY KEY(project_id,evidence_request),
 UNIQUE(project_id,request_id),
 FOREIGN KEY(project_id,evidence_request) REFERENCES provider_qualification_evidence(project_id,request_id)
) STRICT;
CREATE TRIGGER provider_qualification_evidence_no_update BEFORE UPDATE ON provider_qualification_evidence
BEGIN SELECT RAISE(ABORT,'immutable qualification evidence'); END;
CREATE TRIGGER provider_qualification_evidence_no_delete BEFORE DELETE ON provider_qualification_evidence
BEGIN SELECT RAISE(ABORT,'immutable qualification evidence'); END;
CREATE TRIGGER provider_qualification_revocations_no_update BEFORE UPDATE ON provider_qualification_revocations
BEGIN SELECT RAISE(ABORT,'immutable qualification revocation'); END;
CREATE TRIGGER provider_qualification_revocations_no_delete BEFORE DELETE ON provider_qualification_revocations
BEGIN SELECT RAISE(ABORT,'immutable qualification revocation'); END;
