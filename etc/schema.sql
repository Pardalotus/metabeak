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
    source INTEGER NOT NULL,
    analyzer INTEGER NOT NULL,
    subject_entity_id BIGINT NULL,
    object_entity_id BIGINT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT NOW());

-- Queue of Event pointers to be passed to Handler functions.
CREATE TABLE event_queue (
    execution_id BIGSERIAL PRIMARY KEY NOT NULL,
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
