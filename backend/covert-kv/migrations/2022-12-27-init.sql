CREATE TABLE IF NOT EXISTS SECRETS (
    "key" TEXT NOT NULL,
    "version" INTEGER NOT NULL, 
    "value" TEXT,
    created_time TIMESTAMP NOT NULL,
    deleted BOOLEAN NOT NULL DEFAULT FALSE,
    destroyed BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY("key", "version"),
    CONSTRAINT destroyed_secret CHECK (
        -- If not destroyed then value is *not* null
        (NOT(destroyed) AND "value" IS NOT NULL) OR
        -- If destroyed then value is null
        (destroyed AND "value" IS NULL) 
    )
); 

CREATE TABLE IF NOT EXISTS CONFIG (
    lock INTEGER PRIMARY KEY DEFAULT 1,
    max_versions INTEGER NOT NULL DEFAULT 10,

    -- Used to ensure that maximum one config is ever inserted
    CONSTRAINT CONFIG_LOCK CHECK (lock=1)
); 
