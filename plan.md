# Feature plan

Last updated: 2026-10-03 (Asia/Hong_Kong)

This is the living inventory and plan for Alpha Echo. The initial baseline is
based on the repository at `80ac310`, including `README.md`, `CONTEXT.md`, API
modules, and the workbench UI. “Implemented” describes code presence, not a
guarantee that external data sources or every workflow currently succeed.
Completed feature changes are recorded in [audit.md](audit.md).

## Current features

| ID | Feature | Status | Scope |
| --- | --- | --- | --- |
| F01 | Trade-driven stock holdings | Implemented | Record buys/sells, aggregate holdings by account, calculate average cost and realized P&L, and auditably void the latest manual trade. RMB, HKD, and USD support. |
| F02 | Reference market prices | Implemented | Automatic market-data adapters with observation times and explicit missing/stale price states. |
| F03 | Holding logic and intent drafts | Implemented | Versioned trade intent; evidence-based model drafts in the report language require user confirmation before becoming active. |
| F04 | Equity research and reports | Implemented | Macro, A-share/HK/US market context, per-holding diagnosis, news and expectations, portfolio analysis, and English/Chinese report synthesis. Independent research can run in parallel; interrupted tasks resume from saved progress. |
| F05 | Decision desk and market intelligence | Implemented | Read saved reports by date; visually pin macro/cross-market assessment first, then portfolio advice and account-grouped stock analysis cards; flag the latest report as stale when appropriate. All tabs share the editorial white/silver visual system and its dark graphite counterpart. The Settings screen does not expose internal Project Skills or their status. |
| F06 | Report generation controls | Implemented | Manual equity reports and one configurable daily equity schedule with pause/resume controls. Meme scheduling removed. |
| F07 | Meme candidate tracking | Removed | Arena, GMGN runner, own-wallet and smart-money management, carry-forward nomination and report workers removed on 2026-10-03. |
| F08 | Meme survival and narrative research | Removed | Dedicated research skills, output schema, lifecycle analysis and dedicated report/observation storage removed. |
| F09 | Meme exploration and exclusions | Removed | Navigation, chain switching, bubble map, token drawer, historical reports and blacklist UI/API/storage removed. |
| F10 | Smart-money address import | Removed | Single-address and batch import UI/APIs removed with Meme tracking. |
| F11 | Feature change documentation | Implemented | Maintain this inventory and the chronological audit whenever app features are added, removed, or changed. Root `AGENTS.md` defines the workflow. |
| F12 | English / Chinese interface | Implemented | Top-of-page `EN \| 中文` buttons switch interface labels across all tabs, persist the preference, update document language/title and number/date formatting, and preserve entered text and saved report content. New research/report content uses its frozen generation language. |
| F13 | Fresh-start database reset | Completed | One-time, user-authorized reset of this local Alpha Echo database after a verified backup. All business data cleared; migration history retained; automatic equity reports paused. Not a new destructive UI/API control. |

## Upcoming work

### Repository sharing preparation — F11 documentation / configuration hygiene

- **Status:** prepared for a fresh initial commit; remote upload deferred until
  the user provides a destination. Current app functionality is retained.
- **Scope:** privately back up source and old history outside Git; remove the
  local environment file from the shareable directory; redact personal backup
  locations and real holding identifiers in development notes; exclude private
  keys, logs, database dumps and exports; require a user-supplied Compose password.
- **Acceptance:** the shared Git repository has one root commit, no inherited
  refs/reflogs/remotes, a neutral initial author, and no tracked private data.
- **Validation:** 19 frontend tests, typecheck/lint/build, 66 backend tests and
  Rust formatting pass (one database-dependent replay intentionally ignored).
- **Limitations:** secret scanning is not proof of absence. Revocation/rotation
  is still required if any credentials were previously disclosed. Local database
  contents remain outside the repository and are not cleared or exported.

### F04 — Preserve valid diagnoses with general industry evidence

- **Status:** implemented.
- **Scope:** unbound industry evidence may describe general sector conditions.
  Only company/product evidence or explicitly instrument-bound industry evidence
  requires a valid instrument binding; unbound industry evidence cannot declare
  an instrument relationship or satisfy company-news coverage.
- **Acceptance criteria:** the captured failed diagnosis validates without changing
  its facts; company coverage, binding, transmission and cutoff checks remain strict;
  completed report history is not rewritten.
- **Validation:** the regression failed with `related_instrument_id is required`
  before the fix, then passed. All 66 regular backend tests passed, with one
  opt-in audit replay excluded from the default suite; that read-only replay was
  run separately against the captured failed diagnosis and passed.
- **Dependencies:** none added. Backend restart resumes the current report's
  unfinished individual diagnosis while reusing validated upstream tasks.

### F03 / F04 / F06 / F12 — English analysis and report content

- **Status:** implemented; full live English report completion pending.
- **Scope:** manual generation freezes the current interface language; API callers
  may specify `en`/`zh` (English default). Daily reports have their own language
  setting, defaulting to English. All research stages, model-proposed intent
  drafts, synthesis and deterministic report notes use the frozen language.
- **Acceptance criteria:** English prose is accepted for English reports without
  bypassing evidence or Intent safeguards; Chinese generation still works; resumed
  tasks retain their initial language; historical content is not silently rewritten.
- **Validation:** 66 backend tests and 15 frontend tests passed, with production
  build, typecheck and lint. Live API rejects unsupported languages and confirms a
  new English report freezes `report_language=en`; old Chinese report unchanged.
- **Dependencies:** configured Codex research runner and existing source adapters.

### F04 / F06 — Research executor launch recovery

- **Status:** implemented; validated research recovered, full synthesis pending.
- **Scope:** resolve the relocated macOS bundled Codex CLI, retain explicit custom
  executable paths, check executability/project directory before report creation,
  and surface backend creation errors instead of generic connectivity advice.
- **Acceptance criteria:** a new report can start real research and invoke synthesis
  after validated findings; unavailable executors fail early with an actionable
  configuration error; failed historical reports and current holdings remain intact.
- **Dependencies:** installed/authenticated Codex CLI, configured model and network.
- **Validation:** path-relocation regression reproduced the failure before the fix;
  all 62 backend tests and frontend build/15 tests/typecheck/lint pass. The new
  report's macro task completed with valid findings; market research is running.
  Full report synthesis has not completed at handoff.

### F12 — English / Chinese interface

- **Status:** implemented.
- **Acceptance criteria:** both buttons remain available on every tab; switching
  immediately updates navigation, controls, status labels, forms, placeholders,
  and accessibility labels; the active button is indicated; the preference survives
  reloads; switching preserves the current tab, form drafts, and stored content.
- **Dependencies:** browser local storage (optional; in-session switching still
  works if storage is blocked). No backend or report-generation changes required.
- **Validation:** frontend production build and all 18 tests passed; typecheck and
  lint passed. Browser checks confirmed both language directions, translated market
  filters/settings, persisted English after reload, and preserved an unsaved form
  draft while switching. A 375px browser preview confirmed the controls fit on
  mobile. Known domain enums are localized for display; persisted business values
  remain canonical. See the 2026-10-03 F12 entry in [audit.md](audit.md).

### F05 / F12 — Editorial visual system and day/night preference

- **Status:** implemented.
- **Scope:** apply the `design.md` white-first editorial design language across
  navigation, cards, reports, holdings, forms, setup and status surfaces. The
  day/night control is a visual preference only: day uses warm white, black and
  silver; night uses graphite, charcoal and silver-white without saturated blue.
- **Acceptance criteria:** the same task and data flows remain available in both
  modes; the control has an accessible label and state, persists after reload and
  does not alter the selected language, reports, holdings, trade drafts or API
  requests. The layout remains usable at desktop and 390px widths.
- **Dependencies:** optional browser local storage. No backend, database, report
  generation or data-model change is required.
- **Validation:** frontend typecheck, lint, production build and all 19 existing
  tests pass. Browser checks covered light and dark holdings, dark decision desk,
  persistence through reload and a 390px dark layout; the browser was restored to
  the default light preference afterward. See the 2026-10-03 visual-system entry
  in [audit.md](audit.md).

### F05 — Remove Project Skills from Settings

- **Status:** removed from the user interface.
- **Scope:** remove the Project Skills settings panel, its status list, associated
  client state, translation entries, CSS and `/api/skills` browser request. Internal
  research methods remain part of report execution and are not a user setting.
- **Acceptance criteria:** Settings has no Project Skills section or empty space
  reserved for it; opening or refreshing the application makes no client request
  to `/api/skills`; research reports retain their existing execution contract.
- **Dependencies:** none. This does not remove server-side research validation or
  stored report provenance.
- **Validation:** frontend typecheck, lint, production build and all 19 frontend
  tests pass. See the 2026-10-03 removal entry in [audit.md](audit.md).

For each feature, record:

- **ID and name:** reuse its ID for later changes or removal.
- **Status:** planned, in progress, implemented, deferred, or removed.
- **User outcome and scope:** what changes for the user and what is excluded.
- **Acceptance criteria:** observable behavior required for completion.
- **Dependencies:** data sources, configuration, migrations, or prerequisite work.
- **Validation:** checks performed and remaining limitations; link the audit entry.

## Product boundaries

- Current workflows cover equity holdings and reports only. Meme research and GMGN integration have been retired.
- Historical reports and trade facts remain immutable; new facts create new records.
- Stock analysis covers macro, industry, company, pricing, expectations, catalysts,
  downside pressures, and core risks; portfolio analysis considers sizing and
  shared exposures.
- Historical migrations and shared legacy wallet columns/market enums remain for
  schema compatibility; they do not expose a Meme feature or require GMGN.
- Automatic trading, fee accounting, and broader ordinary-crypto research are
  outside the current feature scope.

## 2026-10-03 — Focused overview layout (F05 / F12)

- **Status:** implemented. Both opening sections retain the Total label, ordered
  portfolio conclusions/guidance/alerts, then macro/cross-market assessment.
- **Acceptance:** independent cards grouped by account show Diagnosis, Current
  company status, Company and product news, Future expectations, Upside catalysts,
  Downside pressures and Core risks. Other analytical dimensions and position
  quantity/cost/P&L metadata are omitted from these cards, not deleted from data.
- **News:** company/product evidence must match the exact instrument; dated items
  sort newest first and deduplicate by URL. Show source, date, title, excerpt and
  original link; disclose items beyond the first five on demand.
- **Readability:** 16px analysis text, 18px diagnosis and larger section headings;
  wide-screen outlook/news columns and catalyst/pressure/risk columns become
  single-column on narrower screens. English and Chinese labels remain supported.
- **Dependencies:** existing immutable report snapshots and validated diagnosis
  fields; no new research, data migration or report regeneration required.
- **Validation:** typecheck, lint, production build and all 19 frontend tests pass;
  actual English report verified in browser at 1280px and 390px widths without
  horizontal overflow.

### F05 — Macro-first overview and Advice label

- **Status:** implemented.
- **Scope:** keep the macro/cross-market assessment visually pinned immediately
  below the report selector, ahead of the portfolio conclusion card. Rename only
  the portfolio card's eyebrow label to Advice (`建议` in Chinese); report content
  and titles are unchanged.
- **Acceptance criteria:** macro appears before portfolio advice at every viewport;
  holdings continue to follow both opening panels; English and Chinese labels are
  localized; the report's facts, order within each panel and data contract stay
  unchanged.
- **Dependencies:** existing report findings only; no new API, research or database
  behavior.
- **Validation:** frontend typecheck, lint, production build and all frontend
  tests pass; browser verification confirms the macro panel is the first opening
  card and the portfolio eyebrow reads Advice. See the 2026-10-03 entry in
  [audit.md](audit.md).

## 2026-10-03 — Meme retirement and fresh start

- **Features:** F06 changed; F07–F10 removed; F13 completed.
- **Acceptance criteria:** no Meme navigation, wallet/smart-money settings,
  bubble map, report/blacklist endpoints, workers, schedules or GMGN dependency;
  stock workflows and language switching still function with an empty database.
- **Dependencies:** additive migration `0026_remove_meme_arena.sql` removes
  dedicated Meme tables/settings without editing applied migrations. Backup is
  stored outside the repository, not committed with user data.
- **Reset scope:** only this application's local `alpha_echo` database.
  Keep migration history and a fresh disabled equity schedule; never truncate
  business data automatically on startup.
- **Validation:** frontend production build and 15 tests passed; backend 57 tests
  and formatting passed. Runtime and final review results are recorded in audit.md.
