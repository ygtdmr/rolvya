-- Only plan persistence/activation. No financial admission, worker or secret tables.
CREATE TABLE projects (
    project_id TEXT PRIMARY KEY NOT NULL,
    owner_actor TEXT NOT NULL,
    draft_plan_id TEXT,
    draft_version INTEGER,
    CHECK ((draft_plan_id IS NULL) = (draft_version IS NULL)),
    FOREIGN KEY (project_id, draft_plan_id, draft_version)
      REFERENCES plan_versions(project_id, plan_id, version) DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE TABLE plan_versions (
    project_id TEXT NOT NULL REFERENCES projects(project_id),
    plan_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version > 0),
    fingerprint TEXT NOT NULL CHECK (length(fingerprint)=64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
    canonical BLOB NOT NULL CHECK (length(canonical) BETWEEN 1 AND 67108864),
    state TEXT NOT NULL CHECK (state IN ('draft','active')),
    activated_event INTEGER UNIQUE REFERENCES activation_events(sequence) DEFERRABLE INITIALLY DEFERRED,
    CHECK ((state='active') = (activated_event IS NOT NULL)),
    PRIMARY KEY (project_id, plan_id, version)
) STRICT;
CREATE UNIQUE INDEX one_active_plan_per_project ON plan_versions(project_id) WHERE state='active';
CREATE TABLE tasks (
    project_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    task_id TEXT NOT NULL,
    parent_task_id TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('group','executable_leaf')),
    definition TEXT NOT NULL CHECK (json_valid(definition)),
    PRIMARY KEY (project_id, plan_id, version, task_id),
    CHECK (parent_task_id IS NULL OR parent_task_id<>task_id),
    FOREIGN KEY (project_id, plan_id, version) REFERENCES plan_versions(project_id, plan_id, version),
    FOREIGN KEY (project_id, plan_id, version, parent_task_id)
      REFERENCES tasks(project_id, plan_id, version, task_id) DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE INDEX tasks_parent ON tasks(project_id,plan_id,version,parent_task_id);
CREATE TABLE task_dependencies (
    project_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    predecessor_id TEXT NOT NULL,
    successor_id TEXT NOT NULL,
    CHECK (predecessor_id<>successor_id),
    PRIMARY KEY (project_id,plan_id,version,predecessor_id,successor_id),
    FOREIGN KEY (project_id,plan_id,version,predecessor_id) REFERENCES tasks(project_id,plan_id,version,task_id),
    FOREIGN KEY (project_id,plan_id,version,successor_id) REFERENCES tasks(project_id,plan_id,version,task_id)
) STRICT;
CREATE INDEX dependencies_successor ON task_dependencies(project_id,plan_id,version,successor_id);
CREATE TABLE task_runtime (
    project_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    task_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('planned','waiting','ready','running','validating','succeeded','failed','canceled')),
    PRIMARY KEY (project_id,plan_id,version,task_id),
    FOREIGN KEY (project_id,plan_id,version,task_id) REFERENCES tasks(project_id,plan_id,version,task_id)
) STRICT;
CREATE TABLE activation_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    fingerprint TEXT NOT NULL,
    actor TEXT NOT NULL,
    request_id TEXT NOT NULL,
    UNIQUE (project_id, request_id),
    UNIQUE (project_id, plan_id, version),
    FOREIGN KEY (project_id,plan_id,version) REFERENCES plan_versions(project_id,plan_id,version)
) STRICT;
CREATE TRIGGER frozen_plan_update BEFORE UPDATE ON plan_versions WHEN OLD.state='active'
BEGIN SELECT RAISE(ABORT,'active plan is immutable'); END;
CREATE TRIGGER frozen_plan_delete BEFORE DELETE ON plan_versions WHEN OLD.state='active'
BEGIN SELECT RAISE(ABORT,'active plan is immutable'); END;
CREATE TRIGGER active_plan_requires_receipt BEFORE UPDATE OF state,activated_event ON plan_versions
WHEN NEW.state='active' AND NOT EXISTS (
 SELECT 1 FROM activation_events WHERE sequence=NEW.activated_event AND project_id=NEW.project_id
 AND plan_id=NEW.plan_id AND version=NEW.version AND fingerprint=NEW.fingerprint)
BEGIN SELECT RAISE(ABORT,'activation requires matching receipt'); END;
CREATE TRIGGER no_insert_active_plan BEFORE INSERT ON plan_versions WHEN NEW.state='active'
BEGIN SELECT RAISE(ABORT,'insert draft before activation'); END;
CREATE TRIGGER receipt_is_draft BEFORE INSERT ON activation_events
WHEN NOT EXISTS (SELECT 1 FROM plan_versions WHERE project_id=NEW.project_id AND plan_id=NEW.plan_id
 AND version=NEW.version AND state='draft' AND fingerprint=NEW.fingerprint)
BEGIN SELECT RAISE(ABORT,'receipt requires matching draft'); END;
CREATE TRIGGER immutable_event_update BEFORE UPDATE ON activation_events
BEGIN SELECT RAISE(ABORT,'activation audit is append only'); END;
CREATE TRIGGER immutable_event_delete BEFORE DELETE ON activation_events
BEGIN SELECT RAISE(ABORT,'activation audit is append only'); END;
CREATE TRIGGER runtime_identity BEFORE UPDATE OF project_id,plan_id,version,task_id ON task_runtime
BEGIN SELECT RAISE(ABORT,'runtime identity is immutable'); END;

CREATE TRIGGER frozen_tasks_insert BEFORE INSERT ON tasks WHEN EXISTS (SELECT 1 FROM plan_versions WHERE project_id=NEW.project_id AND plan_id=NEW.plan_id AND version=NEW.version AND state='active')
BEGIN SELECT RAISE(ABORT,'active plan structure is immutable'); END;

CREATE TRIGGER frozen_tasks_update BEFORE UPDATE ON tasks WHEN EXISTS (SELECT 1 FROM plan_versions WHERE project_id=OLD.project_id AND plan_id=OLD.plan_id AND version=OLD.version AND state='active') OR EXISTS (SELECT 1 FROM plan_versions WHERE project_id=NEW.project_id AND plan_id=NEW.plan_id AND version=NEW.version AND state='active')
BEGIN SELECT RAISE(ABORT,'active plan structure is immutable'); END;

CREATE TRIGGER frozen_tasks_delete BEFORE DELETE ON tasks WHEN EXISTS (SELECT 1 FROM plan_versions WHERE project_id=OLD.project_id AND plan_id=OLD.plan_id AND version=OLD.version AND state='active')
BEGIN SELECT RAISE(ABORT,'active plan structure is immutable'); END;

CREATE TRIGGER frozen_task_dependencies_insert BEFORE INSERT ON task_dependencies WHEN EXISTS (SELECT 1 FROM plan_versions WHERE project_id=NEW.project_id AND plan_id=NEW.plan_id AND version=NEW.version AND state='active')
BEGIN SELECT RAISE(ABORT,'active plan structure is immutable'); END;

CREATE TRIGGER frozen_task_dependencies_update BEFORE UPDATE ON task_dependencies WHEN EXISTS (SELECT 1 FROM plan_versions WHERE project_id=OLD.project_id AND plan_id=OLD.plan_id AND version=OLD.version AND state='active') OR EXISTS (SELECT 1 FROM plan_versions WHERE project_id=NEW.project_id AND plan_id=NEW.plan_id AND version=NEW.version AND state='active')
BEGIN SELECT RAISE(ABORT,'active plan structure is immutable'); END;

CREATE TRIGGER frozen_task_dependencies_delete BEFORE DELETE ON task_dependencies WHEN EXISTS (SELECT 1 FROM plan_versions WHERE project_id=OLD.project_id AND plan_id=OLD.plan_id AND version=OLD.version AND state='active')
BEGIN SELECT RAISE(ABORT,'active plan structure is immutable'); END;

CREATE TRIGGER frozen_task_runtime_insert BEFORE INSERT ON task_runtime WHEN EXISTS (SELECT 1 FROM plan_versions WHERE project_id=NEW.project_id AND plan_id=NEW.plan_id AND version=NEW.version AND state='active')
BEGIN SELECT RAISE(ABORT,'active plan structure is immutable'); END;

CREATE TRIGGER frozen_runtime_delete BEFORE DELETE ON task_runtime
WHEN EXISTS (SELECT 1 FROM plan_versions WHERE project_id=OLD.project_id AND plan_id=OLD.plan_id AND version=OLD.version AND state='active')
BEGIN SELECT RAISE(ABORT,'active runtime identity must be preserved'); END;
CREATE TRIGGER activation_requires_runtime BEFORE UPDATE OF state ON plan_versions
WHEN NEW.state='active' AND (SELECT count(*) FROM task_runtime WHERE project_id=NEW.project_id AND plan_id=NEW.plan_id AND version=NEW.version)
  <> (SELECT count(*) FROM tasks WHERE project_id=NEW.project_id AND plan_id=NEW.plan_id AND version=NEW.version)
BEGIN SELECT RAISE(ABORT,'activation requires complete initial runtime'); END;
