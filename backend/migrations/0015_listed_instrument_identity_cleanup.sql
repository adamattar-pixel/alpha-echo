UPDATE instruments
SET chain = NULL,
    contract_address = NULL,
    updated_at = now()
WHERE market IN ('a_share', 'hk', 'us')
  AND (chain IS NOT NULL OR contract_address IS NOT NULL);

ALTER TABLE instruments
    ADD CONSTRAINT listed_instruments_have_no_chain_identity
    CHECK (
        market NOT IN ('a_share', 'hk', 'us')
        OR (chain IS NULL AND contract_address IS NULL)
    );
