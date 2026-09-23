-- Identidade empresarial. Papel vale só para o produto da membership.
-- company_id do JWT não é coluna e não autoriza nada.

CREATE SCHEMA identity;

DO $$
BEGIN
    CREATE ROLE identity_app NOLOGIN NOINHERIT NOBYPASSRLS;
EXCEPTION
    WHEN duplicate_object THEN NULL;
END
$$;

GRANT identity_app TO CURRENT_USER;

CREATE TABLE identity.companies (
    id uuid PRIMARY KEY,
    label text NOT NULL,
    status text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (status IN ('active', 'suspended')),
    CHECK (label <> '')
);

CREATE TABLE identity.accounts (
    id uuid PRIMARY KEY,
    status text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (status IN ('active', 'suspended'))
);

CREATE TABLE identity.memberships (
    company_id uuid NOT NULL,
    id uuid NOT NULL,
    account_id uuid NOT NULL,
    product text NOT NULL,
    role text NOT NULL,
    status text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (company_id, id),
    UNIQUE (company_id, account_id, product),
    FOREIGN KEY (company_id) REFERENCES identity.companies (id),
    FOREIGN KEY (account_id) REFERENCES identity.accounts (id),
    CHECK (status IN ('active', 'suspended', 'revoked')),
    CHECK (
        (product = 'autoos' AND role IN ('admin', 'operator'))
        OR (product = 'autobo' AND role IN ('admin', 'fiscal', 'operator'))
    )
);

CREATE TABLE identity.audit_events (
    company_id uuid NOT NULL,
    id uuid NOT NULL,
    account_id uuid NOT NULL,
    action text NOT NULL,
    outcome text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (company_id, id),
    FOREIGN KEY (company_id) REFERENCES identity.companies (id),
    FOREIGN KEY (account_id) REFERENCES identity.accounts (id),
    CHECK (action IN ('membership_suspended', 'membership_revoked')),
    CHECK (outcome = 'success')
);

CREATE TABLE identity.session_touches (
    company_id uuid NOT NULL,
    id uuid NOT NULL,
    account_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (company_id, id),
    FOREIGN KEY (company_id) REFERENCES identity.companies (id),
    FOREIGN KEY (account_id) REFERENCES identity.accounts (id)
);

CREATE FUNCTION identity.current_company_id() RETURNS uuid
LANGUAGE sql
STABLE
SET search_path = identity, pg_temp
AS $$
    SELECT CASE
        WHEN coalesce(current_setting('app.company_id', true), '') = '' THEN NULL
        ELSE current_setting('app.company_id', true)::uuid
    END
$$;

CREATE FUNCTION identity.current_account_id() RETURNS uuid
LANGUAGE sql
STABLE
SET search_path = identity, pg_temp
AS $$
    SELECT CASE
        WHEN coalesce(current_setting('app.account_id', true), '') = '' THEN NULL
        ELSE current_setting('app.account_id', true)::uuid
    END
$$;

CREATE FUNCTION identity.has_active_membership(p_company uuid, p_account uuid) RETURNS boolean
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = identity, pg_temp
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM memberships
        WHERE company_id = p_company
          AND account_id = p_account
          AND status = 'active'
    )
      AND EXISTS (
        SELECT 1 FROM companies WHERE id = p_company AND status = 'active'
    )
      AND EXISTS (
        SELECT 1 FROM accounts WHERE id = p_account AND status = 'active'
    )
$$;

CREATE FUNCTION identity.set_membership_status(
    p_company uuid,
    p_membership uuid,
    p_status text
) RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = identity, pg_temp
AS $$
DECLARE
    v_account uuid;
BEGIN
    IF p_status NOT IN ('suspended', 'revoked') THEN
        RAISE EXCEPTION 'status de membership inválido';
    END IF;
    UPDATE memberships
       SET status = p_status
     WHERE company_id = p_company
       AND id = p_membership
    RETURNING account_id INTO v_account;
    IF v_account IS NULL THEN
        RETURN;
    END IF;
    INSERT INTO audit_events (company_id, id, account_id, action, outcome)
    VALUES (
        p_company,
        gen_random_uuid(),
        v_account,
        'membership_' || p_status,
        'success'
    );
END;
$$;

REVOKE ALL ON FUNCTION identity.current_company_id() FROM PUBLIC;
REVOKE ALL ON FUNCTION identity.current_account_id() FROM PUBLIC;
REVOKE ALL ON FUNCTION identity.has_active_membership(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION identity.set_membership_status(uuid, uuid, text) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION identity.current_company_id() TO identity_app;
GRANT EXECUTE ON FUNCTION identity.current_account_id() TO identity_app;
GRANT EXECUTE ON FUNCTION identity.has_active_membership(uuid, uuid) TO identity_app;

ALTER TABLE identity.companies ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.companies FORCE ROW LEVEL SECURITY;
ALTER TABLE identity.memberships ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.memberships FORCE ROW LEVEL SECURITY;
ALTER TABLE identity.audit_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.audit_events FORCE ROW LEVEL SECURITY;
ALTER TABLE identity.session_touches ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.session_touches FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_scope ON identity.companies
    FOR ALL
    USING (
        id = identity.current_company_id()
        AND identity.has_active_membership(id, identity.current_account_id())
    )
    WITH CHECK (
        id = identity.current_company_id()
        AND identity.has_active_membership(id, identity.current_account_id())
    );

CREATE POLICY tenant_scope ON identity.memberships
    FOR ALL
    USING (
        company_id = identity.current_company_id()
        AND account_id = identity.current_account_id()
        AND identity.has_active_membership(company_id, account_id)
    )
    WITH CHECK (
        company_id = identity.current_company_id()
        AND account_id = identity.current_account_id()
        AND identity.has_active_membership(company_id, account_id)
    );

CREATE POLICY tenant_scope ON identity.audit_events
    FOR ALL
    USING (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    )
    WITH CHECK (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    );

CREATE POLICY tenant_scope ON identity.session_touches
    FOR ALL
    USING (
        company_id = identity.current_company_id()
        AND account_id = identity.current_account_id()
        AND identity.has_active_membership(company_id, account_id)
    )
    WITH CHECK (
        company_id = identity.current_company_id()
        AND account_id = identity.current_account_id()
        AND identity.has_active_membership(company_id, account_id)
    );

GRANT USAGE ON SCHEMA identity TO identity_app;
GRANT SELECT ON identity.companies, identity.memberships, identity.audit_events TO identity_app;
GRANT SELECT, INSERT, UPDATE ON identity.session_touches TO identity_app;
