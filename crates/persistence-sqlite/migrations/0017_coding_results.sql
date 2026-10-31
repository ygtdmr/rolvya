-- Coding evidence only. task_acceptances remains the sole acceptance authority.
CREATE TABLE coding_sources (
 project_id TEXT NOT NULL, plan_id TEXT NOT NULL, version INTEGER NOT NULL, task_id TEXT NOT NULL,
 context BLOB NOT NULL CHECK(length(context) BETWEEN 1 AND 32768),
 source BLOB NOT NULL CHECK(length(source) BETWEEN 1 AND 2097152),
 binding_version INTEGER NOT NULL CHECK(binding_version>0),
 seal BLOB NOT NULL CHECK(length(seal)=32),
 PRIMARY KEY(project_id,plan_id,version,task_id),
 FOREIGN KEY(project_id,plan_id,version,task_id) REFERENCES tasks(project_id,plan_id,version,task_id),
 FOREIGN KEY(project_id,binding_version) REFERENCES git_bindings(project_id,version)
) STRICT;
CREATE TABLE coding_artifacts (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 content BLOB NOT NULL CHECK(length(content) BETWEEN 1 AND 65536),
 sha256 TEXT NOT NULL CHECK(length(sha256)=64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id)
) STRICT;
CREATE TABLE coding_verifications (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 artifact_sha256 TEXT NOT NULL CHECK(length(artifact_sha256)=64),
 evidence BLOB NOT NULL CHECK(length(evidence) BETWEEN 1 AND 32768),
 seal BLOB NOT NULL CHECK(length(seal)=32),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES coding_artifacts(project_id,attempt_id)
) STRICT;
CREATE TABLE coding_decisions (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 validation_id TEXT NOT NULL,
 manifest_sha256 TEXT NOT NULL CHECK(length(manifest_sha256)=64),
 seal BLOB NOT NULL CHECK(length(seal)=32),
 PRIMARY KEY(project_id,attempt_id),
 FOREIGN KEY(project_id,validation_id) REFERENCES synthetic_validations(project_id,validation_id),
 FOREIGN KEY(project_id,attempt_id) REFERENCES coding_verifications(project_id,attempt_id)
) STRICT;
CREATE TRIGGER coding_sources_update BEFORE UPDATE ON coding_sources BEGIN SELECT RAISE(ABORT,'immutable coding source'); END;
CREATE TRIGGER coding_sources_delete BEFORE DELETE ON coding_sources BEGIN SELECT RAISE(ABORT,'preserve coding source'); END;
CREATE TRIGGER coding_artifacts_update BEFORE UPDATE ON coding_artifacts BEGIN SELECT RAISE(ABORT,'immutable coding artifact'); END;
CREATE TRIGGER coding_artifacts_delete BEFORE DELETE ON coding_artifacts BEGIN SELECT RAISE(ABORT,'preserve coding artifact'); END;
CREATE TRIGGER coding_verifications_update BEFORE UPDATE ON coding_verifications BEGIN SELECT RAISE(ABORT,'immutable coding verification'); END;
CREATE TRIGGER coding_verifications_delete BEFORE DELETE ON coding_verifications BEGIN SELECT RAISE(ABORT,'preserve coding verification'); END;
CREATE TRIGGER coding_decisions_update BEFORE UPDATE ON coding_decisions BEGIN SELECT RAISE(ABORT,'immutable coding decision'); END;
CREATE TRIGGER coding_decisions_delete BEFORE DELETE ON coding_decisions BEGIN SELECT RAISE(ABORT,'preserve coding decision'); END;
CREATE TABLE coding_rejections (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 15),
 phase TEXT NOT NULL CHECK(phase IN ('artifact','verification')),
 reason TEXT NOT NULL CHECK(reason IN ('invalid','unavailable','scope','resource','dependency','storage')),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,attempt_id,ordinal),
 FOREIGN KEY(project_id,attempt_id) REFERENCES task_attempts(project_id,attempt_id)
) STRICT;
CREATE TRIGGER coding_rejections_update BEFORE UPDATE ON coding_rejections BEGIN SELECT RAISE(ABORT,'preserve redacted coding rejection'); END;
CREATE TRIGGER coding_rejections_delete BEFORE DELETE ON coding_rejections BEGIN SELECT RAISE(ABORT,'preserve redacted coding rejection'); END;
