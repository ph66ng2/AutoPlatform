-- Completa snapshots anteriores à ingestão v2 e protege o caminho fiscal
-- legado, que ainda insere somente payload e hash.
ALTER TABLE fiscal.fact_snapshots DISABLE TRIGGER fact_snapshots_immutable;
UPDATE fiscal.fact_snapshots
SET event_id = (payload->>'event_id')::uuid,
    idempotency_key = payload->>'idempotency_key',
    correlation_id = payload->>'correlation_id'
WHERE event_id IS NULL OR idempotency_key IS NULL OR correlation_id IS NULL;
ALTER TABLE fiscal.fact_snapshots ENABLE TRIGGER fact_snapshots_immutable;

CREATE FUNCTION fiscal.fill_fact_snapshot_keys() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.event_id IS NULL THEN
        NEW.event_id := (NEW.payload->>'event_id')::uuid;
    END IF;
    IF NEW.idempotency_key IS NULL THEN
        NEW.idempotency_key := NEW.payload->>'idempotency_key';
    END IF;
    IF NEW.correlation_id IS NULL THEN
        NEW.correlation_id := NEW.payload->>'correlation_id';
    END IF;
    IF NEW.event_id IS DISTINCT FROM (NEW.payload->>'event_id')::uuid
       OR NEW.idempotency_key IS DISTINCT FROM NEW.payload->>'idempotency_key'
       OR NEW.correlation_id IS DISTINCT FROM NEW.payload->>'correlation_id' THEN
        RAISE EXCEPTION 'commercial fact keys differ from payload';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER fact_snapshots_keys BEFORE INSERT ON fiscal.fact_snapshots
FOR EACH ROW EXECUTE FUNCTION fiscal.fill_fact_snapshot_keys();

ALTER TABLE fiscal.fact_snapshots
    ALTER COLUMN event_id SET NOT NULL,
    ALTER COLUMN idempotency_key SET NOT NULL,
    ALTER COLUMN correlation_id SET NOT NULL;
