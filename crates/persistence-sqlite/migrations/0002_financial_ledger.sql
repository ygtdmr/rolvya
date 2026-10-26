-- R04 financial authority only. No scheduler admission or external dispatch.
CREATE TABLE financial_accounts (
    project_id TEXT PRIMARY KEY REFERENCES projects(project_id),
    currency TEXT NOT NULL CHECK (length(currency)=3 AND currency NOT GLOB '*[^A-Z]*')
) STRICT;
CREATE TABLE financial_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL REFERENCES financial_accounts(project_id),
    request_id TEXT NOT NULL,
    actor TEXT NOT NULL,
    recorded_at INTEGER NOT NULL CHECK (recorded_at>=0),
    kind TEXT NOT NULL CHECK (kind IN ('approval','reserve','report','reconcile')),
    payload TEXT NOT NULL CHECK (length(payload) BETWEEN 1 AND 16384 AND json_valid(payload)),
    billing_evidence TEXT,
    UNIQUE (project_id,billing_evidence),
    UNIQUE (project_id,request_id),
    UNIQUE (sequence,project_id)
) STRICT;
CREATE INDEX financial_history ON financial_events(project_id,sequence);
CREATE TABLE budget_approvals (
    event_sequence INTEGER PRIMARY KEY,
    project_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version>0),
    previous_ceiling INTEGER NOT NULL CHECK (previous_ceiling>=0),
    new_ceiling INTEGER NOT NULL CHECK (new_ceiling>=previous_ceiling),
    proof_id TEXT NOT NULL UNIQUE,
    proof_verified_at INTEGER NOT NULL CHECK (proof_verified_at>=0),
    UNIQUE (project_id,version),
    FOREIGN KEY (event_sequence,project_id) REFERENCES financial_events(sequence,project_id)
) STRICT;
CREATE TABLE liability_revisions (
    event_sequence INTEGER PRIMARY KEY,
    project_id TEXT NOT NULL,
    liability_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version>0),
    uncertain INTEGER NOT NULL CHECK (uncertain IN (0,1)),
    finalized INTEGER NOT NULL CHECK (finalized IN (0,1) AND NOT (finalized=1 AND uncertain=1)),
    UNIQUE (project_id,liability_id,version),
    FOREIGN KEY (event_sequence,project_id) REFERENCES financial_events(sequence,project_id)
) STRICT;
CREATE TABLE financial_positions (
    event_sequence INTEGER NOT NULL REFERENCES liability_revisions(event_sequence),
    category TEXT NOT NULL CHECK (category IN ('ai_api','compute','storage','transfer')),
    settled_minor INTEGER NOT NULL CHECK (settled_minor>=0),
    held_minor INTEGER NOT NULL CHECK (held_minor>=0),
    provisional_minor INTEGER NOT NULL CHECK (provisional_minor>=0),
    PRIMARY KEY (event_sequence,category)
) STRICT;
CREATE TRIGGER approval_order BEFORE INSERT ON budget_approvals WHEN
 NEW.version<>COALESCE((SELECT max(version)+1 FROM budget_approvals WHERE project_id=NEW.project_id),1)
 OR NEW.previous_ceiling<>COALESCE((SELECT new_ceiling FROM budget_approvals WHERE project_id=NEW.project_id ORDER BY version DESC LIMIT 1),0)
 OR (NEW.version>1 AND NEW.new_ceiling<=NEW.previous_ceiling)
 OR NOT EXISTS (SELECT 1 FROM financial_events WHERE sequence=NEW.event_sequence AND project_id=NEW.project_id AND kind='approval')
BEGIN SELECT RAISE(ABORT,'invalid approval transition'); END;
CREATE TRIGGER liability_order BEFORE INSERT ON liability_revisions WHEN
 NEW.version<>COALESCE((SELECT max(version)+1 FROM liability_revisions WHERE project_id=NEW.project_id AND liability_id=NEW.liability_id),1)
 OR NOT EXISTS (SELECT 1 FROM financial_events WHERE sequence=NEW.event_sequence AND project_id=NEW.project_id AND kind IN ('reserve','report','reconcile'))
BEGIN SELECT RAISE(ABORT,'invalid liability transition'); END;
CREATE TRIGGER immutable_financial_accounts_update BEFORE UPDATE ON financial_accounts
BEGIN SELECT RAISE(ABORT,'financial history is append only'); END;
CREATE TRIGGER immutable_financial_accounts_delete BEFORE DELETE ON financial_accounts
BEGIN SELECT RAISE(ABORT,'financial history is append only'); END;
CREATE TRIGGER immutable_financial_events_update BEFORE UPDATE ON financial_events
BEGIN SELECT RAISE(ABORT,'financial history is append only'); END;
CREATE TRIGGER immutable_financial_events_delete BEFORE DELETE ON financial_events
BEGIN SELECT RAISE(ABORT,'financial history is append only'); END;
CREATE TRIGGER immutable_budget_approvals_update BEFORE UPDATE ON budget_approvals
BEGIN SELECT RAISE(ABORT,'financial history is append only'); END;
CREATE TRIGGER immutable_budget_approvals_delete BEFORE DELETE ON budget_approvals
BEGIN SELECT RAISE(ABORT,'financial history is append only'); END;
CREATE TRIGGER immutable_liability_revisions_update BEFORE UPDATE ON liability_revisions
BEGIN SELECT RAISE(ABORT,'financial history is append only'); END;
CREATE TRIGGER immutable_liability_revisions_delete BEFORE DELETE ON liability_revisions
BEGIN SELECT RAISE(ABORT,'financial history is append only'); END;
CREATE TRIGGER immutable_financial_positions_update BEFORE UPDATE ON financial_positions
BEGIN SELECT RAISE(ABORT,'financial history is append only'); END;
CREATE TRIGGER immutable_financial_positions_delete BEFORE DELETE ON financial_positions
BEGIN SELECT RAISE(ABORT,'financial history is append only'); END;
