---
name: analyze-macro-context
description: Build the shared, cutoff-safe macro context for an Alpha Echo equity report before market context and position diagnosis run; exclude Crypto and Meme analysis.
---

# Analyze macro context

Apply `apply-investment-research-standards`. Treat the cutoff, research window, frozen positions, intents, and captured sources as immutable.

## Workflow

1. Identify only macro changes that can affect the frozen A-share, HK, or US equity holdings.
2. Separate confirmed facts, current conditions, forward expectations, priced expectations, and scenarios.
3. Cover growth, inflation, policy rates, liquidity, currencies, fiscal or industrial policy, commodities, and geopolitics only when a causal link to held equities is defensible.
4. Describe the base path, upside deviation, downside deviation, and observable conditions that would change the path.
5. Create transmission paths into relevant equity markets or held instruments. Do not include Crypto or Meme targets.
6. Record scheduled macro events already known by the cutoff.

## Output

Return the public research schema. Use `macro_context` findings scoped to `macro`; link every event and transmission path through a finding. Put assumptions and uncertainty in inference rather than presenting forecasts as facts.

Return no material-change finding when nothing relevant changed. Do not recommend rates, FX, commodities, or individual trades.
