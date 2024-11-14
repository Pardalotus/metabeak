
CREATE TABLE handler (
    handler_id BIGSERIAL PRIMARY KEY NOT NULL,
    owner_id INTEGER NOT NULL,
    hash TEXT,
    code TEXT NOT NULL,
    status INTEGER NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(hash));

CREATE TABLE event (
    event_id BIGSERIAL PRIMARY KEY NOT NULL,
    json TEXT NOT NULL,
    status INTEGER NOT NULL,
    source INTEGER NOT NULL,
    analyzer INTEGER NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT NOW());

CREATE TABLE event_queue (
    execution_id BIGSERIAL PRIMARY KEY NOT NULL,
    event_id BIGINT,
    created TIMESTAMPTZ NOT NULL DEFAULT NOW());

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

CREATE TABLE execution_result (
    result_id BIGSERIAL PRIMARY KEY NOT NULL,
    handler_id BIGINT NOT NULL,
    event_id BIGINT NOT NULL,
    result TEXT NULL,
    error TEXT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT NOW());
