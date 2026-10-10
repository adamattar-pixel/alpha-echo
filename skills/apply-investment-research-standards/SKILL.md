---
name: apply-investment-research-standards
description: Apply the mandatory evidence, cutoff-time, provenance, uncertainty, and Trade Intent-relative judgment rules to every Alpha Echo market research run or finding before it can enter a briefing.
---

# Apply investment research standards

Treat the supplied `as_of`, research window, position snapshot, Trade Intent versions, and evidence records as immutable inputs.

Use `frozen_portfolio.report_language`: `en` means English and `zh` means Simplified Chinese. Legacy inputs without this field use Chinese. Apply it to all generated human-readable text, including headings, summaries, evidence titles, expectations, conditions, and drafts. Preserve original identifiers, URLs, quotations and user-authored facts.

## Workflow

1. Reject sources published or observed after `as_of`. Never use later price action to justify an earlier conclusion.
2. Group repeated reporting into events. Preserve the strongest primary source and link corroborating sources without counting them as independent events.
3. Label every statement as `fact`, `inference`, `view`, or `scenario`. Do not blend labels inside one claim.
4. Bind each material fact to one or more supplied evidence IDs. Prefer filings, issuer releases, regulators, exchanges, official statistics, and protocol data.
5. Record meaningful conflicting evidence. Lower confidence when source quality, timeliness, coverage, or causal linkage is weak.
6. Evaluate impact against the applicable Trade Intent version: `strengthened`, `weakened`, `invalidated`, or `unchanged`.
7. Include confirmation conditions, invalidation conditions, conditional action, time horizon, confidence, and data gaps.
8. Return no opportunity when evidence does not support one.

## Refusal rules

- Do not invent evidence IDs, prices, flows, holdings, confirmed Trade Intents, or causal links. When an equity Intent is missing, you may propose an evidence-based `intent_draft` in the report language, but must label it as unconfirmed and keep it non-actionable until the user accepts it.
- Do not call inferred activity precise net inflow without complete flow data.
- Do not turn good company performance into positive surprise without comparing it with priced expectations.
- Do not give unconditional buy or sell instructions.
- Do not modify portfolio facts or mark a run successful.

## Output check

Before returning, read [references/output-contract.md](references/output-contract.md) and verify every finding. Return validation errors separately from findings.
