ALTER TABLE briefing_runs
    ADD COLUMN raw_output JSONB;

ALTER TABLE research_runs
    ADD COLUMN supporting_skills JSONB NOT NULL DEFAULT '{}'::jsonb;

UPDATE instruments
SET contract_address = lower(contract_address)
WHERE contract_address IS NOT NULL
  AND lower(chain) IN (
      'ethereum', 'eth', 'bsc', 'bnb', 'base', 'arbitrum',
      'optimism', 'polygon', 'avalanche', 'fantom', 'linea', 'blast'
  );

DROP INDEX instruments_chain_contract_identity;

CREATE UNIQUE INDEX instruments_chain_contract_identity
    ON instruments (chain, contract_address)
    WHERE contract_address IS NOT NULL;
