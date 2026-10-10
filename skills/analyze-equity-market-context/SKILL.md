---
name: analyze-equity-market-context
description: Analyze the market, industry, leadership, expectations, and pricing context needed by held A-share, HK, or US equities. Use once per active equity market before diagnosing its positions; do not scan for unrelated opportunities.
---

# Analyze equity market context

Apply `apply-investment-research-standards`. Read `task_scope.market`, then load exactly one matching reference: [A share](references/a-share.md), [HK](references/hk.md), or [US](references/us.md).

## Workflow

1. Use validated macro context and cutoff-safe market evidence.
2. Analyze only industries, themes, factors, and market conditions connected to frozen positions in the scoped market.
3. Establish market breadth, leadership, risk appetite, liquidity and valuation conditions only where supplied evidence supports them.
4. For each relevant industry, distinguish current facts, expected path, market consensus, and what prices appear to imply.
5. Identify explicit or latent narratives, crowding, contradictory evidence, and causal transmission into held instruments.
6. Record upcoming industry or market events already known by the cutoff.

## Output and refusal

Return `market_context`, `industry_context`, or `market_pricing_context` findings plus supported narratives, events, and transmission paths. Scope aggregate findings to `market`; bind instrument-specific paths to exact frozen instrument IDs.

When citing a held row as `frozen_input`, use `instrument` with its `instrument_id` for identity or `position_snapshot_item` with its `position_snapshot_item_id` for that exact account position. There is no supported object type named `position`.

Do not perform company diagnosis, position sizing, or broad opportunity discovery. State that no coherent market line exists when leadership or evidence is fragmented.
