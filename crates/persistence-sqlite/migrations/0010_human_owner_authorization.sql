-- Human project authority is explicit; legacy owner_actor and ledger stay intact.
CREATE TABLE human_project_bindings (
 project_id TEXT PRIMARY KEY REFERENCES projects(project_id),
 initial_user TEXT NOT NULL REFERENCES human_users(user_id),
 legacy_actor TEXT NOT NULL,
 request_id TEXT NOT NULL UNIQUE,
 established_at INTEGER NOT NULL CHECK(established_at>=0),
 binding BLOB NOT NULL CHECK(length(binding)=32)
) STRICT;
CREATE TABLE human_memberships (
 project_id TEXT NOT NULL REFERENCES human_project_bindings(project_id),
 user_id TEXT NOT NULL REFERENCES human_users(user_id),
 role TEXT NOT NULL CHECK(role IN ('owner','editor','viewer')),
 state TEXT NOT NULL CHECK(state IN ('active','revoked')),
 version INTEGER NOT NULL CHECK(version>0),
 PRIMARY KEY(project_id,user_id)
) STRICT;
CREATE INDEX human_memberships_user ON human_memberships(user_id,project_id);
CREATE TABLE human_membership_history (
 project_id TEXT NOT NULL,
 user_id TEXT NOT NULL,
 version INTEGER NOT NULL CHECK(version>0),
 role TEXT NOT NULL CHECK(role IN ('owner','editor','viewer')),
 state TEXT NOT NULL CHECK(state IN ('active','revoked')),
 reference TEXT NOT NULL CHECK(length(reference)<=128),
 occurred_at INTEGER NOT NULL CHECK(occurred_at>=0),
 PRIMARY KEY(project_id,user_id,version),
 FOREIGN KEY(project_id,user_id) REFERENCES human_memberships(project_id,user_id)
) STRICT;
CREATE TRIGGER human_history_current BEFORE INSERT ON human_membership_history
WHEN NOT EXISTS(SELECT 1 FROM human_memberships m WHERE m.project_id=NEW.project_id AND m.user_id=NEW.user_id AND m.version=NEW.version AND m.role=NEW.role AND m.state=NEW.state)
BEGIN SELECT RAISE(ABORT,'history must describe current membership'); END;
CREATE TRIGGER human_history_update BEFORE UPDATE ON human_membership_history BEGIN SELECT RAISE(ABORT,'immutable membership history'); END;
CREATE TRIGGER human_history_delete BEFORE DELETE ON human_membership_history BEGIN SELECT RAISE(ABORT,'membership history retained'); END;
CREATE TABLE human_owner_actions (
 action_id TEXT PRIMARY KEY CHECK(length(action_id)=32),
 project_id TEXT NOT NULL REFERENCES human_project_bindings(project_id),
 actor TEXT NOT NULL REFERENCES human_users(user_id),
 session_id TEXT NOT NULL REFERENCES human_sessions(session_id),
 generation INTEGER NOT NULL CHECK(generation>0),
 role_version INTEGER NOT NULL CHECK(role_version>0),
 security_epoch INTEGER NOT NULL CHECK(security_epoch>0),
 email_version INTEGER NOT NULL CHECK(email_version>0),
 credential_version INTEGER NOT NULL CHECK(credential_version>0),
 policy_version INTEGER NOT NULL CHECK(policy_version>0),
 payload TEXT NOT NULL CHECK(length(payload)<=2048),
 digest BLOB NOT NULL CHECK(length(digest)=32),
 created_at INTEGER NOT NULL CHECK(created_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at=created_at+1800),
 request_id TEXT NOT NULL UNIQUE,
 state TEXT NOT NULL CHECK(state IN ('pending','committed','revoked','held')),
 binding BLOB NOT NULL CHECK(length(binding)=32)
) STRICT;
CREATE INDEX human_owner_actions_session ON human_owner_actions(session_id,state);
CREATE INDEX human_owner_actions_actor ON human_owner_actions(project_id,actor,state);
CREATE TABLE human_owner_proofs (
 action_id TEXT PRIMARY KEY REFERENCES human_owner_actions(action_id),
 method TEXT NOT NULL CHECK(method IN ('password','magic_link','otp')),
 credential_version INTEGER NOT NULL CHECK(credential_version>0),
 factor_id TEXT REFERENCES human_factors(factor_id),
 factor_step INTEGER,
 verified_at INTEGER NOT NULL CHECK(verified_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at=verified_at+300),
 binding BLOB NOT NULL CHECK(length(binding)=32),
 CHECK((factor_id IS NULL AND factor_step IS NULL) OR (factor_id IS NOT NULL AND factor_step IS NOT NULL AND factor_step>=0))
) STRICT;
CREATE TABLE human_owner_acceptances (
 action_id TEXT PRIMARY KEY REFERENCES human_owner_actions(action_id),
 recipient TEXT NOT NULL REFERENCES human_users(user_id),
 session_lookup BLOB NOT NULL REFERENCES human_session_tokens(lookup),
 accepted_at INTEGER NOT NULL CHECK(accepted_at>=0),
 binding BLOB NOT NULL CHECK(length(binding)=32)
) STRICT;
CREATE TABLE human_owner_effects (
 action_id TEXT PRIMARY KEY REFERENCES human_owner_proofs(action_id),
 financial_sequence INTEGER UNIQUE REFERENCES financial_events(sequence),
 committed_at INTEGER NOT NULL CHECK(committed_at>=0)
) STRICT;
CREATE TABLE human_project_invitations (
 invitation_id TEXT PRIMARY KEY CHECK(length(invitation_id)=32),
 project_id TEXT NOT NULL REFERENCES human_project_bindings(project_id),
 issuer TEXT NOT NULL REFERENCES human_users(user_id),
 issuer_role_version INTEGER NOT NULL CHECK(issuer_role_version>0),
 issuer_session TEXT NOT NULL REFERENCES human_sessions(session_id),
 issuer_security_epoch INTEGER NOT NULL CHECK(issuer_security_epoch>0),
 recipient TEXT NOT NULL REFERENCES human_users(user_id),
 email_version INTEGER NOT NULL CHECK(email_version>0),
 security_epoch INTEGER NOT NULL CHECK(security_epoch>0),
 target_version INTEGER NOT NULL CHECK(target_version>=0),
 role TEXT NOT NULL CHECK(role IN ('editor','viewer')),
 issued_at INTEGER NOT NULL CHECK(issued_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at=issued_at+86400),
 request_id TEXT NOT NULL UNIQUE,
 commitment BLOB NOT NULL CHECK(length(commitment)=32),
 binding BLOB NOT NULL CHECK(length(binding)=32),
 version INTEGER NOT NULL CHECK(version>0),
 state TEXT NOT NULL CHECK(state IN ('pending','consumed','revoked'))
) STRICT;
CREATE INDEX human_project_invitations_project ON human_project_invitations(project_id,state,invitation_id);
CREATE TABLE human_owner_challenges (
 challenge_id TEXT PRIMARY KEY CHECK(length(challenge_id)=32),
 action_id TEXT NOT NULL REFERENCES human_owner_actions(action_id),
 kind TEXT NOT NULL CHECK(kind IN ('magic_link','otp')),
 browser BLOB NOT NULL CHECK(length(browser)=32),
 issued_at INTEGER NOT NULL CHECK(issued_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at=issued_at+600),
 client_key BLOB NOT NULL CHECK(length(client_key)=32),
 commitment BLOB NOT NULL CHECK(length(commitment)=32),
 failures INTEGER NOT NULL DEFAULT 0 CHECK(failures BETWEEN 0 AND 5),
 state TEXT NOT NULL CHECK(state IN ('queued','dispatching','accepted','delivered','uncertain','failed','consumed','exhausted')),
 request_id TEXT NOT NULL UNIQUE
) STRICT;
CREATE INDEX human_owner_challenges_action ON human_owner_challenges(action_id,issued_at);
CREATE INDEX human_owner_challenges_client ON human_owner_challenges(client_key,issued_at);
CREATE TABLE human_governance_events (
 sequence INTEGER PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES human_project_bindings(project_id),
 actor TEXT NOT NULL REFERENCES human_users(user_id),
 request_id TEXT NOT NULL,
 event TEXT NOT NULL CHECK(length(event) BETWEEN 1 AND 40),
 reference TEXT NOT NULL CHECK(length(reference)<=128),
 occurred_at INTEGER NOT NULL CHECK(occurred_at>=0),
 UNIQUE(project_id,request_id,event)
) STRICT;
CREATE TABLE human_owner_notifications (
 notification_id INTEGER PRIMARY KEY,
 event_sequence INTEGER NOT NULL REFERENCES human_governance_events(sequence),
 recipient TEXT NOT NULL REFERENCES human_users(user_id),
 UNIQUE(event_sequence,recipient)
) STRICT;
CREATE INDEX human_owner_notifications_recipient ON human_owner_notifications(recipient,notification_id);
-- Retain all authority/effect/audit history; mutable lifecycle is one-way.
CREATE TRIGGER human_membership_guard BEFORE UPDATE ON human_memberships
WHEN NEW.project_id IS NOT OLD.project_id OR NEW.user_id IS NOT OLD.user_id OR NEW.version!=OLD.version+1
 OR (OLD.role='owner' AND OLD.state='active' AND (NEW.role!='owner' OR NEW.state!='active') AND NOT EXISTS
 (SELECT 1 FROM human_memberships m JOIN human_users u USING(user_id) WHERE m.project_id=OLD.project_id AND m.user_id!=OLD.user_id AND m.role='owner' AND m.state='active' AND u.state='active'))
BEGIN SELECT RAISE(ABORT,'invalid membership or last Owner transition'); END;
CREATE TRIGGER human_owner_action_guard BEFORE UPDATE ON human_owner_actions
WHEN NEW.action_id IS NOT OLD.action_id OR NEW.project_id IS NOT OLD.project_id OR NEW.actor IS NOT OLD.actor
 OR NEW.session_id IS NOT OLD.session_id OR NEW.generation IS NOT OLD.generation OR NEW.role_version IS NOT OLD.role_version
 OR NEW.security_epoch IS NOT OLD.security_epoch OR NEW.email_version IS NOT OLD.email_version OR NEW.credential_version IS NOT OLD.credential_version
 OR NEW.policy_version IS NOT OLD.policy_version OR NEW.payload IS NOT OLD.payload OR NEW.digest IS NOT OLD.digest
 OR NEW.created_at IS NOT OLD.created_at OR NEW.expires_at IS NOT OLD.expires_at OR NEW.request_id IS NOT OLD.request_id OR NEW.binding IS NOT OLD.binding
 OR OLD.state!='pending' OR NEW.state NOT IN ('committed','revoked','held')
BEGIN SELECT RAISE(ABORT,'immutable or consumed Owner action'); END;
CREATE TRIGGER human_invitation_guard BEFORE UPDATE ON human_project_invitations
WHEN NEW.invitation_id IS NOT OLD.invitation_id OR NEW.project_id IS NOT OLD.project_id OR NEW.issuer IS NOT OLD.issuer
 OR NEW.issuer_session IS NOT OLD.issuer_session OR NEW.issuer_security_epoch IS NOT OLD.issuer_security_epoch OR NEW.issuer_role_version IS NOT OLD.issuer_role_version OR NEW.recipient IS NOT OLD.recipient OR NEW.email_version IS NOT OLD.email_version
 OR NEW.security_epoch IS NOT OLD.security_epoch OR NEW.target_version IS NOT OLD.target_version OR NEW.role IS NOT OLD.role
 OR NEW.issued_at IS NOT OLD.issued_at OR NEW.expires_at IS NOT OLD.expires_at OR NEW.request_id IS NOT OLD.request_id
 OR NEW.commitment IS NOT OLD.commitment OR NEW.binding IS NOT OLD.binding OR NEW.version!=OLD.version+1 OR OLD.state!='pending' OR NEW.state NOT IN ('consumed','revoked')
BEGIN SELECT RAISE(ABORT,'immutable or consumed invitation'); END;
CREATE TRIGGER human_owner_challenge_guard BEFORE UPDATE ON human_owner_challenges
WHEN NEW.challenge_id IS NOT OLD.challenge_id OR NEW.action_id IS NOT OLD.action_id OR NEW.kind IS NOT OLD.kind
 OR NEW.browser IS NOT OLD.browser OR NEW.issued_at IS NOT OLD.issued_at OR NEW.expires_at IS NOT OLD.expires_at OR NEW.client_key IS NOT OLD.client_key OR NEW.commitment IS NOT OLD.commitment OR NEW.request_id IS NOT OLD.request_id
 OR NEW.failures<OLD.failures OR NEW.failures>OLD.failures+1
 OR NOT ((OLD.state='queued' AND NEW.state='dispatching') OR (OLD.state='dispatching' AND NEW.state IN ('accepted','delivered','uncertain','failed')) OR (OLD.state='delivered' AND NEW.state IN ('delivered','consumed','exhausted')))
BEGIN SELECT RAISE(ABORT,'invalid Owner challenge transition'); END;
CREATE TRIGGER human_project_bindings_no_delete BEFORE DELETE ON human_project_bindings BEGIN SELECT RAISE(ABORT,'governance evidence retained'); END;
CREATE TRIGGER human_project_bindings_no_update BEFORE UPDATE ON human_project_bindings BEGIN SELECT RAISE(ABORT,'immutable governance evidence'); END;
CREATE TRIGGER human_memberships_no_delete BEFORE DELETE ON human_memberships BEGIN SELECT RAISE(ABORT,'governance evidence retained'); END;
CREATE TRIGGER human_owner_actions_no_delete BEFORE DELETE ON human_owner_actions BEGIN SELECT RAISE(ABORT,'governance evidence retained'); END;
CREATE TRIGGER human_owner_proofs_no_delete BEFORE DELETE ON human_owner_proofs BEGIN SELECT RAISE(ABORT,'governance evidence retained'); END;
CREATE TRIGGER human_owner_proofs_no_update BEFORE UPDATE ON human_owner_proofs BEGIN SELECT RAISE(ABORT,'immutable governance evidence'); END;
CREATE TRIGGER human_owner_acceptances_no_delete BEFORE DELETE ON human_owner_acceptances BEGIN SELECT RAISE(ABORT,'governance evidence retained'); END;
CREATE TRIGGER human_owner_acceptances_no_update BEFORE UPDATE ON human_owner_acceptances BEGIN SELECT RAISE(ABORT,'immutable governance evidence'); END;
CREATE TRIGGER human_owner_effects_no_delete BEFORE DELETE ON human_owner_effects BEGIN SELECT RAISE(ABORT,'governance evidence retained'); END;
CREATE TRIGGER human_owner_effects_no_update BEFORE UPDATE ON human_owner_effects BEGIN SELECT RAISE(ABORT,'immutable governance evidence'); END;
CREATE TRIGGER human_project_invitations_no_delete BEFORE DELETE ON human_project_invitations BEGIN SELECT RAISE(ABORT,'governance evidence retained'); END;
CREATE TRIGGER human_owner_challenges_no_delete BEFORE DELETE ON human_owner_challenges BEGIN SELECT RAISE(ABORT,'governance evidence retained'); END;
CREATE TRIGGER human_governance_events_no_delete BEFORE DELETE ON human_governance_events BEGIN SELECT RAISE(ABORT,'governance evidence retained'); END;
CREATE TRIGGER human_governance_events_no_update BEFORE UPDATE ON human_governance_events BEGIN SELECT RAISE(ABORT,'immutable governance evidence'); END;
CREATE TRIGGER human_owner_notifications_no_delete BEFORE DELETE ON human_owner_notifications BEGIN SELECT RAISE(ABORT,'governance evidence retained'); END;
CREATE TRIGGER human_owner_notifications_no_update BEFORE UPDATE ON human_owner_notifications BEGIN SELECT RAISE(ABORT,'immutable governance evidence'); END;
