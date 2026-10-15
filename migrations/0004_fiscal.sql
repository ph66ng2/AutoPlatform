-- Núcleo fiscal provider-neutral. O snapshot e os artefatos são imutáveis;
-- nenhuma tabela de NF-e importada é reutilizada para documentos emitidos.
CREATE SCHEMA fiscal;

DO $$
BEGIN
    CREATE ROLE fiscal_app NOLOGIN NOINHERIT NOBYPASSRLS;
EXCEPTION WHEN duplicate_object THEN NULL;
END
$$;
GRANT fiscal_app TO CURRENT_USER;

CREATE FUNCTION identity.has_fiscal_permission(p_company uuid, p_account uuid)
RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = identity, pg_temp
AS $$
    SELECT identity.has_active_membership(p_company, p_account)
       AND EXISTS (
           SELECT 1 FROM memberships
           WHERE company_id = p_company AND account_id = p_account
             AND product = 'autobo' AND role = 'fiscal' AND status = 'active'
       )
$$;
REVOKE ALL ON FUNCTION identity.has_fiscal_permission(uuid, uuid) FROM PUBLIC;
GRANT USAGE ON SCHEMA identity TO fiscal_app;
GRANT EXECUTE ON FUNCTION identity.current_company_id() TO fiscal_app;
GRANT EXECUTE ON FUNCTION identity.current_account_id() TO fiscal_app;
GRANT EXECUTE ON FUNCTION identity.has_fiscal_permission(uuid, uuid) TO fiscal_app;

CREATE TABLE fiscal.issuer_profiles (
    company_id uuid NOT NULL REFERENCES identity.companies(id),
    id uuid NOT NULL,
    establishment_id uuid NOT NULL,
    label text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (company_id, id),
    UNIQUE (company_id, establishment_id),
    CHECK (label <> '')
);

CREATE TABLE fiscal.fact_snapshots (
    company_id uuid NOT NULL REFERENCES identity.companies(id),
    fact_id uuid NOT NULL,
    os_id uuid NOT NULL,
    os_version bigint NOT NULL CHECK (os_version > 0),
    origin text NOT NULL CHECK (origin = 'autoos'),
    payload_hash text NOT NULL CHECK (payload_hash ~ '^[0-9a-f]{64}$'),
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (company_id, fact_id),
    UNIQUE (company_id, os_id, os_version)
);

CREATE TABLE fiscal.documents (
    company_id uuid NOT NULL,
    id uuid NOT NULL,
    fact_id uuid NOT NULL,
    profile_id uuid NOT NULL,
    plan_key text NOT NULL,
    plan_hash text NOT NULL CHECK (plan_hash ~ '^[0-9a-f]{64}$'),
    decision_version bigint NOT NULL CHECK (decision_version > 0),
    scenario text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('nfse', 'nfe')),
    fiscal_code text NOT NULL CHECK (fiscal_code <> ''),
    amount_cents bigint NOT NULL CHECK (amount_cents >= 0),
    tax_base_cents bigint NOT NULL CHECK (tax_base_cents >= 0),
    state text NOT NULL DEFAULT 'prepared' CHECK (state IN (
        'prepared', 'sending', 'authorized', 'rejected', 'indeterminate',
        'cancel_pending', 'cancelled'
    )),
    external_key text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (company_id, id),
    UNIQUE (company_id, fact_id, plan_key),
    FOREIGN KEY (company_id, fact_id) REFERENCES fiscal.fact_snapshots(company_id, fact_id),
    FOREIGN KEY (company_id, profile_id) REFERENCES fiscal.issuer_profiles(company_id, id),
    CHECK (plan_key <> '' AND scenario <> '')
);
CREATE INDEX documents_profile_idx ON fiscal.documents(company_id, profile_id);

CREATE TABLE fiscal.document_items (
    company_id uuid NOT NULL,
    document_id uuid NOT NULL,
    item_id text NOT NULL,
    amount_cents bigint CHECK (amount_cents >= 0),
    PRIMARY KEY (company_id, document_id, item_id),
    FOREIGN KEY (company_id, document_id) REFERENCES fiscal.documents(company_id, id),
    CHECK (item_id <> '')
);

CREATE TABLE fiscal.document_events (
    company_id uuid NOT NULL,
    id uuid NOT NULL,
    document_id uuid NOT NULL,
    actor_id uuid,
    kind text NOT NULL CHECK (kind IN (
        'prepared', 'issue_confirmed', 'issue_started', 'authorized',
        'rejected', 'indeterminate', 'queried', 'cancel_confirmed',
        'cancel_started', 'cancelled'
    )),
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (company_id, id),
    FOREIGN KEY (company_id, document_id) REFERENCES fiscal.documents(company_id, id),
    FOREIGN KEY (actor_id) REFERENCES identity.accounts(id)
);
CREATE INDEX document_events_document_idx ON fiscal.document_events(company_id, document_id, occurred_at);

CREATE TABLE fiscal.document_artifacts (
    company_id uuid NOT NULL,
    document_id uuid NOT NULL,
    kind text NOT NULL CHECK (kind IN ('xml', 'pdf')),
    content bytea NOT NULL CHECK (octet_length(content) > 0),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (company_id, document_id, kind),
    FOREIGN KEY (company_id, document_id) REFERENCES fiscal.documents(company_id, id)
);

CREATE FUNCTION fiscal.validate_transition() RETURNS trigger
LANGUAGE plpgsql SET search_path = fiscal, pg_temp AS $$
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
CREATE TRIGGER documents_transition BEFORE UPDATE ON fiscal.documents
FOR EACH ROW EXECUTE FUNCTION fiscal.validate_transition();

GRANT USAGE ON SCHEMA fiscal TO fiscal_app;
GRANT SELECT, INSERT ON fiscal.issuer_profiles, fiscal.fact_snapshots,
    fiscal.documents, fiscal.document_items, fiscal.document_events,
    fiscal.document_artifacts TO fiscal_app;
GRANT UPDATE (state, external_key) ON fiscal.documents TO fiscal_app;

ALTER TABLE fiscal.issuer_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE fiscal.issuer_profiles FORCE ROW LEVEL SECURITY;
ALTER TABLE fiscal.fact_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE fiscal.fact_snapshots FORCE ROW LEVEL SECURITY;
ALTER TABLE fiscal.documents ENABLE ROW LEVEL SECURITY;
ALTER TABLE fiscal.documents FORCE ROW LEVEL SECURITY;
ALTER TABLE fiscal.document_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE fiscal.document_items FORCE ROW LEVEL SECURITY;
ALTER TABLE fiscal.document_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE fiscal.document_events FORCE ROW LEVEL SECURITY;
ALTER TABLE fiscal.document_artifacts ENABLE ROW LEVEL SECURITY;
ALTER TABLE fiscal.document_artifacts FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_fiscal ON fiscal.issuer_profiles FOR ALL TO fiscal_app
USING (company_id = identity.current_company_id()
       AND identity.has_fiscal_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_fiscal_permission(company_id, identity.current_account_id()));
CREATE POLICY tenant_fiscal ON fiscal.fact_snapshots FOR ALL TO fiscal_app
USING (company_id = identity.current_company_id()
       AND identity.has_fiscal_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_fiscal_permission(company_id, identity.current_account_id()));
CREATE POLICY tenant_fiscal ON fiscal.documents FOR ALL TO fiscal_app
USING (company_id = identity.current_company_id()
       AND identity.has_fiscal_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_fiscal_permission(company_id, identity.current_account_id()));
CREATE POLICY tenant_fiscal ON fiscal.document_items FOR ALL TO fiscal_app
USING (company_id = identity.current_company_id()
       AND identity.has_fiscal_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_fiscal_permission(company_id, identity.current_account_id()));
CREATE POLICY tenant_fiscal ON fiscal.document_events FOR ALL TO fiscal_app
USING (company_id = identity.current_company_id()
       AND identity.has_fiscal_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_fiscal_permission(company_id, identity.current_account_id()));
CREATE POLICY tenant_fiscal ON fiscal.document_artifacts FOR ALL TO fiscal_app
USING (company_id = identity.current_company_id()
       AND identity.has_fiscal_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_fiscal_permission(company_id, identity.current_account_id()));
