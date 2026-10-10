ALTER TABLE trade_intent_versions
    ADD COLUMN title TEXT,
    ADD COLUMN strategy_type TEXT,
    ADD COLUMN time_horizon TEXT;

UPDATE trade_intent_versions tiv
SET title = ti.title,
    strategy_type = ti.strategy_type,
    time_horizon = ti.time_horizon
FROM trade_intents ti
WHERE ti.id = tiv.trade_intent_id;

ALTER TABLE trade_intent_versions
    ALTER COLUMN title SET NOT NULL,
    ALTER COLUMN strategy_type SET NOT NULL,
    ALTER COLUMN time_horizon SET NOT NULL;
