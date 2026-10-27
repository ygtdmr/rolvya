-- Human sessions/factors only. Published migrations and provider authority unchanged.
CREATE TABLE human_sessions (
 session_id TEXT PRIMARY KEY CHECK(length(session_id)=32),
 user_id TEXT NOT NULL REFERENCES human_users(user_id),
 source_evidence TEXT NOT NULL REFERENCES human_authentications(evidence_id),
 security_epoch INTEGER NOT NULL CHECK(security_epoch>0),
 email_version INTEGER NOT NULL CHECK(email_version>0),
 credential_version INTEGER NOT NULL CHECK(credential_version>0),
 policy_version INTEGER NOT NULL CHECK(policy_version>0),
 method TEXT NOT NULL CHECK(method IN ('password','magic_link','otp')),
 assurance TEXT NOT NULL CHECK(assurance IN ('primary','totp','recovery')),
 browser BLOB NOT NULL CHECK(length(browser)=32),
 device TEXT NOT NULL CHECK(length(device) BETWEEN 1 AND 64),
 created_at INTEGER NOT NULL CHECK(created_at>=0),
 absolute_expires INTEGER NOT NULL CHECK(absolute_expires=created_at+7776000),
 last_human_at INTEGER NOT NULL CHECK(last_human_at>=created_at),
 observed_at INTEGER NOT NULL CHECK(observed_at>=last_human_at),
 generation INTEGER NOT NULL CHECK(generation>0),
 revoked_at INTEGER CHECK(revoked_at>=created_at),
 binding BLOB NOT NULL CHECK(length(binding)=32)
) STRICT;
CREATE INDEX human_sessions_user ON human_sessions(user_id,session_id);
CREATE TABLE human_session_tokens (
 lookup BLOB PRIMARY KEY CHECK(length(lookup)=32),
 session_id TEXT NOT NULL REFERENCES human_sessions(session_id),
 generation INTEGER NOT NULL CHECK(generation>0),
 issued_at INTEGER NOT NULL CHECK(issued_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at>issued_at AND expires_at<=issued_at+86400),
 csrf BLOB NOT NULL CHECK(length(csrf)=32),
 binding BLOB NOT NULL CHECK(length(binding)=32),
 UNIQUE(session_id,generation)
) STRICT;
CREATE TABLE human_primary_context (
 evidence_id TEXT PRIMARY KEY REFERENCES human_authentications(evidence_id),
 browser BLOB NOT NULL CHECK(length(browser)=32),
 email_version INTEGER NOT NULL CHECK(email_version>0),
 parent_session TEXT REFERENCES human_sessions(session_id),
 parent_generation INTEGER,
 expires_at INTEGER NOT NULL CHECK(expires_at>=300),
 binding BLOB NOT NULL CHECK(length(binding)=32),
 CHECK((parent_session IS NULL AND parent_generation IS NULL) OR (parent_session IS NOT NULL AND parent_generation>0))
) STRICT;
CREATE TABLE human_primary_uses (
 evidence_id TEXT PRIMARY KEY REFERENCES human_primary_context(evidence_id),
 purpose TEXT NOT NULL CHECK(purpose IN ('session','factor_pending','factor_enrollment','recovery_rotation')),
 used_at INTEGER NOT NULL CHECK(used_at>=0)
) STRICT;
CREATE TABLE human_primary_blocks (
 evidence_id TEXT PRIMARY KEY REFERENCES human_primary_context(evidence_id),
 blocked_at INTEGER NOT NULL CHECK(blocked_at>=0)
) STRICT;
CREATE TABLE human_factors (
 factor_id TEXT PRIMARY KEY CHECK(length(factor_id)=32),
 user_id TEXT NOT NULL REFERENCES human_users(user_id),
 version INTEGER NOT NULL CHECK(version>0),
 state TEXT NOT NULL CHECK(state IN ('pending','active','aborted','revoked')),
 session_id TEXT NOT NULL REFERENCES human_sessions(session_id),
 session_generation INTEGER NOT NULL CHECK(session_generation>0),
 source_evidence TEXT NOT NULL UNIQUE REFERENCES human_primary_uses(evidence_id),
 security_epoch INTEGER NOT NULL CHECK(security_epoch>0),
 created_at INTEGER NOT NULL CHECK(created_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at=created_at+600),
 last_step INTEGER NOT NULL CHECK(last_step>=-1),
 failures INTEGER NOT NULL CHECK(failures BETWEEN 0 AND 5),
 envelope_version INTEGER NOT NULL CHECK(envelope_version=1),
 key_id BLOB NOT NULL CHECK(length(key_id)=16),
 key_version INTEGER NOT NULL CHECK(key_version>0),
 data_nonce BLOB NOT NULL CHECK(length(data_nonce)=24),
 wrap_nonce BLOB NOT NULL CHECK(length(wrap_nonce)=24 AND wrap_nonce!=data_nonce),
 wrapped_dek BLOB NOT NULL CHECK(length(wrapped_dek)=48),
 ciphertext BLOB NOT NULL CHECK(length(ciphertext)=48),
 UNIQUE(user_id,version)
) STRICT;
CREATE UNIQUE INDEX human_factor_current ON human_factors(user_id) WHERE state IN ('pending','active');
CREATE TABLE human_pending_logins (
 pending_id TEXT PRIMARY KEY CHECK(length(pending_id)=32),
 evidence_id TEXT NOT NULL UNIQUE REFERENCES human_primary_uses(evidence_id),
 factor_id TEXT NOT NULL REFERENCES human_factors(factor_id),
 secret_commitment BLOB NOT NULL CHECK(length(secret_commitment)=32),
 state TEXT NOT NULL CHECK(state IN ('pending','consumed','exhausted')),
 failures INTEGER NOT NULL CHECK(failures BETWEEN 0 AND 5)
) STRICT;
CREATE TABLE human_recovery_codes (
 code_id TEXT PRIMARY KEY CHECK(length(code_id)=32),
 factor_id TEXT NOT NULL REFERENCES human_factors(factor_id),
 batch INTEGER NOT NULL CHECK(batch>0),
 commitment BLOB NOT NULL UNIQUE CHECK(length(commitment)=32),
 state TEXT NOT NULL CHECK(state IN ('active','consumed','revoked'))
) STRICT;
CREATE INDEX human_recovery_factor ON human_recovery_codes(factor_id,state);
CREATE TABLE human_factor_attempts (
 attempt_id TEXT PRIMARY KEY,
 user_id TEXT NOT NULL REFERENCES human_users(user_id),
 client_key BLOB NOT NULL CHECK(length(client_key)=32),
 occurred_at INTEGER NOT NULL CHECK(occurred_at>=0),
 succeeded INTEGER NOT NULL CHECK(succeeded IN (0,1))
) STRICT;
CREATE INDEX human_factor_attempt_user ON human_factor_attempts(user_id,occurred_at);
CREATE INDEX human_factor_attempt_client ON human_factor_attempts(client_key,occurred_at);
CREATE TABLE human_session_events (
 sequence INTEGER PRIMARY KEY,
 user_id TEXT NOT NULL REFERENCES human_users(user_id),
 session_id TEXT REFERENCES human_sessions(session_id),
 request_id TEXT NOT NULL,
 event TEXT NOT NULL CHECK(event IN ('issued','rotated','revoked','revoked_all','stale_generation','factor_pending','factor_started','factor_activated','factor_denied','recovery_used','recovery_rotated','incident','expired')),
 occurred_at INTEGER NOT NULL CHECK(occurred_at>=0)
) STRICT;
-- Preserve identity/epoch protections while permitting only an independently
-- confirmed active factor to change the optional requirement from 0 to 1.
DROP TRIGGER human_user_update;
CREATE TRIGGER human_user_update BEFORE UPDATE ON human_users
WHEN NEW.user_id IS NOT OLD.user_id OR NEW.created_at IS NOT OLD.created_at
 OR NEW.security_epoch!=OLD.security_epoch+1 OR NEW.state NOT IN ('active','disabled','compromised')
 OR (NEW.factor_required IS NOT OLD.factor_required AND NOT
  (OLD.factor_required=0 AND NEW.factor_required=1 AND EXISTS
   (SELECT 1 FROM human_factors f WHERE f.user_id=OLD.user_id AND f.state='active' AND f.last_step>=0 AND f.security_epoch=OLD.security_epoch)))
BEGIN SELECT RAISE(ABORT,'invalid human identity transition'); END;
CREATE TRIGGER human_session_update BEFORE UPDATE ON human_sessions
WHEN NEW.session_id IS NOT OLD.session_id OR NEW.user_id IS NOT OLD.user_id OR NEW.source_evidence IS NOT OLD.source_evidence
 OR NEW.security_epoch IS NOT OLD.security_epoch OR NEW.email_version IS NOT OLD.email_version
 OR NEW.credential_version IS NOT OLD.credential_version OR NEW.policy_version IS NOT OLD.policy_version
 OR NEW.method IS NOT OLD.method OR NEW.assurance IS NOT OLD.assurance OR NEW.browser IS NOT OLD.browser OR NEW.device IS NOT OLD.device
 OR NEW.created_at IS NOT OLD.created_at OR NEW.absolute_expires IS NOT OLD.absolute_expires OR NEW.binding IS NOT OLD.binding
 OR NEW.last_human_at<OLD.last_human_at OR NEW.observed_at<OLD.observed_at
 OR NEW.generation NOT IN (OLD.generation,OLD.generation+1)
 OR (OLD.revoked_at IS NOT NULL AND NEW.revoked_at IS NOT OLD.revoked_at)
BEGIN SELECT RAISE(ABORT,'invalid session transition'); END;
CREATE TRIGGER human_session_delete BEFORE DELETE ON human_sessions BEGIN SELECT RAISE(ABORT,'session evidence retained'); END;
CREATE TRIGGER human_session_token_update BEFORE UPDATE ON human_session_tokens BEGIN SELECT RAISE(ABORT,'immutable session token'); END;
CREATE TRIGGER human_session_token_delete BEFORE DELETE ON human_session_tokens BEGIN SELECT RAISE(ABORT,'token evidence retained'); END;
CREATE TRIGGER human_context_update BEFORE UPDATE ON human_primary_context BEGIN SELECT RAISE(ABORT,'immutable primary context'); END;
CREATE TRIGGER human_context_delete BEFORE DELETE ON human_primary_context BEGIN SELECT RAISE(ABORT,'primary context retained'); END;
CREATE TRIGGER human_use_update BEFORE UPDATE ON human_primary_uses BEGIN SELECT RAISE(ABORT,'immutable proof consumption'); END;
CREATE TRIGGER human_use_delete BEFORE DELETE ON human_primary_uses BEGIN SELECT RAISE(ABORT,'proof consumption retained'); END;
CREATE TRIGGER human_block_update BEFORE UPDATE ON human_primary_blocks BEGIN SELECT RAISE(ABORT,'immutable proof block'); END;
CREATE TRIGGER human_block_delete BEFORE DELETE ON human_primary_blocks BEGIN SELECT RAISE(ABORT,'proof block retained'); END;
CREATE TRIGGER human_factor_update BEFORE UPDATE ON human_factors
WHEN NEW.factor_id IS NOT OLD.factor_id OR NEW.user_id IS NOT OLD.user_id OR NEW.version IS NOT OLD.version
 OR NEW.session_id IS NOT OLD.session_id OR NEW.session_generation IS NOT OLD.session_generation OR NEW.source_evidence IS NOT OLD.source_evidence
 OR NEW.security_epoch IS NOT OLD.security_epoch OR NEW.created_at IS NOT OLD.created_at OR NEW.expires_at IS NOT OLD.expires_at
 OR NEW.envelope_version IS NOT OLD.envelope_version OR NEW.key_id IS NOT OLD.key_id OR NEW.key_version IS NOT OLD.key_version
 OR NEW.data_nonce IS NOT OLD.data_nonce OR NEW.wrap_nonce IS NOT OLD.wrap_nonce OR NEW.wrapped_dek IS NOT OLD.wrapped_dek OR NEW.ciphertext IS NOT OLD.ciphertext
 OR NEW.last_step<OLD.last_step OR NEW.failures<OLD.failures
 OR NOT ((OLD.state='pending' AND NEW.state IN ('pending','active','aborted')) OR (OLD.state='active' AND NEW.state IN ('active','revoked')))
BEGIN SELECT RAISE(ABORT,'invalid factor transition'); END;
CREATE TRIGGER human_factor_delete BEFORE DELETE ON human_factors BEGIN SELECT RAISE(ABORT,'factor evidence retained'); END;
CREATE TRIGGER human_pending_update BEFORE UPDATE ON human_pending_logins
WHEN NEW.pending_id IS NOT OLD.pending_id OR NEW.evidence_id IS NOT OLD.evidence_id OR NEW.factor_id IS NOT OLD.factor_id
 OR NEW.secret_commitment IS NOT OLD.secret_commitment OR OLD.state!='pending' OR NEW.state NOT IN ('pending','consumed','exhausted')
 OR NEW.failures<OLD.failures OR NEW.failures>OLD.failures+1
BEGIN SELECT RAISE(ABORT,'invalid pending login transition'); END;
CREATE TRIGGER human_pending_delete BEFORE DELETE ON human_pending_logins BEGIN SELECT RAISE(ABORT,'pending evidence retained'); END;
CREATE TRIGGER human_recovery_update BEFORE UPDATE ON human_recovery_codes
WHEN NEW.code_id IS NOT OLD.code_id OR NEW.factor_id IS NOT OLD.factor_id OR NEW.batch IS NOT OLD.batch OR NEW.commitment IS NOT OLD.commitment
 OR OLD.state!='active' OR NEW.state NOT IN ('consumed','revoked')
BEGIN SELECT RAISE(ABORT,'invalid recovery transition'); END;
CREATE TRIGGER human_recovery_delete BEFORE DELETE ON human_recovery_codes BEGIN SELECT RAISE(ABORT,'recovery evidence retained'); END;
CREATE TRIGGER human_session_event_update BEFORE UPDATE ON human_session_events BEGIN SELECT RAISE(ABORT,'append-only session audit'); END;
CREATE TRIGGER human_session_event_delete BEFORE DELETE ON human_session_events BEGIN SELECT RAISE(ABORT,'append-only session audit'); END;
