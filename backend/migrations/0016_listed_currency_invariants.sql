UPDATE accounts
SET currency = CASE market
    WHEN 'a_share' THEN 'RMB'
    WHEN 'hk' THEN 'HKD'
    WHEN 'us' THEN 'USD'
    ELSE currency
END
WHERE market IN ('a_share', 'hk', 'us');

UPDATE instruments
SET currency = CASE market
    WHEN 'a_share' THEN 'RMB'
    WHEN 'hk' THEN 'HKD'
    WHEN 'us' THEN 'USD'
    ELSE currency
END,
updated_at = now()
WHERE market IN ('a_share', 'hk', 'us');

ALTER TABLE accounts
    ADD CONSTRAINT listed_accounts_use_market_currency
    CHECK (
        (market = 'a_share' AND currency = 'RMB')
        OR (market = 'hk' AND currency = 'HKD')
        OR (market = 'us' AND currency = 'USD')
        OR market NOT IN ('a_share', 'hk', 'us')
    );

ALTER TABLE instruments
    ADD CONSTRAINT listed_instruments_use_market_currency
    CHECK (
        (market = 'a_share' AND currency = 'RMB')
        OR (market = 'hk' AND currency = 'HKD')
        OR (market = 'us' AND currency = 'USD')
        OR market NOT IN ('a_share', 'hk', 'us')
    );
