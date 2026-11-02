-- Indexed high-water reads for authenticated panel invalidations. Existing
-- append-only journals remain the sole source; no parallel event/session ledger.
CREATE INDEX panel_scheduler_revision ON scheduler_events(project_id,sequence DESC);
CREATE INDEX panel_governance_revision ON human_governance_events(project_id,sequence DESC);
CREATE INDEX panel_activation_revision ON activation_events(project_id,sequence DESC);

-- A foreground gesture can confirm only a fresh intent for its current session
-- generation. No raw intent or session bearer is stored here. This is not a new
-- session issuer or a claim of independently attested physical presence.
CREATE TABLE human_browser_interactions (
 commitment BLOB PRIMARY KEY CHECK(length(commitment)=32),
 token_lookup BLOB NOT NULL REFERENCES human_session_tokens(lookup),
 issued_at INTEGER NOT NULL CHECK(issued_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at=issued_at+60),
 consumed_at INTEGER CHECK(consumed_at>=issued_at AND consumed_at<expires_at)
) STRICT;
CREATE INDEX human_browser_interaction_expiry ON human_browser_interactions(expires_at);
CREATE INDEX human_browser_interaction_token ON human_browser_interactions(token_lookup,expires_at);
CREATE TRIGGER human_browser_interaction_update BEFORE UPDATE ON human_browser_interactions
WHEN NEW.commitment IS NOT OLD.commitment OR NEW.token_lookup IS NOT OLD.token_lookup
 OR NEW.issued_at IS NOT OLD.issued_at OR NEW.expires_at IS NOT OLD.expires_at
 OR OLD.consumed_at IS NOT NULL OR NEW.consumed_at IS NULL
BEGIN SELECT RAISE(ABORT,'invalid browser interaction transition'); END;
