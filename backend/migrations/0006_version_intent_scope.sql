ALTER TABLE trade_intent_versions
    ADD COLUMN instrument_ids UUID[] NOT NULL DEFAULT '{}'::uuid[],
    ADD COLUMN account_id UUID REFERENCES accounts(id),
    ADD COLUMN wallet_id UUID REFERENCES wallets(id),
    ADD COLUMN position_slice NUMERIC(20, 8);

UPDATE trade_intent_versions tiv
SET instrument_ids = scope.instrument_ids,
    account_id = scope.account_id,
    wallet_id = scope.wallet_id,
    position_slice = scope.position_slice
FROM (
    SELECT trade_intent_id,
           array_agg(instrument_id ORDER BY instrument_id) AS instrument_ids,
           (array_agg(account_id ORDER BY account_id) FILTER (WHERE account_id IS NOT NULL))[1] AS account_id,
           (array_agg(wallet_id ORDER BY wallet_id) FILTER (WHERE wallet_id IS NOT NULL))[1] AS wallet_id,
           min(position_slice) AS position_slice
    FROM position_intent_links
    GROUP BY trade_intent_id
) scope
WHERE scope.trade_intent_id = tiv.trade_intent_id;

ALTER TABLE trade_intent_versions
    ADD CONSTRAINT trade_intent_versions_instruments_check
        CHECK (cardinality(instrument_ids) > 0),
    ADD CONSTRAINT trade_intent_versions_owner_check
        CHECK ((account_id IS NOT NULL)::int + (wallet_id IS NOT NULL)::int <= 1);
