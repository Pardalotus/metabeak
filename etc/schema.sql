-- Stored handler function.
CREATE TABLE handler (
    handler_id BIGSERIAL PRIMARY KEY NOT NULL,
    owner_id INTEGER NOT NULL,
    hash TEXT,
    code TEXT NOT NULL,
    status INTEGER NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(hash));

-- Identifier for an Entity.
CREATE TABLE entity (
    entity_id BIGSERIAL PRIMARY KEY NOT NULL,
    identifier_type INT NOT NULL,
    identifier TEXT NOT NULL,
    -- Put the type first in the BTree index as nearly all the time each identifier_string has only one identifier_type entry.
    -- The other order would result in each identifier btree having a one-item btree for type in nearly all cases.
    UNIQUE(identifier_type, identifier)
);

-- Event passed to a handler function.
CREATE TABLE event (
    event_id BIGSERIAL PRIMARY KEY NOT NULL,
    json TEXT NOT NULL,
    status INTEGER NOT NULL,
    source_id INTEGER NOT NULL,
    analyzer_id INTEGER NOT NULL,
    assertion_id BIGINT NOT NULL,
    subject_entity_id BIGINT NULL REFERENCES entity(entity_id),
    object_entity_id BIGINT NULL REFERENCES entity(entity_id),
    created TIMESTAMPTZ NOT NULL DEFAULT NOW());

-- Queue of Event pointers to be passed to Handler functions.
CREATE TABLE event_queue (
    event_queue_id BIGSERIAL PRIMARY KEY NOT NULL,
    event_id BIGINT,
    created TIMESTAMPTZ NOT NULL DEFAULT NOW());

-- Populate Event Queue for new Events.
CREATE FUNCTION new_event_trigger_f()
    RETURNS TRIGGER
    LANGUAGE plpgsql AS
$$
BEGIN
    INSERT INTO event_queue (event_id)
    VALUES (NEW.event_id);
RETURN NULL;
END;
$$;

CREATE TRIGGER new_event_trigger
    AFTER INSERT ON event
    FOR EACH ROW
    EXECUTE FUNCTION new_event_trigger_f();

-- Result of executing a Handler Function.
CREATE TABLE execution_result (
    result_id BIGSERIAL PRIMARY KEY NOT NULL,
    handler_id BIGINT NOT NULL,
    event_id BIGINT NOT NULL,
    result TEXT NULL,
    error TEXT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT NOW());

-- Metadata assertion of a source.
-- There may be multiple metadata assertions about a subject entity, even by a source.
-- Older duplicate assertions may be removed.
CREATE TABLE metadata_assertion (
    assertion_id BIGSERIAL PRIMARY KEY NOT NULL,
    source_id INTEGER,
    json TEXT NOT NULL,

    -- Hash of the JSON.
    hash TEXT,
    subject_entity_id BIGINT REFERENCES entity(entity_id),
    created TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Reason for saving this assertion.
    -- 1 is primary activity (i.e. new record found)
    -- 2 is secondary activity (i.e. gathering background metadata)
    reason SMALLINT,

    -- Reject duplicate assertions from the same source based on their hash.
    UNIQUE(subject_entity_id, hash, source_id)
);

-- Named checkpoint date, used by agents.
CREATE TABLE CHECKPOINT (
    id TEXT PRIMARY KEY NOT NULL,
    date TIMESTAMPTZ NOT NULL
);


-- Queue of new primary Metadata Assertions.
CREATE TABLE metadata_assertion_queue (
    queue_id BIGSERIAL PRIMARY KEY NOT NULL,
    assertion_id BIGINT,
    created TIMESTAMPTZ NOT NULL DEFAULT NOW());

-- Populate Metadata Assertions Queue for new primary Metadata Assertions.
CREATE FUNCTION new_metadata_trigger_f()
    RETURNS TRIGGER
    LANGUAGE plpgsql AS
$$
BEGIN
    -- Only queue up Primary assertions.
    IF NEW.reason = 1 THEN
    INSERT INTO metadata_assertion_queue (assertion_id)
    VALUES (NEW.assertion_id);
END IF;
RETURN NULL;
END;
$$;

CREATE TRIGGER new_metadata_assertion_trigger
    AFTER INSERT ON metadata_assertion
    FOR EACH ROW
    EXECUTE FUNCTION new_metadata_trigger_f();
