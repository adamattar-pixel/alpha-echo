---
name: compose-portfolio-briefing
description: Compose an immutable Alpha Echo stock-portfolio report from validated macro context, market context, position diagnoses, and portfolio-construction findings without adding new research.
---

# Compose portfolio briefing

Use only validated findings and linked events, narratives, and transmission paths from the supplied briefing. Never browse or add a fact during composition.

Write all human-readable briefing content in the frozen report language. Preserve tickers, official names, source identifiers, and Finding IDs exactly.

Treat every input as one unified report. Do not create morning, evening, manual, or revision-specific wording.

Use `frozen_portfolio.report_language`: `en` means English and `zh` means Simplified Chinese. Legacy inputs without this field use Chinese. Apply it to all generated human-readable text, including headings, summaries, evidence titles, expectations, conditions, and drafts. Preserve original identifiers, URLs, quotations and user-authored facts.

## Workflow

1. Lead with material changes to held positions and their Trade Intents.
   If `analysis.intent_assessment=missing`, label the holding as awaiting user confirmation and summarize its `analysis.intent_draft` as a model-proposed entry rationale in the report language. Keep it visibly separate from verified facts and from a user-confirmed Intent; do not describe the draft as strengthened, weakened, invalidated, or unchanged.
2. Preserve the macro → industry/market → company → market-pricing causal chain.
3. Distinguish current facts, future expectations, expectation gaps, upside catalysts, downside pressures, and core risks.
4. Present portfolio-construction issues after individual diagnoses; do not mix company quality with position sizing.
5. Include scheduled events, observation conditions, invalidation conditions, and conditional actions.
6. State what does not require action and expose missing or partial research.

## Required position coverage

Create exactly one item with `section="position_diagnosis"` for every validated finding whose `finding_type` is `equity_position_diagnosis`. Each such item must cite exactly that one position-diagnosis Finding ID; it may additionally cite macro or market Findings. Never merge several held stocks into one diagnosis item, even when the conclusion is similar.

For a missing Intent set `intent_status="draft_unconfirmed"` and `actionability="non_actionable"`; explicitly state `awaiting user confirmation` for English or `待用户确认` for Chinese and never recommend buying, selling, adding, or reducing. For a matched Intent set `intent_status="active"` and `actionability="conditional"`. Every non-position item sets both fields to null.

Use the company name and ticker in the item title. Make the body readable as a compact research note in the report language and include all of these labeled parts:

- `诊断结论` — what changed, or explicitly that no material change was verified.
- `相关新闻` — split supplied evidence into `本轮新增` (published on or after the Finding's `window_start`) and `近期背景` (older evidence used for the latest results, guidance, expectations, or material-event baseline). Include titles, publishers, publication dates, and links. When no incremental item exists but recent baseline evidence does, lead with that evidence under `最近动态` and add only a compact freshness note; do not make an empty-news message the dominant content. Never invent news.
- `未来预期` and `预期差` — distinguish market expectations from your evidence-based assessment.
- `宏观 / 行业 / 公司 / 市场定价` — preserve the four-level causal chain.
- `潜在利好 / 潜在利空 / 核心风险` — list concrete catalysts, pressures, and risks from the diagnosis.
- `持仓逻辑与条件` — show the Intent assessment, confirmation and invalidation conditions, and conditional action. When the Intent is missing, show the proposed `intent_draft`, label it `Model draft, awaiting user confirmation` for English or `模型草稿，待用户确认` for Chinese, and remain non-directional.

Use short Markdown headings or bullet lists inside `body`; do not emit HTML. The application will render this Markdown as formatted content.

Every briefing item must cite validated finding IDs. Exclude Crypto and Meme sections. It is correct to report no material change and no action.

For English reports translate the section labels above: Diagnosis; Related news; New this run; Recent background; Latest developments; Future expectations; Expectation gap; Macro / Industry / Company / Market pricing; Upside catalysts / Downside pressures / Core risks; Holding rationale and conditions.
