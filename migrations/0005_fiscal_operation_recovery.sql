-- A migration 0004 permanece imutável. Documentos anteriores podem ter uma
-- operação em andamento; o núcleo consulta o último evento para esses casos.
ALTER TABLE fiscal.documents ADD COLUMN operation text;
ALTER TABLE fiscal.documents ALTER COLUMN operation SET DEFAULT 'none';
ALTER TABLE fiscal.documents ADD CONSTRAINT documents_operation_check
    CHECK (operation IN ('none', 'issue', 'cancel')) NOT VALID;
ALTER TABLE fiscal.documents ADD CONSTRAINT documents_state_operation_check
    CHECK ((state IN ('prepared', 'authorized', 'rejected', 'cancelled') AND operation = 'none') OR
           (state = 'sending' AND operation = 'issue') OR
           (state = 'cancel_pending' AND operation = 'cancel') OR
           (state = 'indeterminate' AND operation IN ('issue', 'cancel'))) NOT VALID;
GRANT UPDATE (operation) ON fiscal.documents TO fiscal_app;

CREATE OR REPLACE FUNCTION fiscal.validate_transition() RETURNS trigger
LANGUAGE plpgsql SET search_path = fiscal, pg_temp AS $$
DECLARE
    old_operation text;
BEGIN
    IF ROW(NEW.company_id, NEW.id, NEW.fact_id, NEW.profile_id, NEW.plan_key,
           NEW.plan_hash, NEW.decision_version, NEW.scenario, NEW.kind,
           NEW.fiscal_code, NEW.amount_cents, NEW.tax_base_cents, NEW.created_at)
       IS DISTINCT FROM
       ROW(OLD.company_id, OLD.id, OLD.fact_id, OLD.profile_id, OLD.plan_key,
           OLD.plan_hash, OLD.decision_version, OLD.scenario, OLD.kind,
           OLD.fiscal_code, OLD.amount_cents, OLD.tax_base_cents, OLD.created_at) THEN
        RAISE EXCEPTION 'fiscal:immutable';
    END IF;
    IF NOT (
        (OLD.state = 'prepared' AND NEW.state = 'sending') OR
        (OLD.state = 'sending' AND NEW.state IN ('authorized', 'rejected', 'indeterminate')) OR
        (OLD.state = 'indeterminate' AND NEW.state IN ('authorized', 'rejected', 'cancelled')) OR
        (OLD.state = 'authorized' AND NEW.state = 'cancel_pending') OR
        (OLD.state = 'cancel_pending' AND NEW.state IN ('authorized', 'cancelled', 'indeterminate'))
    ) THEN
        RAISE EXCEPTION 'fiscal:transition';
    END IF;
    old_operation := OLD.operation;
    IF old_operation IS NULL AND OLD.state IN ('sending', 'cancel_pending', 'indeterminate') THEN
        SELECT CASE e.kind WHEN 'issue_confirmed' THEN 'issue' ELSE 'cancel' END
          INTO old_operation
          FROM fiscal.document_events e
         WHERE e.company_id = OLD.company_id AND e.document_id = OLD.id
           AND e.kind IN ('issue_confirmed', 'cancel_confirmed')
         ORDER BY e.occurred_at DESC, e.id DESC LIMIT 1;
    END IF;
    IF (NEW.state = 'sending' AND NEW.operation IS DISTINCT FROM 'issue') OR
       (NEW.state = 'cancel_pending' AND NEW.operation IS DISTINCT FROM 'cancel') OR
       (NEW.state = 'indeterminate' AND NEW.operation IS DISTINCT FROM old_operation) OR
       (NEW.state IN ('authorized', 'rejected', 'cancelled') AND NEW.operation IS DISTINCT FROM 'none') OR
       (OLD.state = 'indeterminate' AND old_operation = 'issue' AND NEW.state = 'cancelled') OR
       (OLD.state = 'indeterminate' AND old_operation = 'cancel' AND NEW.state = 'rejected') THEN
        RAISE EXCEPTION 'fiscal:operation';
    END IF;
    IF NEW.state = 'authorized' AND NOT EXISTS (
        SELECT 1 FROM fiscal.document_artifacts
        WHERE company_id = NEW.company_id AND document_id = NEW.id AND kind = 'xml'
    ) THEN
        RAISE EXCEPTION 'fiscal:xml-required';
    END IF;
    NEW.updated_at := clock_timestamp();
    RETURN NEW;
END;
$$;
