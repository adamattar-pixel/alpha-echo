# Research finding contract

New equity research supports only `macro`, `a_share`, `hk`, `us`, `cross` and
`portfolio` markets. Crypto and Meme findings/events/narratives/transmission paths
are not accepted in stock reports.

Every finding must include:

- `finding_type`, `market`, `window_start`, `window_end`
- affected position, Trade Intent, narrative, or explicit `null`
- `fact_summary` and evidence IDs
- `inference` or explicit `null`
- `impact_direction`, `impact_strength` from 0–5, and `time_horizon`
- `confidence` from 0–1 with evidence quality reflected in the value
- contrary evidence, confirmation conditions, invalidation conditions
- conditional action and data gaps

Every `equity_position_diagnosis` must also include structured `analysis` with `intent_assessment` (`matched` or `missing`), `intent_draft`, macro, industry, company, market-pricing, future-expectation and expectation-gap text, plus separate upside catalysts, downside pressures, and core risks. A missing Intent makes the finding non-directional and non-actionable, but must include a model-proposed `intent_draft` in the report language for user review. A matched Intent must set that draft to null. Other finding types must set `analysis` to `null`.

Supported frozen evidence types are `instrument`, `position_snapshot`, `position_snapshot_item`, `trade_intent`, `trade_intent_version`, `account`, `market_narrative`, `market_event`, `research_finding`, and `source_snapshot`. Wallet and tracked-wallet objects are retired and cannot be cited in new stock research. A frozen position row is cited as `position_snapshot_item` with `position_snapshot_item_id`; `position` is not a supported type. Aggregate findings use `market` or `portfolio` with a null ID.

Every evidence row classifies `coverage_scope` as `position`, `company`, `product`, `industry`, `market`, or `macro`. `company` and `product` evidence must bind `related_instrument_id` to the exact diagnosed holding and cannot be a frozen portfolio fact. Material `industry` evidence may bind the exact holding only with a structured `relationship_basis` proving that the source names the instrument or a non-empty `relationship_explanation` in the report language recording the direct transmission. Direct-transmission evidence also requires a finding-linked transmission path that cites the same evidence and targets the exact related instrument. It remains industry evidence and does not satisfy company-news coverage. Other scopes set the instrument ID and relationship fields to null. An equity diagnosis must cite matching `company` or `product` evidence, or explicitly disclose in `data_gaps` that this coverage was not found and name the issuer/IR, exchange, regulator, or product-provider source classes searched. Macro, market, and industry evidence cannot be presented as company news. Missing publication time is never replaced by observation time when deciding whether news belongs to the incremental report window. A non-null publication time is either RFC3339 or a calendar date `YYYY-MM-DD`; date-only evidence is interpreted conservatively at the end of that UTC day for cutoff checks.

The enclosing run must include `as_of`, input snapshot IDs, Skill name and version, model, schema version, raw structured output, validation status, and errors/data gaps.

Reject a finding when an evidence ID is missing, a source violates the cutoff, an affected object does not exist, an enum is invalid, or confidence is outside 0–1.
