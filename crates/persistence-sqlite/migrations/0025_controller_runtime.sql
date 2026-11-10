-- Liveness evidence for the installed control loop only. It grants no admission,
-- dispatch, provider, financial or recovery authority; those remain in R06/R08/R09.
CREATE TABLE controller_runtime_heartbeat (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 instance_id TEXT NOT NULL CHECK(length(instance_id)=32 AND instance_id NOT GLOB '*[^0-9a-f]*'),
 mode TEXT NOT NULL CHECK(mode IN ('closed','synthetic')),
 state TEXT NOT NULL CHECK(state IN ('running','stopped')),
 started_at INTEGER NOT NULL CHECK(started_at>=0),
 heartbeat_at INTEGER NOT NULL CHECK(heartbeat_at>=started_at),
 ticks INTEGER NOT NULL CHECK(ticks>=0),
 failed_ticks INTEGER NOT NULL CHECK(failed_ticks>=0 AND failed_ticks<=ticks),
 last_failure TEXT CHECK(last_failure IS NULL OR last_failure IN ('pass','storage','panic'))
) STRICT;
