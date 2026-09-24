-- Estoque central. Ledger imutável; saldo não é coluna editável; reserva não mexe em on_hand.
-- Mutação offline é recusada. AutoBO é autoridade no bundle; o tenant vem da sessão, não do JWT.

CREATE SCHEMA inventory;

DO $$
BEGIN
    CREATE ROLE inventory_app NOLOGIN NOINHERIT NOBYPASSRLS;
EXCEPTION
    WHEN duplicate_object THEN NULL;
END
$$;

GRANT inventory_app TO CURRENT_USER;

GRANT USAGE ON SCHEMA identity TO inventory_app;
GRANT EXECUTE ON FUNCTION identity.current_company_id() TO inventory_app;
GRANT EXECUTE ON FUNCTION identity.current_account_id() TO inventory_app;
GRANT EXECUTE ON FUNCTION identity.has_active_membership(uuid, uuid) TO inventory_app;

CREATE TABLE inventory.warehouses (
    company_id uuid NOT NULL,
    id uuid NOT NULL,
    code text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (company_id, id),
    UNIQUE (company_id, code),
    FOREIGN KEY (company_id) REFERENCES identity.companies (id),
    CHECK (code = 'default')
);

CREATE TABLE inventory.products (
    company_id uuid NOT NULL,
    id uuid NOT NULL,
    sku text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (company_id, id),
    UNIQUE (company_id, sku),
    FOREIGN KEY (company_id) REFERENCES identity.companies (id),
    CHECK (sku <> '' AND char_length(sku) <= 64)
);

CREATE TABLE inventory.stock (
    company_id uuid NOT NULL,
    product_id uuid NOT NULL,
    warehouse_id uuid NOT NULL,
    PRIMARY KEY (company_id, product_id, warehouse_id),
    FOREIGN KEY (company_id, product_id) REFERENCES inventory.products (company_id, id),
    FOREIGN KEY (company_id, warehouse_id) REFERENCES inventory.warehouses (company_id, id)
);

CREATE TABLE inventory.movements (
    company_id uuid NOT NULL,
    id uuid NOT NULL,
    product_id uuid NOT NULL,
    warehouse_id uuid NOT NULL,
    quantity integer NOT NULL,
    reason text NOT NULL,
    operation_id text NOT NULL,
    reservation_id uuid,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (company_id, id),
    UNIQUE (company_id, operation_id),
    FOREIGN KEY (company_id, product_id) REFERENCES inventory.products (company_id, id),
    FOREIGN KEY (company_id, warehouse_id) REFERENCES inventory.warehouses (company_id, id),
    CHECK (reason IN ('receive', 'consume')),
    CHECK (
        (reason = 'receive' AND quantity > 0)
        OR (reason = 'consume' AND quantity < 0)
    )
);

CREATE TABLE inventory.reservations (
    company_id uuid NOT NULL,
    id uuid NOT NULL,
    product_id uuid NOT NULL,
    warehouse_id uuid NOT NULL,
    quantity integer NOT NULL,
    status text NOT NULL,
    operation_id text NOT NULL,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (company_id, id),
    UNIQUE (company_id, operation_id),
    FOREIGN KEY (company_id, product_id) REFERENCES inventory.products (company_id, id),
    FOREIGN KEY (company_id, warehouse_id) REFERENCES inventory.warehouses (company_id, id),
    CHECK (quantity > 0),
    CHECK (status IN ('open', 'committed', 'released', 'expired'))
);

CREATE TABLE inventory.commands (
    company_id uuid NOT NULL,
    operation_id text NOT NULL,
    kind text NOT NULL,
    sku text NOT NULL,
    result jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (company_id, operation_id),
    CHECK (char_length(operation_id) BETWEEN 16 AND 64),
    CHECK (kind IN ('receive', 'reserve', 'commit', 'release', 'expire', 'consume'))
);

CREATE FUNCTION inventory.require_online() RETURNS void
LANGUAGE plpgsql
STABLE
SET search_path = inventory, identity, pg_temp
AS $$
BEGIN
    IF coalesce(current_setting('app.inventory_mode', true), 'online') = 'offline' THEN
        RAISE EXCEPTION 'inventory:offline';
    END IF;
END;
$$;

CREATE FUNCTION inventory.require_context() RETURNS uuid
LANGUAGE plpgsql
STABLE
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_company uuid := identity.current_company_id();
    v_account uuid := identity.current_account_id();
BEGIN
    IF v_company IS NULL OR v_account IS NULL
        OR NOT identity.has_active_membership(v_company, v_account) THEN
        RAISE EXCEPTION 'inventory:denied';
    END IF;
    RETURN v_company;
END;
$$;

CREATE FUNCTION inventory.replay(p_company uuid, p_operation_id text, p_kind text)
RETURNS jsonb
LANGUAGE plpgsql
STABLE
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_kind text;
    v_result jsonb;
BEGIN
    SELECT kind, result INTO v_kind, v_result
      FROM commands
     WHERE company_id = p_company
       AND operation_id = p_operation_id;
    IF v_kind IS NULL THEN
        RETURN NULL;
    END IF;
    IF v_kind <> p_kind THEN
        RAISE EXCEPTION 'inventory:kind';
    END IF;
    RETURN v_result;
END;
$$;

CREATE FUNCTION inventory.ensure_stock(p_company uuid, p_sku text)
RETURNS TABLE(product_id uuid, warehouse_id uuid)
LANGUAGE plpgsql
VOLATILE
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_product uuid;
    v_warehouse uuid;
BEGIN
    INSERT INTO warehouses (company_id, id, code)
    VALUES (p_company, gen_random_uuid(), 'default')
    ON CONFLICT (company_id, code) DO NOTHING;

    SELECT w.id INTO v_warehouse
      FROM warehouses w
     WHERE w.company_id = p_company
       AND w.code = 'default';

    INSERT INTO products (company_id, id, sku)
    VALUES (p_company, gen_random_uuid(), p_sku)
    ON CONFLICT (company_id, sku) DO NOTHING;

    SELECT p.id INTO v_product
      FROM products p
     WHERE p.company_id = p_company
       AND p.sku = p_sku;

    INSERT INTO stock (company_id, product_id, warehouse_id)
    VALUES (p_company, v_product, v_warehouse)
    ON CONFLICT DO NOTHING;

    product_id := v_product;
    warehouse_id := v_warehouse;
    RETURN NEXT;
END;
$$;

CREATE FUNCTION inventory.lock_and_snapshot(p_company uuid, p_product uuid, p_warehouse uuid)
RETURNS TABLE(on_hand bigint, reserved bigint, available bigint)
LANGUAGE plpgsql
VOLATILE
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_on_hand bigint;
    v_reserved bigint;
BEGIN
    PERFORM 1
       FROM stock s
      WHERE s.company_id = p_company
        AND s.product_id = p_product
        AND s.warehouse_id = p_warehouse
      FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'inventory:sku';
    END IF;

    SELECT coalesce(sum(m.quantity), 0)::bigint INTO v_on_hand
      FROM movements m
     WHERE m.company_id = p_company
       AND m.product_id = p_product
       AND m.warehouse_id = p_warehouse;

    SELECT coalesce(sum(r.quantity), 0)::bigint INTO v_reserved
      FROM reservations r
     WHERE r.company_id = p_company
       AND r.product_id = p_product
       AND r.warehouse_id = p_warehouse
       AND r.status = 'open'
       AND r.expires_at > clock_timestamp();

    IF v_on_hand < 0 OR v_reserved < 0 OR v_on_hand < v_reserved THEN
        RAISE EXCEPTION 'inventory:invariant';
    END IF;

    on_hand := v_on_hand;
    reserved := v_reserved;
    available := v_on_hand - v_reserved;
    RETURN NEXT;
END;
$$;

CREATE FUNCTION inventory.pack(
    p_operation_id text,
    p_sku text,
    p_quantity integer,
    p_reason text,
    p_on_hand bigint,
    p_reserved bigint,
    p_available bigint,
    p_movement uuid,
    p_reservation uuid,
    p_replay boolean
) RETURNS jsonb
LANGUAGE sql
IMMUTABLE
SET search_path = inventory, pg_temp
AS $$
    SELECT jsonb_build_object(
        'operation_id', p_operation_id,
        'sku', p_sku,
        'quantity', p_quantity,
        'reason', p_reason,
        'on_hand', p_on_hand,
        'reserved', p_reserved,
        'available', p_available,
        'movement_id', p_movement,
        'reservation_id', p_reservation,
        'replay', p_replay
    )
$$;

CREATE FUNCTION inventory.register_product(p_sku text) RETURNS uuid
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_company uuid;
    v_ids record;
BEGIN
    PERFORM inventory.require_online();
    v_company := inventory.require_context();
    IF p_sku IS NULL OR p_sku = '' OR char_length(p_sku) > 64 THEN
        RAISE EXCEPTION 'inventory:sku';
    END IF;
    SELECT * INTO v_ids FROM inventory.ensure_stock(v_company, p_sku);
    RETURN v_ids.product_id;
END;
$$;

CREATE FUNCTION inventory.availability(p_sku text) RETURNS jsonb
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_company uuid;
    v_ids record;
    v_snap record;
BEGIN
    v_company := inventory.require_context();
    SELECT p.id AS product_id, w.id AS warehouse_id
      INTO v_ids
      FROM products p
      JOIN warehouses w
        ON w.company_id = p.company_id
       AND w.code = 'default'
     WHERE p.company_id = v_company
       AND p.sku = p_sku;
    IF v_ids.product_id IS NULL THEN
        RAISE EXCEPTION 'inventory:sku';
    END IF;
    SELECT * INTO v_snap
      FROM inventory.lock_and_snapshot(v_company, v_ids.product_id, v_ids.warehouse_id);
    RETURN jsonb_build_object(
        'sku', p_sku,
        'on_hand', v_snap.on_hand,
        'reserved', v_snap.reserved,
        'available', v_snap.available
    );
END;
$$;

CREATE FUNCTION inventory.receive(p_sku text, p_quantity integer, p_operation_id text) RETURNS jsonb
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_company uuid;
    v_ids record;
    v_snap record;
    v_movement uuid;
    v_result jsonb;
BEGIN
    PERFORM inventory.require_online();
    v_company := inventory.require_context();
    v_result := inventory.replay(v_company, p_operation_id, 'receive');
    IF v_result IS NOT NULL THEN
        RETURN v_result;
    END IF;
    IF p_quantity IS NULL OR p_quantity <= 0 THEN
        RAISE EXCEPTION 'inventory:quantity';
    END IF;
    SELECT * INTO v_ids FROM inventory.ensure_stock(v_company, p_sku);
    SELECT * INTO v_snap FROM inventory.lock_and_snapshot(v_company, v_ids.product_id, v_ids.warehouse_id);
    v_movement := gen_random_uuid();
    INSERT INTO movements (
        company_id, id, product_id, warehouse_id, quantity, reason, operation_id
    ) VALUES (
        v_company, v_movement, v_ids.product_id, v_ids.warehouse_id, p_quantity, 'receive', p_operation_id
    );
    v_snap.on_hand := v_snap.on_hand + p_quantity;
    v_snap.available := v_snap.on_hand - v_snap.reserved;
    v_result := inventory.pack(
        p_operation_id, p_sku, p_quantity, 'receive',
        v_snap.on_hand, v_snap.reserved, v_snap.available,
        v_movement, NULL, false
    );
    INSERT INTO commands (company_id, operation_id, kind, sku, result)
    VALUES (v_company, p_operation_id, 'receive', p_sku, v_result);
    RETURN v_result;
EXCEPTION
    WHEN unique_violation THEN
        v_result := inventory.replay(v_company, p_operation_id, 'receive');
        IF v_result IS NULL THEN
            RAISE;
        END IF;
        RETURN v_result;
END;
$$;

CREATE FUNCTION inventory.reserve(
    p_sku text,
    p_quantity integer,
    p_operation_id text,
    p_ttl text
) RETURNS jsonb
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_company uuid;
    v_ids record;
    v_snap record;
    v_reservation uuid;
    v_result jsonb;
BEGIN
    PERFORM inventory.require_online();
    v_company := inventory.require_context();
    v_result := inventory.replay(v_company, p_operation_id, 'reserve');
    IF v_result IS NOT NULL THEN
        RETURN v_result;
    END IF;
    IF p_quantity IS NULL OR p_quantity <= 0 THEN
        RAISE EXCEPTION 'inventory:quantity';
    END IF;
    SELECT p.id AS product_id, w.id AS warehouse_id
      INTO v_ids
      FROM products p
      JOIN warehouses w
        ON w.company_id = p.company_id
       AND w.code = 'default'
     WHERE p.company_id = v_company
       AND p.sku = p_sku;
    IF v_ids.product_id IS NULL THEN
        RAISE EXCEPTION 'inventory:sku';
    END IF;
    SELECT * INTO v_snap FROM inventory.lock_and_snapshot(v_company, v_ids.product_id, v_ids.warehouse_id);
    IF v_snap.available < p_quantity THEN
        RAISE EXCEPTION 'inventory:insufficient';
    END IF;
    v_reservation := gen_random_uuid();
    INSERT INTO reservations (
        company_id, id, product_id, warehouse_id, quantity, status, operation_id, expires_at
    ) VALUES (
        v_company, v_reservation, v_ids.product_id, v_ids.warehouse_id,
        p_quantity, 'open', p_operation_id, clock_timestamp() + p_ttl::interval
    );
    v_snap.reserved := v_snap.reserved + p_quantity;
    v_snap.available := v_snap.on_hand - v_snap.reserved;
    v_result := inventory.pack(
        p_operation_id, p_sku, -p_quantity, 'reserve',
        v_snap.on_hand, v_snap.reserved, v_snap.available,
        NULL, v_reservation, false
    );
    INSERT INTO commands (company_id, operation_id, kind, sku, result)
    VALUES (v_company, p_operation_id, 'reserve', p_sku, v_result);
    RETURN v_result;
EXCEPTION
    WHEN unique_violation THEN
        v_result := inventory.replay(v_company, p_operation_id, 'reserve');
        IF v_result IS NULL THEN
            RAISE;
        END IF;
        RETURN v_result;
END;
$$;

CREATE FUNCTION inventory.consume(p_sku text, p_quantity integer, p_operation_id text) RETURNS jsonb
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_company uuid;
    v_ids record;
    v_snap record;
    v_movement uuid;
    v_result jsonb;
BEGIN
    PERFORM inventory.require_online();
    v_company := inventory.require_context();
    v_result := inventory.replay(v_company, p_operation_id, 'consume');
    IF v_result IS NOT NULL THEN
        RETURN v_result;
    END IF;
    IF p_quantity IS NULL OR p_quantity <= 0 THEN
        RAISE EXCEPTION 'inventory:quantity';
    END IF;
    SELECT p.id AS product_id, w.id AS warehouse_id
      INTO v_ids
      FROM products p
      JOIN warehouses w
        ON w.company_id = p.company_id
       AND w.code = 'default'
     WHERE p.company_id = v_company
       AND p.sku = p_sku;
    IF v_ids.product_id IS NULL THEN
        RAISE EXCEPTION 'inventory:sku';
    END IF;
    SELECT * INTO v_snap FROM inventory.lock_and_snapshot(v_company, v_ids.product_id, v_ids.warehouse_id);
    IF v_snap.available < p_quantity THEN
        RAISE EXCEPTION 'inventory:insufficient';
    END IF;
    v_movement := gen_random_uuid();
    INSERT INTO movements (
        company_id, id, product_id, warehouse_id, quantity, reason, operation_id
    ) VALUES (
        v_company, v_movement, v_ids.product_id, v_ids.warehouse_id, -p_quantity, 'consume', p_operation_id
    );
    v_snap.on_hand := v_snap.on_hand - p_quantity;
    v_snap.available := v_snap.on_hand - v_snap.reserved;
    v_result := inventory.pack(
        p_operation_id, p_sku, -p_quantity, 'consume',
        v_snap.on_hand, v_snap.reserved, v_snap.available,
        v_movement, NULL, false
    );
    INSERT INTO commands (company_id, operation_id, kind, sku, result)
    VALUES (v_company, p_operation_id, 'consume', p_sku, v_result);
    RETURN v_result;
EXCEPTION
    WHEN unique_violation THEN
        v_result := inventory.replay(v_company, p_operation_id, 'consume');
        IF v_result IS NULL THEN
            RAISE;
        END IF;
        RETURN v_result;
END;
$$;

CREATE FUNCTION inventory.commit_reservation(p_reserve_operation_id text, p_operation_id text) RETURNS jsonb
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_company uuid;
    v_res inventory.reservations%ROWTYPE;
    v_sku text;
    v_snap record;
    v_movement uuid;
    v_result jsonb;
BEGIN
    PERFORM inventory.require_online();
    v_company := inventory.require_context();
    v_result := inventory.replay(v_company, p_operation_id, 'commit');
    IF v_result IS NOT NULL THEN
        RETURN v_result;
    END IF;
    SELECT * INTO v_res
      FROM reservations
     WHERE company_id = v_company
       AND operation_id = p_reserve_operation_id;
    IF v_res.id IS NULL THEN
        RAISE EXCEPTION 'inventory:reservation';
    END IF;
    SELECT sku INTO v_sku FROM products WHERE company_id = v_company AND id = v_res.product_id;
    SELECT * INTO v_snap FROM inventory.lock_and_snapshot(v_company, v_res.product_id, v_res.warehouse_id);
    SELECT * INTO v_res
      FROM reservations
     WHERE company_id = v_company
       AND id = v_res.id;
    IF v_res.status <> 'open' OR v_res.expires_at <= clock_timestamp() THEN
        RAISE EXCEPTION 'inventory:reservation';
    END IF;
    v_movement := gen_random_uuid();
    INSERT INTO movements (
        company_id, id, product_id, warehouse_id, quantity, reason, operation_id, reservation_id
    ) VALUES (
        v_company, v_movement, v_res.product_id, v_res.warehouse_id, -v_res.quantity, 'consume', p_operation_id, v_res.id
    );
    UPDATE reservations
       SET status = 'committed'
     WHERE company_id = v_company
       AND id = v_res.id;
    v_snap.on_hand := v_snap.on_hand - v_res.quantity;
    v_snap.reserved := v_snap.reserved - v_res.quantity;
    v_snap.available := v_snap.on_hand - v_snap.reserved;
    v_result := inventory.pack(
        p_operation_id, v_sku, -v_res.quantity, 'consume',
        v_snap.on_hand, v_snap.reserved, v_snap.available,
        v_movement, v_res.id, false
    );
    INSERT INTO commands (company_id, operation_id, kind, sku, result)
    VALUES (v_company, p_operation_id, 'commit', v_sku, v_result);
    RETURN v_result;
EXCEPTION
    WHEN unique_violation THEN
        v_result := inventory.replay(v_company, p_operation_id, 'commit');
        IF v_result IS NULL THEN
            RAISE;
        END IF;
        RETURN v_result;
END;
$$;

CREATE FUNCTION inventory.release_reservation(p_reserve_operation_id text, p_operation_id text) RETURNS jsonb
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_company uuid;
    v_res inventory.reservations%ROWTYPE;
    v_sku text;
    v_snap record;
    v_result jsonb;
BEGIN
    PERFORM inventory.require_online();
    v_company := inventory.require_context();
    v_result := inventory.replay(v_company, p_operation_id, 'release');
    IF v_result IS NOT NULL THEN
        RETURN v_result;
    END IF;
    SELECT * INTO v_res
      FROM reservations
     WHERE company_id = v_company
       AND operation_id = p_reserve_operation_id;
    IF v_res.id IS NULL THEN
        RAISE EXCEPTION 'inventory:reservation';
    END IF;
    SELECT sku INTO v_sku FROM products WHERE company_id = v_company AND id = v_res.product_id;
    SELECT * INTO v_snap FROM inventory.lock_and_snapshot(v_company, v_res.product_id, v_res.warehouse_id);
    SELECT * INTO v_res
      FROM reservations
     WHERE company_id = v_company
       AND id = v_res.id;
    IF v_res.status <> 'open' THEN
        RAISE EXCEPTION 'inventory:reservation';
    END IF;
    UPDATE reservations
       SET status = 'released'
     WHERE company_id = v_company
       AND id = v_res.id;
    IF v_res.expires_at > clock_timestamp() THEN
        v_snap.reserved := v_snap.reserved - v_res.quantity;
        v_snap.available := v_snap.on_hand - v_snap.reserved;
    END IF;
    v_result := inventory.pack(
        p_operation_id, v_sku, v_res.quantity, 'release',
        v_snap.on_hand, v_snap.reserved, v_snap.available,
        NULL, v_res.id, false
    );
    INSERT INTO commands (company_id, operation_id, kind, sku, result)
    VALUES (v_company, p_operation_id, 'release', v_sku, v_result);
    RETURN v_result;
EXCEPTION
    WHEN unique_violation THEN
        v_result := inventory.replay(v_company, p_operation_id, 'release');
        IF v_result IS NULL THEN
            RAISE;
        END IF;
        RETURN v_result;
END;
$$;

CREATE FUNCTION inventory.expire_reservation(p_reserve_operation_id text, p_operation_id text) RETURNS jsonb
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_company uuid;
    v_res inventory.reservations%ROWTYPE;
    v_sku text;
    v_snap record;
    v_result jsonb;
BEGIN
    PERFORM inventory.require_online();
    v_company := inventory.require_context();
    v_result := inventory.replay(v_company, p_operation_id, 'expire');
    IF v_result IS NOT NULL THEN
        RETURN v_result;
    END IF;
    SELECT * INTO v_res
      FROM reservations
     WHERE company_id = v_company
       AND operation_id = p_reserve_operation_id;
    IF v_res.id IS NULL THEN
        RAISE EXCEPTION 'inventory:reservation';
    END IF;
    SELECT sku INTO v_sku FROM products WHERE company_id = v_company AND id = v_res.product_id;
    SELECT * INTO v_snap FROM inventory.lock_and_snapshot(v_company, v_res.product_id, v_res.warehouse_id);
    SELECT * INTO v_res
      FROM reservations
     WHERE company_id = v_company
       AND id = v_res.id;
    IF v_res.status = 'expired' THEN
        v_result := inventory.pack(
            p_operation_id, v_sku, v_res.quantity, 'release',
            v_snap.on_hand, v_snap.reserved, v_snap.available,
            NULL, v_res.id, false
        );
        INSERT INTO commands (company_id, operation_id, kind, sku, result)
        VALUES (v_company, p_operation_id, 'expire', v_sku, v_result);
        RETURN v_result;
    END IF;
    IF v_res.status <> 'open' OR v_res.expires_at > clock_timestamp() THEN
        RAISE EXCEPTION 'inventory:reservation';
    END IF;
    UPDATE reservations
       SET status = 'expired'
     WHERE company_id = v_company
       AND id = v_res.id;
    v_result := inventory.pack(
        p_operation_id, v_sku, v_res.quantity, 'release',
        v_snap.on_hand, v_snap.reserved, v_snap.available,
        NULL, v_res.id, false
    );
    INSERT INTO commands (company_id, operation_id, kind, sku, result)
    VALUES (v_company, p_operation_id, 'expire', v_sku, v_result);
    RETURN v_result;
EXCEPTION
    WHEN unique_violation THEN
        v_result := inventory.replay(v_company, p_operation_id, 'expire');
        IF v_result IS NULL THEN
            RAISE;
        END IF;
        RETURN v_result;
END;
$$;

CREATE FUNCTION inventory.reconcile() RETURNS jsonb
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = inventory, identity, pg_temp
AS $$
DECLARE
    v_company uuid;
    v_count integer;
BEGIN
    PERFORM inventory.require_online();
    v_company := inventory.require_context();
    UPDATE reservations
       SET status = 'expired'
     WHERE company_id = v_company
       AND status = 'open'
       AND expires_at <= clock_timestamp();
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN jsonb_build_object('expired', v_count);
END;
$$;

REVOKE ALL ON FUNCTION inventory.require_online() FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.require_context() FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.replay(uuid, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.ensure_stock(uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.lock_and_snapshot(uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.pack(text, text, integer, text, bigint, bigint, bigint, uuid, uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.register_product(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.availability(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.receive(text, integer, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.reserve(text, integer, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.consume(text, integer, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.commit_reservation(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.release_reservation(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.expire_reservation(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION inventory.reconcile() FROM PUBLIC;

GRANT EXECUTE ON FUNCTION inventory.register_product(text) TO inventory_app;
GRANT EXECUTE ON FUNCTION inventory.availability(text) TO inventory_app;
GRANT EXECUTE ON FUNCTION inventory.receive(text, integer, text) TO inventory_app;
GRANT EXECUTE ON FUNCTION inventory.reserve(text, integer, text, text) TO inventory_app;
GRANT EXECUTE ON FUNCTION inventory.consume(text, integer, text) TO inventory_app;
GRANT EXECUTE ON FUNCTION inventory.commit_reservation(text, text) TO inventory_app;
GRANT EXECUTE ON FUNCTION inventory.release_reservation(text, text) TO inventory_app;
GRANT EXECUTE ON FUNCTION inventory.expire_reservation(text, text) TO inventory_app;
GRANT EXECUTE ON FUNCTION inventory.reconcile() TO inventory_app;

ALTER TABLE inventory.warehouses ENABLE ROW LEVEL SECURITY;
ALTER TABLE inventory.warehouses FORCE ROW LEVEL SECURITY;
ALTER TABLE inventory.products ENABLE ROW LEVEL SECURITY;
ALTER TABLE inventory.products FORCE ROW LEVEL SECURITY;
ALTER TABLE inventory.stock ENABLE ROW LEVEL SECURITY;
ALTER TABLE inventory.stock FORCE ROW LEVEL SECURITY;
ALTER TABLE inventory.movements ENABLE ROW LEVEL SECURITY;
ALTER TABLE inventory.movements FORCE ROW LEVEL SECURITY;
ALTER TABLE inventory.reservations ENABLE ROW LEVEL SECURITY;
ALTER TABLE inventory.reservations FORCE ROW LEVEL SECURITY;
ALTER TABLE inventory.commands ENABLE ROW LEVEL SECURITY;
ALTER TABLE inventory.commands FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_scope ON inventory.warehouses
    FOR ALL
    USING (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    )
    WITH CHECK (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    );

CREATE POLICY tenant_scope ON inventory.products
    FOR ALL
    USING (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    )
    WITH CHECK (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    );

CREATE POLICY tenant_scope ON inventory.stock
    FOR ALL
    USING (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    )
    WITH CHECK (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    );

CREATE POLICY tenant_scope ON inventory.movements
    FOR ALL
    USING (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    )
    WITH CHECK (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    );

CREATE POLICY tenant_scope ON inventory.reservations
    FOR ALL
    USING (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    )
    WITH CHECK (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    );

CREATE POLICY tenant_scope ON inventory.commands
    FOR ALL
    USING (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    )
    WITH CHECK (
        company_id = identity.current_company_id()
        AND identity.has_active_membership(company_id, identity.current_account_id())
    );

GRANT USAGE ON SCHEMA inventory TO inventory_app;
GRANT SELECT ON inventory.warehouses, inventory.products, inventory.stock, inventory.movements, inventory.reservations, inventory.commands TO inventory_app;
