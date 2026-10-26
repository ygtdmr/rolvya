-- No key material: identity and a domain-separated HMAC verifier only.
-- An upgraded or preexisting database is never automatically a fresh installation.
CREATE TABLE controller_installation (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 state TEXT NOT NULL CHECK(state IN ('recovery','fresh','pending','ready')),
 birth_identity BLOB CHECK(length(birth_identity)=16),
 request_id TEXT CHECK(length(request_id) BETWEEN 1 AND 64),
 installation_id BLOB CHECK(length(installation_id)=16),
 key_id BLOB CHECK(length(key_id)=16),
 format_version INTEGER,
 key_version INTEGER,
 verifier BLOB CHECK(length(verifier)=32),
 CHECK((state<>'recovery')=(birth_identity IS NOT NULL)),
 CHECK((state IN ('pending','ready'))=(request_id IS NOT NULL)),
 CHECK((state='ready')=(installation_id IS NOT NULL)),
 CHECK((state='ready')=(key_id IS NOT NULL)),
 CHECK((state='ready')=(format_version IS NOT NULL)),
 CHECK((state='ready')=(key_version IS NOT NULL)),
 CHECK((state='ready')=(verifier IS NOT NULL)),
 CHECK(format_version IS NULL OR format_version=1),
 CHECK(key_version IS NULL OR key_version=1)
) STRICT;
INSERT INTO controller_installation(singleton,state) VALUES(1,'recovery');
CREATE TRIGGER controller_installation_no_delete BEFORE DELETE ON controller_installation
BEGIN SELECT RAISE(ABORT,'installation recovery requires controlled operation'); END;
CREATE TRIGGER controller_installation_transition BEFORE UPDATE ON controller_installation WHEN
 NEW.singleton<>OLD.singleton OR NOT (
  (OLD.state='recovery' AND NEW.state='fresh' AND NOT EXISTS(SELECT 1 FROM projects)) OR
  (OLD.state='fresh' AND NEW.birth_identity=OLD.birth_identity AND NEW.state='pending' AND NOT EXISTS(SELECT 1 FROM projects)) OR
  (OLD.state='pending' AND NEW.birth_identity=OLD.birth_identity AND NEW.state='ready' AND NEW.request_id=OLD.request_id)
 )
BEGIN SELECT RAISE(ABORT,'invalid installation transition'); END;
