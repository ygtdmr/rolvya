-- S2-03A: primary proofs only; no sessions, TOTP credentials, roles or privileged effects.
CREATE TABLE human_users (
 user_id TEXT PRIMARY KEY CHECK(length(user_id)=32 AND user_id NOT GLOB '*[^0-9a-f]*'),
 state TEXT NOT NULL CHECK(state IN ('active','disabled','compromised')),
 security_epoch INTEGER NOT NULL CHECK(security_epoch>0),
 factor_required INTEGER NOT NULL DEFAULT 0 CHECK(factor_required IN (0,1)),
 created_at INTEGER NOT NULL CHECK(created_at>=0)
) STRICT;
CREATE TABLE human_emails (
 user_id TEXT PRIMARY KEY REFERENCES human_users(user_id),
 address TEXT NOT NULL UNIQUE CHECK(length(address) BETWEEN 3 AND 254),
 version INTEGER NOT NULL CHECK(version>0),
 verified INTEGER NOT NULL CHECK(verified IN (0,1)),
 email_login INTEGER NOT NULL CHECK(email_login IN (0,1)),
 CHECK(email_login=0 OR verified=1)
) STRICT;
CREATE TABLE human_passwords (
 user_id TEXT PRIMARY KEY REFERENCES human_users(user_id),
 version INTEGER NOT NULL CHECK(version>0),
 format INTEGER NOT NULL CHECK(format=1),
 phc TEXT NOT NULL CHECK(length(phc) BETWEEN 40 AND 512),
 binding BLOB NOT NULL CHECK(length(binding)=32)
) STRICT;
CREATE TABLE human_login_policy (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 version INTEGER NOT NULL CHECK(version>0),
 password INTEGER NOT NULL CHECK(password IN (0,1)),
 verified_email INTEGER NOT NULL CHECK(verified_email IN (0,1)),
 CHECK(password+verified_email>0)
) STRICT;
CREATE TABLE human_enrollment (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 state TEXT NOT NULL CHECK(state IN ('fresh','pending','ready')),
 request_id TEXT,
 commitment BLOB,
 user_id TEXT REFERENCES human_users(user_id),
 CHECK((state='fresh' AND request_id IS NULL AND commitment IS NULL AND user_id IS NULL)
    OR (state='pending' AND request_id IS NOT NULL AND length(commitment)=32 AND user_id IS NULL)
    OR (state='ready' AND request_id IS NOT NULL AND length(commitment)=32 AND user_id IS NOT NULL))
) STRICT;
INSERT INTO human_enrollment(singleton,state) VALUES(1,'fresh');
CREATE TABLE human_enroll_requests (
 request_id TEXT PRIMARY KEY,
 commitment BLOB NOT NULL CHECK(length(commitment)=32),
 user_id TEXT NOT NULL REFERENCES human_users(user_id)
) STRICT;
CREATE TABLE human_attempts (
 attempt_id TEXT PRIMARY KEY,
 channel TEXT NOT NULL CHECK(channel IN ('password','email')),
 account_key BLOB NOT NULL CHECK(length(account_key)=32),
 client_key BLOB NOT NULL CHECK(length(client_key)=32),
 started_at INTEGER NOT NULL CHECK(started_at>=0),
 state TEXT NOT NULL CHECK(state IN ('pending','failed','succeeded'))
) STRICT;
CREATE INDEX human_attempt_account ON human_attempts(channel,account_key,started_at);
CREATE INDEX human_attempt_client ON human_attempts(channel,client_key,started_at);
CREATE TABLE human_challenges (
 challenge_id TEXT PRIMARY KEY,
 user_id TEXT REFERENCES human_users(user_id),
 address TEXT NOT NULL CHECK(length(address) BETWEEN 3 AND 254),
 email_version INTEGER NOT NULL CHECK(email_version>=0),
 security_epoch INTEGER NOT NULL CHECK(security_epoch>=0),
 policy_version INTEGER NOT NULL CHECK(policy_version>0),
 kind TEXT NOT NULL CHECK(kind IN ('magic_link','otp')),
 purpose TEXT NOT NULL CHECK(purpose IN ('login','verify_email')),
 browser BLOB NOT NULL CHECK(length(browser)=32),
 account_key BLOB NOT NULL CHECK(length(account_key)=32),
 client_key BLOB NOT NULL CHECK(length(client_key)=32),
 request_id TEXT NOT NULL UNIQUE,
 request_commitment BLOB NOT NULL CHECK(length(request_commitment)=32),
 token_commitment BLOB NOT NULL CHECK(length(token_commitment)=32),
 key_id BLOB NOT NULL CHECK(length(key_id)=16),
 key_version INTEGER NOT NULL CHECK(key_version>0),
 issued_at INTEGER NOT NULL CHECK(issued_at>=0),
 expires_at INTEGER NOT NULL CHECK(expires_at=issued_at+600),
 failures INTEGER NOT NULL DEFAULT 0 CHECK(failures BETWEEN 0 AND 5),
 state TEXT NOT NULL CHECK(state IN ('queued','dispatching','accepted','delivered','uncertain','failed','consumed','exhausted','revoked')),
 CHECK(state!='exhausted' OR failures=5)
) STRICT;
CREATE INDEX human_challenge_account ON human_challenges(account_key,issued_at);
CREATE INDEX human_challenge_client ON human_challenges(client_key,issued_at);
CREATE INDEX human_challenge_expiry ON human_challenges(state,expires_at);
CREATE TABLE human_authentications (
 evidence_id TEXT PRIMARY KEY,
 user_id TEXT NOT NULL REFERENCES human_users(user_id),
 method TEXT NOT NULL CHECK(method IN ('password','magic_link','otp')),
 credential_version INTEGER NOT NULL CHECK(credential_version>0),
 security_epoch INTEGER NOT NULL CHECK(security_epoch>0),
 policy_version INTEGER NOT NULL CHECK(policy_version>0),
 verified_at INTEGER NOT NULL CHECK(verified_at>=0),
 factor_required INTEGER NOT NULL CHECK(factor_required IN (0,1)),
 source_ref TEXT NOT NULL UNIQUE,
 binding BLOB NOT NULL CHECK(length(binding)=32)
) STRICT;
CREATE TABLE human_security_events (
 sequence INTEGER PRIMARY KEY,
 user_id TEXT REFERENCES human_users(user_id),
 request_id TEXT NOT NULL,
 event TEXT NOT NULL CHECK(event IN ('enrollment_pending','enrolled','password_changed','email_changed','policy_changed','identity_restricted','email_login_changed','password_denied','primary_verified','challenge_queued','mail_dispatching','mail_accepted','mail_delivered','mail_uncertain','challenge_denied','email_verified','maintenance')),
 occurred_at INTEGER NOT NULL CHECK(occurred_at>=0)
) STRICT;
CREATE TRIGGER human_user_update BEFORE UPDATE ON human_users
WHEN NEW.user_id IS NOT OLD.user_id OR NEW.created_at IS NOT OLD.created_at
 OR NEW.security_epoch!=OLD.security_epoch+1 OR NEW.factor_required IS NOT OLD.factor_required
 OR NEW.state NOT IN ('active','disabled','compromised')
BEGIN SELECT RAISE(ABORT,'invalid human identity transition'); END;
CREATE TRIGGER human_user_delete BEFORE DELETE ON human_users BEGIN SELECT RAISE(ABORT,'human identity retained'); END;
CREATE TRIGGER human_email_update BEFORE UPDATE ON human_emails
WHEN NEW.user_id IS NOT OLD.user_id OR NEW.version!=OLD.version+1
BEGIN SELECT RAISE(ABORT,'invalid email version'); END;
CREATE TRIGGER human_email_delete BEFORE DELETE ON human_emails BEGIN SELECT RAISE(ABORT,'email binding retained'); END;
CREATE TRIGGER human_password_update BEFORE UPDATE ON human_passwords
WHEN NEW.user_id IS NOT OLD.user_id OR NEW.version!=OLD.version+1
BEGIN SELECT RAISE(ABORT,'invalid password version'); END;
CREATE TRIGGER human_password_delete BEFORE DELETE ON human_passwords BEGIN SELECT RAISE(ABORT,'credential retained'); END;
CREATE TRIGGER human_policy_update BEFORE UPDATE ON human_login_policy
WHEN NEW.singleton!=OLD.singleton OR NEW.version!=OLD.version+1
BEGIN SELECT RAISE(ABORT,'invalid policy version'); END;
CREATE TRIGGER human_policy_delete BEFORE DELETE ON human_login_policy BEGIN SELECT RAISE(ABORT,'policy retained'); END;
CREATE TRIGGER human_enrollment_update BEFORE UPDATE ON human_enrollment
WHEN NOT ((OLD.state='fresh' AND NEW.state='pending') OR
 (OLD.state='pending' AND NEW.state='ready' AND NEW.request_id=OLD.request_id AND NEW.commitment=OLD.commitment))
BEGIN SELECT RAISE(ABORT,'invalid enrollment transition'); END;
CREATE TRIGGER human_enrollment_delete BEFORE DELETE ON human_enrollment BEGIN SELECT RAISE(ABORT,'enrollment retained'); END;
CREATE TRIGGER human_request_update BEFORE UPDATE ON human_enroll_requests BEGIN SELECT RAISE(ABORT,'immutable enrollment request'); END;
CREATE TRIGGER human_request_delete BEFORE DELETE ON human_enroll_requests BEGIN SELECT RAISE(ABORT,'enrollment evidence retained'); END;
CREATE TRIGGER human_attempt_update BEFORE UPDATE ON human_attempts
WHEN NEW.attempt_id IS NOT OLD.attempt_id OR NEW.channel IS NOT OLD.channel
 OR NEW.account_key IS NOT OLD.account_key OR NEW.client_key IS NOT OLD.client_key
 OR NEW.started_at IS NOT OLD.started_at OR OLD.state!='pending' OR NEW.state NOT IN ('failed','succeeded')
BEGIN SELECT RAISE(ABORT,'invalid attempt transition'); END;
CREATE TRIGGER human_challenge_update BEFORE UPDATE ON human_challenges
WHEN NEW.challenge_id IS NOT OLD.challenge_id OR NEW.user_id IS NOT OLD.user_id
 OR NEW.address IS NOT OLD.address OR NEW.email_version IS NOT OLD.email_version
 OR NEW.security_epoch IS NOT OLD.security_epoch OR NEW.policy_version IS NOT OLD.policy_version
 OR NEW.kind IS NOT OLD.kind OR NEW.purpose IS NOT OLD.purpose OR NEW.browser IS NOT OLD.browser
 OR NEW.account_key IS NOT OLD.account_key OR NEW.client_key IS NOT OLD.client_key
 OR NEW.request_id IS NOT OLD.request_id OR NEW.request_commitment IS NOT OLD.request_commitment
 OR NEW.token_commitment IS NOT OLD.token_commitment OR NEW.key_id IS NOT OLD.key_id OR NEW.key_version IS NOT OLD.key_version
 OR NEW.issued_at IS NOT OLD.issued_at OR NEW.expires_at IS NOT OLD.expires_at
 OR NOT (
 (OLD.state='queued' AND NEW.state IN ('dispatching','failed','revoked') AND NEW.failures=OLD.failures)
 OR (OLD.state='dispatching' AND NEW.state IN ('accepted','delivered','uncertain') AND NEW.failures=OLD.failures)
 OR (OLD.state='accepted' AND NEW.state IN ('delivered','uncertain','revoked') AND NEW.failures=OLD.failures)
 OR (OLD.state='delivered' AND NEW.state IN ('consumed','revoked') AND NEW.failures=OLD.failures)
 OR (OLD.state='delivered' AND OLD.failures<5 AND NEW.failures=OLD.failures+1 AND
     ((NEW.failures<5 AND NEW.state='delivered') OR (NEW.failures=5 AND NEW.state='exhausted'))))
BEGIN SELECT RAISE(ABORT,'invalid challenge transition'); END;
CREATE TRIGGER human_challenge_delete BEFORE DELETE ON human_challenges BEGIN SELECT RAISE(ABORT,'challenge evidence retained'); END;
CREATE TRIGGER human_authentication_update BEFORE UPDATE ON human_authentications BEGIN SELECT RAISE(ABORT,'immutable primary proof'); END;
CREATE TRIGGER human_authentication_delete BEFORE DELETE ON human_authentications BEGIN SELECT RAISE(ABORT,'primary proof retained'); END;
CREATE TRIGGER human_security_update BEFORE UPDATE ON human_security_events BEGIN SELECT RAISE(ABORT,'append-only human audit'); END;
CREATE TRIGGER human_security_delete BEFORE DELETE ON human_security_events BEGIN SELECT RAISE(ABORT,'append-only human audit'); END;
