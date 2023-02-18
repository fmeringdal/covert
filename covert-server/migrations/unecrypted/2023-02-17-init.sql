-- Encrypted key shares
CREATE TABLE IF NOT EXISTS KEY_SHARES (
    nonce BLOB NOT NULL PRIMARY KEY,
    "key" BLOB NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS SEAL_CONFIG (
    lock INTEGER PRIMARY KEY DEFAULT 1,

    shares INTEGER NOT NULL,
    threshold INTEGER NOT NULL,

    -- Used to ensure that maximum one config is ever inserted
    CONSTRAINT CONFIG_LOCK CHECK (lock=1)
) STRICT;