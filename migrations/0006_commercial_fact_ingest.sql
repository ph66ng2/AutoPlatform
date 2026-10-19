-- Ingestão AutoOS do snapshot canônico. A leitura fiscal preexistente permanece.
DO $$ BEGIN
    CREATE ROLE integration_app NOLOGIN NOINHERIT NOBYPASSRLS;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;
GRANT integration_app TO CURRENT_USER;

CREATE FUNCTION identity.has_autoos_permission(p_company uuid, p_account uuid)
RETURNS boolean LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = identity, pg_temp AS $$
    SELECT identity.has_active_membership(p_company, p_account)
       AND EXISTS (SELECT 1 FROM memberships
                   WHERE company_id = p_company AND account_id = p_account
                     AND product = 'autoos' AND role IN ('admin', 'operator')
                     AND status = 'active')
$$;
REVOKE ALL ON FUNCTION identity.has_autoos_permission(uuid, uuid) FROM PUBLIC;
GRANT USAGE ON SCHEMA identity, fiscal TO integration_app;
GRANT EXECUTE ON FUNCTION identity.current_company_id() TO integration_app;
GRANT EXECUTE ON FUNCTION identity.current_account_id() TO integration_app;
GRANT EXECUTE ON FUNCTION identity.has_autoos_permission(uuid, uuid) TO integration_app;

ALTER TABLE fiscal.fact_snapshots
    ADD COLUMN event_id uuid,
    ADD COLUMN idempotency_key text,
    ADD COLUMN correlation_id text;
CREATE UNIQUE INDEX fact_snapshots_event_idx
    ON fiscal.fact_snapshots(company_id, event_id) WHERE event_id IS NOT NULL;
CREATE UNIQUE INDEX fact_snapshots_idempotency_idx
    ON fiscal.fact_snapshots(company_id, idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX fact_snapshots_correlation_idx
    ON fiscal.fact_snapshots(company_id, correlation_id) WHERE correlation_id IS NOT NULL;
GRANT SELECT, INSERT ON fiscal.fact_snapshots TO integration_app;
CREATE POLICY tenant_autoos_ingest ON fiscal.fact_snapshots FOR ALL TO integration_app
USING (company_id = identity.current_company_id()
       AND identity.has_autoos_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_autoos_permission(company_id, identity.current_account_id()));

-- Após a ingestão nenhuma identidade ou conteúdo do fato pode mudar.
CREATE FUNCTION fiscal.prevent_fact_snapshot_change() RETURNS trigger
LANGUAGE plpgsql AS $$ BEGIN
    RAISE EXCEPTION 'commercial fact snapshot is immutable';
END $$;
CREATE TRIGGER fact_snapshots_immutable BEFORE UPDATE OR DELETE ON fiscal.fact_snapshots
FOR EACH ROW EXECUTE FUNCTION fiscal.prevent_fact_snapshot_change();
