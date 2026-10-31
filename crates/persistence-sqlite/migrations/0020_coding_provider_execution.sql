-- Provider effect evidence extends the existing coding attempt, never a new ledger.
CREATE TABLE coding_agent_calls (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 step INTEGER NOT NULL CHECK(step BETWEEN 1 AND 8),
 body BLOB NOT NULL CHECK(length(body) BETWEEN 1 AND 4096),
 seal BLOB NOT NULL CHECK(length(seal)=32),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,attempt_id,step),
 FOREIGN KEY(project_id,attempt_id) REFERENCES coding_work(project_id,attempt_id)
) STRICT;
CREATE TABLE coding_agent_observations (
 project_id TEXT NOT NULL, attempt_id TEXT NOT NULL,
 step INTEGER NOT NULL CHECK(step BETWEEN 1 AND 8),
 body BLOB NOT NULL CHECK(length(body) BETWEEN 1 AND 4096),
 seal BLOB NOT NULL CHECK(length(seal)=32),
 recorded_at INTEGER NOT NULL CHECK(recorded_at>=0),
 PRIMARY KEY(project_id,attempt_id,step),
 FOREIGN KEY(project_id,attempt_id,step) REFERENCES coding_agent_calls(project_id,attempt_id,step)
) STRICT;
CREATE TRIGGER coding_agent_calls_update BEFORE UPDATE ON coding_agent_calls BEGIN SELECT RAISE(ABORT,'immutable provider operation'); END;
CREATE TRIGGER coding_agent_calls_delete BEFORE DELETE ON coding_agent_calls BEGIN SELECT RAISE(ABORT,'retained provider operation'); END;
CREATE TRIGGER coding_agent_observations_update BEFORE UPDATE ON coding_agent_observations BEGIN SELECT RAISE(ABORT,'immutable provider observation'); END;
CREATE TRIGGER coding_agent_observations_delete BEFORE DELETE ON coding_agent_observations BEGIN SELECT RAISE(ABORT,'retained provider observation'); END;
