---
name: diagnose-equity-positions
description: Diagnose one frozen A-share, HK, or US equity position against its active Trade Intent using macro, industry, company, market-pricing, expectation, catalyst, pressure, and core-risk dimensions. Run independently for every holding after shared context is validated.
---

# Diagnose equity positions

Apply `apply-investment-research-standards`. Read `task_scope.market`, then load exactly one matching source reference: [A share](references/a-share.md), [HK](references/hk.md), or [US](references/us.md).

Use `frozen_portfolio.report_language`: `en` means English and `zh` means Simplified Chinese. Legacy inputs without this field use Chinese. Apply it to all generated human-readable text, including headings, summaries, evidence titles, expectations, conditions, and drafts. Preserve original identifiers, URLs, quotations and user-authored facts.

## Unit of diagnosis

Each run receives exactly one non-zero frozen `position_snapshot_item_id` and must produce exactly one `equity_position_diagnosis` finding for it. Match the exact account, instrument, position snapshot item, and active Trade Intent version. Every diagnosis must cite exactly one `frozen_input` evidence item typed `position_snapshot_item` with that row's `position_snapshot_item_id`.

When an active Intent version matches the exact instrument and account, set `analysis.intent_assessment` to `matched`, set `analysis.intent_draft` to null, cite that `trade_intent_version` as frozen evidence, use it as the affected object, and assess its impact. When none matches, set `intent_assessment` to `missing`, affect the `position_snapshot_item`, set impact direction/strength and conditional action to null, and record the missing Intent as a data gap. Also write an `analysis.intent_draft` in the frozen report language containing a proposed thesis, time horizon, catalysts, and falsifiable invalidation conditions. Build it only from validated evidence and diagnostic dimensions. It is a model proposal awaiting user review: it is not the user's historical reason, it is not active, and it cannot support a trade action until confirmed. You may still organize verified macro, industry, company, pricing, expectation, catalyst, pressure, and risk facts, but must not claim that a nonexistent user thesis was strengthened, weakened, invalidated, or unchanged. A frozen row without a valid `position_snapshot_item_id` is rejected before research rather than silently grouped or omitted. There is no supported object type named `position`.

## Workflow

1. Reuse validated macro and market context. Do not research those layers again.
2. Research the single scoped holding. Start by inspecting its matching frozen `equity_news` source snapshot from the `equity_news_rss` Adapter, then use live search to verify and deepen material items. First collect company-specific evidence published inside the incremental report window. Then search backward adaptively for the latest material baseline: the most recent results, current guidance or management outlook, and material company, competitor, supply-chain, regulatory, or policy events. The report window defines what is new; it is not the historical-search cutoff. Start around 90 days, extend to 180 days, and then reach the latest fiscal or annual result when coverage is sparse, while never using evidence after `as_of`. No incremental news means the latest verified baseline becomes the primary context; it does not mean the company-news section is empty.
3. Compare four layers: macro, industry, company, and market pricing.
4. At every layer distinguish current fact, expected path, consensus or price-implied expectation, and the resulting expectation gap.
5. Separate upside catalysts, downside pressures that weaken but do not invalidate, and core risks that can invalidate the thesis.
6. Only for `intent_assessment=matched`, judge the Intent as `strengthened`, `weakened`, `invalidated`, or `unchanged`; price movement alone cannot determine the result.
7. Give observable confirmation and invalidation conditions and a conditional response. Do not decide absolute position size.

## Required diagnostic contract

Populate `analysis` on every diagnosis with `intent_assessment`, `intent_draft`, plus non-empty `macro_context`, `industry_context`, `company_context`, `market_pricing`, `future_expectation`, and `expectation_gap` strings. Include arrays for `upside_catalysts`, `downside_pressures`, and `core_risks`; use empty arrays only when evidence supports none.

Return an explicit unchanged diagnosis when sufficient evidence shows no thesis change. Return a data gap rather than a generic view when company or expectation coverage is inadequate.

## Company-news coverage

For every holding, run explicit symbol plus issuer-name searches and primary-source searches against the issuer, exchange, and regulator appropriate to the market. Prefer at least two distinct company-specific sources when available. Shared macro, index, or sector evidence does not satisfy company-news coverage. Clearly distinguish:

- `本轮新增`: published on or after `research_window_start`;
- `近期背景`: older than `research_window_start` but needed to understand the latest results, guidance, expectations, or risks.

When `本轮新增` is empty, prominently return the latest verified `近期背景` with its publication date and use it in `company_context`, `future_expectation`, `expectation_gap`, catalysts, pressures, and core risks. Do not merely emit an empty-news message. Only report a company-evidence gap after searching through the latest fiscal or annual disclosure.

When no reliable company-specific source is found, name the missing source classes in `data_gaps`; never relabel generic market news as company news. For ETFs and leveraged products, replace issuer-company research with product-provider documents, underlying-index/theme evidence, NAV or premium-discount evidence, tracking behavior, leverage decay, and rebalance mechanics where applicable.

Material industry, policy, competitor, or supply-chain news may use `coverage_scope="industry"` and bind `related_instrument_id` to this exact holding only with `relationship_basis="source_names_instrument"` plus a source excerpt that names the symbol/company, or `relationship_basis="direct_transmission"` plus a `relationship_explanation` in the report language stating the direct causal chain. Every `direct_transmission` row also requires a finding-linked `transmission_path` that cites the same evidence and targets the exact related instrument. Keep it separate from company/product news; it never satisfies the required company/product coverage by itself.
