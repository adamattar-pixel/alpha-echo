---
name: assess-portfolio-construction
description: Assess a frozen stock portfolio after all position diagnoses are validated, focusing on sizing, concentration, duplicated exposures, shared catalysts and risks, currencies, liquidity, and risk-budget alignment rather than repeating company research.
---

# Assess portfolio construction

Apply `apply-investment-research-standards`. Use only frozen portfolio facts and validated upstream stock findings. Read [risk taxonomy](references/risk-taxonomy.md).

## Workflow

1. Verify that every non-zero equity position has a validated diagnosis or an explicit data gap.
   A diagnosis with `analysis.intent_assessment=missing` may inform exposure mapping, but it cannot support thesis-strength, conviction, risk-budget alignment, or trade-action claims.
2. Compare position weights with diagnosis strength, risk budget, time horizon, liquidity, and loss severity. Do not invent a target weight.
3. Identify industry, theme, macro-factor, geography, currency, catalyst, and liquidity concentrations.
4. Detect different tickers or markets that express the same economic exposure.
5. Identify several positions that can fail together under one scenario or event calendar.
6. Distinguish measured concentration from inferred relationships; never invent correlations, betas, or hedges.
7. Rank only material portfolio issues by severity and immediacy.

## Output

Return `portfolio_construction` findings scoped to `portfolio`. Explain which validated position findings support each issue. Conditional actions may propose review, reduction, addition, sequencing, or cash preservation, but must include confirmation and invalidation conditions and must not execute trades.

Return no new portfolio issue when exposures are proportionate and evidence does not support one. Do not repeat company-level summaries.
