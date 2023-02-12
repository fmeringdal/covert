CREATE TABLE IF NOT EXISTS CONNECTION (
    lock INTEGER PRIMARY KEY DEFAULT 1,
    connection_url TEXT NOT NULL,
    max_open_connections INTEGER NOT NULL

    -- Used to ensure that maximum one connection is ever inserted
    CONSTRAINT CONFIG_LOCK CHECK (lock=1)
); 

CREATE TABLE IF NOT EXISTS ROLES (
    "name" TEXT PRIMARY KEY,
    "sql" TEXT NOT NULL,
    revocation_sql TEXT NOT NULL
); 
