CREATE TABLE IF NOT EXISTS NAMESPACES (
    id TEXT NOT NULL PRIMARY KEY,
    "name" TEXT NOT NULL,
    parent_namespace_id TEXT REFERENCES NAMESPACES(id)
        -- Force child namespaces to be properly removed
        ON DELETE RESTRICT,
    CONSTRAINT HAS_PARENT CHECK (
        (parent_namespace_id IS NULL AND name = "root") OR
        (parent_namespace_id IS NOT NULL AND name != "root")
    ),
    CONSTRAINT UNIQUE_SUB_NS_NAME UNIQUE(parent_namespace_id, "name"),
    CONSTRAINT VALID_NAME CHECK(
            (LOWER(name) = name) AND 
            (LENGTH(name) > 0) AND 
            (INSTR(name, " ") = 0) AND 
            (INSTR(name, "/") = 0)
    )
) STRICT;

-- Ensure only one root namespace can exist.
CREATE UNIQUE INDEX IF NOT EXISTS UNIQUE_ROOT_NS ON NAMESPACES("name") WHERE parent_namespace_id IS NULL;

CREATE TABLE IF NOT EXISTS ENTITIES (
    "name" TEXT NOT NULL,
    "disabled" INTEGER NOT NULL,
    namespace_id TEXT NOT NULL REFERENCES NAMESPACES(id) ON DELETE CASCADE ON UPDATE CASCADE,
    PRIMARY KEY(namespace_id, "name")
) STRICT;

CREATE TABLE IF NOT EXISTS POLICIES (
    "name" TEXT NOT NULL,
    "policy" TEXT NOT NULL,
    namespace_id TEXT NOT NULL REFERENCES NAMESPACES(id) ON DELETE CASCADE ON UPDATE CASCADE,
    PRIMARY KEY(namespace_id, "name")
) STRICT;

CREATE TABLE IF NOT EXISTS ENTITY_ALIASES (
    namespace_id TEXT NOT NULL,
    entity_name TEXT NOT NULL,
    mount_path TEXT NOT NULL,
    "name" TEXT NOT NULL,
    PRIMARY KEY(namespace_id, entity_name, mount_path),
    CONSTRAINT FK_ENTITY
        FOREIGN KEY (namespace_id, entity_name)
        REFERENCES ENTITIES (namespace_id, "name")
        ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT FK_MOUNT
        FOREIGN KEY (namespace_id, mount_path)
        REFERENCES MOUNTS (namespace_id, "path")
        ON DELETE CASCADE ON UPDATE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS ENTITY_POLICIES (
    namespace_id TEXT NOT NULL,
    policy_name TEXT NOT NULL,
    entity_name TEXT NOT NULL,
    PRIMARY KEY(namespace_id, policy_name, entity_name),
    CONSTRAINT FK_ENTITY
        FOREIGN KEY (namespace_id, entity_name)
        REFERENCES ENTITIES (namespace_id, "name")
        ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT FK_POLICY
        FOREIGN KEY (namespace_id, policy_name)
        REFERENCES POLICIES (namespace_id, "name")
        ON DELETE CASCADE ON UPDATE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS TOKENS (
    id INTEGER PRIMARY KEY,
    token TEXT NOT NULL,
    issued_at TEXT NOT NULL,
    expires_at TEXT,
    namespace_id TEXT NOT NULL,
    entity_name TEXT NOT NULL,
    CONSTRAINT FK_ENTITY
        FOREIGN KEY (namespace_id, entity_name)
        REFERENCES ENTITIES (namespace_id, "name")
        ON DELETE CASCADE ON UPDATE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS MOUNTS (
    id  TEXT PRIMARY KEY,
    "path" TEXT NOT NULL,
    variant TEXT NOT NULL,
    max_lease_ttl INTEGER NOT NULL,
    default_lease_ttl INTEGER NOT NULL,
    namespace_id TEXT NOT NULL REFERENCES NAMESPACES(id) 
        -- Ensure that mounts are properly deleted and all leases revoked
        -- when NS is deleted
        ON DELETE RESTRICT 
        ON UPDATE CASCADE,
    CONSTRAINT UNIQUE_NS_MOUNT_PATHS UNIQUE(namespace_id, "path"),
    CONSTRAINT VALID_PATH CHECK(
            (LOWER(path) = path) AND 
            (LENGTH(path) > 0) AND 
            (INSTR(path, " ") = 0)
    )
) STRICT;

CREATE TABLE IF NOT EXISTS LEASES (
    id TEXT NOT NULL,
    namespace_id TEXT NOT NULL,
    -- The mount path of the backend that issued the leased data
    issued_mount_path TEXT NOT NULL,
    -- Can be NULL for token leases
    revoke_path TEXT,
    revoke_data TEXT NOT NULL,
    -- Can be NULL for token leases
    renew_path TEXT,
    renew_data TEXT NOT NULL,
    failed_revocation_attempts INTEGER NOT NULL,
    issued_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    last_renewal_time TEXT NOT NULL,
    CONSTRAINT FK_MOUNT
        FOREIGN KEY (namespace_id, issued_mount_path)
        REFERENCES MOUNTS (namespace_id, "path")
        -- ON DELETE RESTRICT to ensure leases are properly revoked when a secret
        -- engine is disabled
        ON DELETE RESTRICT 
        -- The mount path can be updated just fine without affecting leases
        ON UPDATE CASCADE
) STRICT;
