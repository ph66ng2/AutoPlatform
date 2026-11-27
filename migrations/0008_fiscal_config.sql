-- Cadastro fiscal aditivo. Decisões são aprovadas por fato, estabelecimento e
-- cenário: o cadastro nunca inventa valores nem classifica peças pelo legado.
ALTER TABLE fiscal.issuer_profiles
    ADD COLUMN municipality_ibge_code text,
    ADD COLUMN tax_regime text,
    ADD COLUMN review_status text NOT NULL DEFAULT 'draft'
        CHECK (review_status IN ('draft', 'ready')),
    ADD CONSTRAINT issuer_municipality_code CHECK (
        municipality_ibge_code IS NULL OR municipality_ibge_code ~ '^[0-9]{7}$'
    ),
    ADD CONSTRAINT issuer_ready_complete CHECK (
        review_status <> 'ready' OR
        (municipality_ibge_code IS NOT NULL AND tax_regime IS NOT NULL AND tax_regime <> '')
    );

CREATE TABLE fiscal.item_profiles (
    company_id uuid NOT NULL,
    establishment_id uuid NOT NULL,
    item_key text NOT NULL CHECK (item_key <> ''),
    description text NOT NULL CHECK (description <> ''),
    item_kind text NOT NULL CHECK (item_kind IN ('service', 'product')),
    piece_source text NOT NULL CHECK (piece_source IN ('none', 'customer', 'workshop')),
    reviewed_at timestamptz,
    reviewed_by uuid,
    reviewed_product text NOT NULL DEFAULT 'autobo' CHECK (reviewed_product = 'autobo'),
    PRIMARY KEY (company_id, establishment_id, item_key),
    FOREIGN KEY (company_id, establishment_id)
        REFERENCES fiscal.issuer_profiles(company_id, establishment_id),
    FOREIGN KEY (company_id, reviewed_by, reviewed_product)
        REFERENCES identity.memberships(company_id, account_id, product),
    CHECK ((reviewed_at IS NULL) = (reviewed_by IS NULL))
);

CREATE TABLE fiscal.operation_scenarios (
    company_id uuid NOT NULL,
    establishment_id uuid NOT NULL,
    scenario text NOT NULL CHECK (scenario <> ''),
    operation_type text NOT NULL CHECK (operation_type IN ('service', 'goods', 'mixed')),
    piece_source text NOT NULL CHECK (piece_source IN ('none', 'customer', 'workshop')),
    pricing_mode text NOT NULL CHECK (pricing_mode IN ('itemized', 'bundle')),
    review_status text NOT NULL DEFAULT 'draft' CHECK (review_status IN ('draft', 'approved')),
    approved_by uuid,
    approved_product text NOT NULL DEFAULT 'autobo' CHECK (approved_product = 'autobo'),
    approved_at timestamptz,
    effective_from timestamptz,
    effective_until timestamptz,
    evidence_ref text,
    PRIMARY KEY (company_id, establishment_id, scenario),
    FOREIGN KEY (company_id, establishment_id)
        REFERENCES fiscal.issuer_profiles(company_id, establishment_id),
    FOREIGN KEY (company_id, approved_by, approved_product)
        REFERENCES identity.memberships(company_id, account_id, product),
    CHECK (review_status <> 'approved' OR
        (approved_by IS NOT NULL AND approved_at IS NOT NULL AND
         effective_from IS NOT NULL AND evidence_ref IS NOT NULL AND evidence_ref <> '')),
    CHECK (effective_until IS NULL OR effective_from IS NULL OR effective_until > effective_from)
);

CREATE FUNCTION fiscal.guard_reviewed_item() RETURNS trigger
LANGUAGE plpgsql SET search_path = fiscal, pg_temp AS $$
BEGIN
    IF OLD.reviewed_at IS NOT NULL THEN
        RAISE EXCEPTION 'fiscal:reviewed-item-immutable';
    END IF;
    IF ROW(NEW.company_id, NEW.establishment_id, NEW.item_key,
           NEW.description, NEW.item_kind, NEW.piece_source)
       IS DISTINCT FROM
       ROW(OLD.company_id, OLD.establishment_id, OLD.item_key,
           OLD.description, OLD.item_kind, OLD.piece_source) THEN
        RAISE EXCEPTION 'fiscal:item-identity-immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER fiscal_items_guard BEFORE UPDATE ON fiscal.item_profiles
FOR EACH ROW EXECUTE FUNCTION fiscal.guard_reviewed_item();

CREATE TABLE fiscal.decisions (
    company_id uuid NOT NULL,
    establishment_id uuid NOT NULL,
    scenario text NOT NULL,
    fact_id uuid NOT NULL,
    version bigint NOT NULL CHECK (version > 0),
    documents jsonb NOT NULL CHECK (jsonb_typeof(documents) = 'array'),
    review_status text NOT NULL DEFAULT 'draft' CHECK (review_status IN ('draft', 'approved')),
    approved_by uuid,
    approved_product text NOT NULL DEFAULT 'autobo' CHECK (approved_product = 'autobo'),
    approved_at timestamptz,
    effective_from timestamptz,
    effective_until timestamptz,
    evidence_ref text,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (company_id, establishment_id, scenario, fact_id, version),
    FOREIGN KEY (company_id, establishment_id, scenario)
        REFERENCES fiscal.operation_scenarios(company_id, establishment_id, scenario),
    FOREIGN KEY (company_id, approved_by, approved_product)
        REFERENCES identity.memberships(company_id, account_id, product),
    CHECK (review_status <> 'approved' OR
        (approved_by IS NOT NULL AND approved_at IS NOT NULL AND
         effective_from IS NOT NULL AND evidence_ref IS NOT NULL AND evidence_ref <> '')),
    CHECK (effective_until IS NULL OR effective_from IS NULL OR effective_until > effective_from)
);
CREATE INDEX fiscal_decisions_resolve_idx ON fiscal.decisions
    (company_id, establishment_id, scenario, fact_id, version DESC)
    WHERE review_status = 'approved';

CREATE FUNCTION fiscal.guard_approved_scenario() RETURNS trigger
LANGUAGE plpgsql SET search_path = fiscal, pg_temp AS $$
BEGIN
    IF OLD.review_status = 'approved' THEN
        RAISE EXCEPTION 'fiscal:approved-scenario-immutable';
    END IF;
    IF ROW(NEW.company_id, NEW.establishment_id, NEW.scenario,
           NEW.operation_type, NEW.piece_source, NEW.pricing_mode)
       IS DISTINCT FROM
       ROW(OLD.company_id, OLD.establishment_id, OLD.scenario,
           OLD.operation_type, OLD.piece_source, OLD.pricing_mode) THEN
        RAISE EXCEPTION 'fiscal:scenario-identity-immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER fiscal_scenarios_guard BEFORE UPDATE ON fiscal.operation_scenarios
FOR EACH ROW EXECUTE FUNCTION fiscal.guard_approved_scenario();

CREATE FUNCTION fiscal.guard_approved_decision() RETURNS trigger
LANGUAGE plpgsql SET search_path = fiscal, pg_temp AS $$
BEGIN
    IF OLD.review_status = 'approved' THEN
        RAISE EXCEPTION 'fiscal:approved-decision-immutable';
    END IF;
    IF ROW(NEW.company_id, NEW.establishment_id, NEW.scenario, NEW.fact_id,
           NEW.version, NEW.documents, NEW.created_at)
       IS DISTINCT FROM
       ROW(OLD.company_id, OLD.establishment_id, OLD.scenario, OLD.fact_id,
           OLD.version, OLD.documents, OLD.created_at) THEN
        RAISE EXCEPTION 'fiscal:decision-identity-immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER fiscal_decisions_guard BEFORE UPDATE ON fiscal.decisions
FOR EACH ROW EXECUTE FUNCTION fiscal.guard_approved_decision();

CREATE TABLE fiscal.provider_configs (
    company_id uuid NOT NULL,
    establishment_id uuid NOT NULL,
    provider text NOT NULL CHECK (provider <> ''),
    key_id text NOT NULL CHECK (key_id <> ''),
    wrapped_dek bytea NOT NULL CHECK (octet_length(wrapped_dek) > 28),
    encrypted_secret bytea NOT NULL CHECK (octet_length(encrypted_secret) > 28),
    rotated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (company_id, establishment_id, provider),
    FOREIGN KEY (company_id, establishment_id)
        REFERENCES fiscal.issuer_profiles(company_id, establishment_id)
);

GRANT SELECT, INSERT, UPDATE ON fiscal.item_profiles,
    fiscal.operation_scenarios, fiscal.decisions TO fiscal_app;
GRANT SELECT, INSERT, UPDATE ON fiscal.provider_configs TO fiscal_app;
GRANT UPDATE (municipality_ibge_code, tax_regime, review_status)
    ON fiscal.issuer_profiles TO fiscal_app;

ALTER TABLE fiscal.item_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE fiscal.item_profiles FORCE ROW LEVEL SECURITY;
ALTER TABLE fiscal.operation_scenarios ENABLE ROW LEVEL SECURITY;
ALTER TABLE fiscal.operation_scenarios FORCE ROW LEVEL SECURITY;
ALTER TABLE fiscal.decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE fiscal.decisions FORCE ROW LEVEL SECURITY;
ALTER TABLE fiscal.provider_configs ENABLE ROW LEVEL SECURITY;
ALTER TABLE fiscal.provider_configs FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_fiscal ON fiscal.item_profiles FOR ALL TO fiscal_app
USING (company_id = identity.current_company_id()
       AND identity.has_fiscal_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_fiscal_permission(company_id, identity.current_account_id()));
CREATE POLICY tenant_fiscal ON fiscal.operation_scenarios FOR ALL TO fiscal_app
USING (company_id = identity.current_company_id()
       AND identity.has_fiscal_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_fiscal_permission(company_id, identity.current_account_id()));
CREATE POLICY tenant_fiscal ON fiscal.decisions FOR ALL TO fiscal_app
USING (company_id = identity.current_company_id()
       AND identity.has_fiscal_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_fiscal_permission(company_id, identity.current_account_id()));
CREATE POLICY tenant_fiscal ON fiscal.provider_configs FOR ALL TO fiscal_app
USING (company_id = identity.current_company_id()
       AND identity.has_fiscal_permission(company_id, identity.current_account_id()))
WITH CHECK (company_id = identity.current_company_id()
            AND identity.has_fiscal_permission(company_id, identity.current_account_id()));
