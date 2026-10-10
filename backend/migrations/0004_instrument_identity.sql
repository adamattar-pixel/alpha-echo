DROP INDEX instruments_listed_identity;

CREATE UNIQUE INDEX instruments_listed_identity
    ON instruments (market, COALESCE(exchange, ''), symbol)
    WHERE contract_address IS NULL;
