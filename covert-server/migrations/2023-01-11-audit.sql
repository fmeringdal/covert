CREATE TABLE IF NOT EXISTS AUDIT_LOGS (
    id INTEGER NOT NULL,
    path TEXT NOT NULL,
    method TEXT NOT NULL,
    -- info field instead to store extra context
    -- - policies at the time
    -- - mount type
    info TEXT NOT NULL,
    entity_id INTEGER NOT NULL REFERENCES ENTITIES (id) ON DELETE RESTRICT ON UPDATE RESTRICT,
    timestamp INTEGER NOT NULL DEFAULT CURRENT_TIMESTAMP,
    processing_time_ms INTEGER,
    response_status_code INTEGER
) STRICT;
