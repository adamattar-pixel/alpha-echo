CREATE UNIQUE INDEX trades_wallet_source_external_unique
    ON trades (wallet_id, source, external_id)
    WHERE wallet_id IS NOT NULL AND external_id IS NOT NULL;
