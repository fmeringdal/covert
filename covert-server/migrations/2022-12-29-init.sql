CREATE TABLE IF NOT EXISTS ENTITIES (
    "name" TEXT PRIMARY KEY,
    "disabled" INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS POLICIES (
    "name" TEXT PRIMARY KEY,
    "policy" TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS ENTITY_ALIASES (
    "name" TEXT NOT NULL,
    mount_path TEXT NOT NULL REFERENCES MOUNTS ("path") 
        ON DELETE CASCADE 
        -- Update alias mount path in case mount is moved
        ON UPDATE CASCADE,
    entity_name TEXT NOT NULL REFERENCES ENTITIES ("name") ON DELETE CASCADE ON UPDATE RESTRICT,
    PRIMARY KEY(entity_name, "name")
) STRICT;

CREATE TABLE IF NOT EXISTS ENTITY_POLICIES (
    policy_name TEXT NOT NULL REFERENCES POLICIES ("name") ON DELETE CASCADE ON UPDATE RESTRICT,
    entity_name TEXT NOT NULL REFERENCES ENTITIES ("name") ON DELETE CASCADE ON UPDATE RESTRICT,
    PRIMARY KEY(policy_name, entity_name)
) STRICT;

CREATE TABLE IF NOT EXISTS TOKENS (
    id INTEGER PRIMARY KEY,
    token TEXT NOT NULL,
    issued_at TEXT NOT NULL,
    expires_at TEXT,
    entity_name TEXT NOT NULL REFERENCES ENTITIES ("name") ON DELETE CASCADE ON UPDATE RESTRICT
    -- TODO: expires at can only be null for root tokens
    -- CONSTRAINT EXPIRES_AT CHECK (
    --     (expires_at IS NULL AND entity_name = "root") OR
    --     (expires_at IS NOT NULL AND entity_name != "root")
    -- )
) STRICT;

CREATE TABLE IF NOT EXISTS MOUNTS (
    id  TEXT PRIMARY KEY,
    "path" TEXT UNIQUE NOT NULL,
    variant TEXT NOT NULL,
    max_lease_ttl INTEGER NOT NULL,
    default_lease_ttl INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS LEASES (
    id TEXT NOT NULL,
    -- The mount path of the backend that issued the leased data
    issued_mount_path TEXT NOT NULL REFERENCES MOUNTS ("path") 
        -- ON DELETE RESTRICT to ensure leases are properly revoked when a secret
        -- engine is disabled
        ON DELETE RESTRICT 
        -- The mount path can be updated just fine without affecting leases
        ON UPDATE CASCADE,
    -- Can be NULL for token leases
    revoke_path TEXT,
    revoke_data TEXT NOT NULL,
    -- Can be NULL for token leases
    renew_path TEXT,
    renew_data TEXT NOT NULL,
    issued_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    last_renewal_time TEXT NOT NULL
) STRICT;
