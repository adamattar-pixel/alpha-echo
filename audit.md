# Feature audit

Chronological development record for app feature additions, removals, and
user-visible changes. Dates use Asia/Hong_Kong. The current inventory and upcoming
work live in [plan.md](plan.md). This document does not replace database audit
records, immutable reports, or source-control history.

Append new entries at the end. Each entry should include the date, change type
(added, removed, changed, or corrected), feature IDs, user-visible effect, reason,
affected files, validation performed, and any known limitations. Include a commit
reference when available; a commit reference is not required before committing.

## 2026-10-03 — Existing feature baseline

- **Type:** baseline; not a claim that these features were added on this date.
- **Features:** F01–F10 in `plan.md`.
- **Reference:** `80ac310` (`fix: restore meme narrative generation`), the starting
  point for branch `codex/new-features`.
- **Scope:** trade-driven stock holdings, reference prices, confirmed holding
  logic, equity research and saved report views, generation controls, and the
  separate Meme wallet/arena/narrative/exploration workflows.
- **Evidence:** reviewed `README.md`, `CONTEXT.md`, `backend/src/http.rs`, and
  relevant parts of `app/intelligence-desk.tsx` against the repository file inventory.
- **Validation:** baseline documentation review only; no full workflow tests or
  external-provider checks performed for this entry. Historical development dates
  and validation results have not been reconstructed.

## 2026-10-03 — Add ongoing feature documentation

- **Type:** added.
- **Feature:** F11 — Feature change documentation.
- **Effect:** the repository now contains a living feature plan and an append-only
  feature audit. Future app feature additions, removals, and behavior changes must
  update both alongside the implementation.
- **Reason:** requested tracking for each feature added to or removed from the app.
- **Files:** `plan.md`, `audit.md`, and root `AGENTS.md`.
- **Validation:** checked the patch for whitespace errors and confirmed both
  documents use matching feature IDs and links. No app runtime code changed.
- **Limitation:** this is a repository development rule, not an automated CI gate.

## 2026-10-03 — Add English / Chinese interface switching

- **Type:** added.
- **Feature:** F12 — English / Chinese interface.
- **Effect:** every tab has top-of-page `EN | 中文` buttons with an active-language
  indicator. Switching immediately translates interface navigation, forms,
  placeholders, controls, status labels, and accessibility text. The preference is
  saved locally, dates/numbers use the selected locale, and document language/title
  follow the selection. Current tabs and unsaved drafts survive a switch.
- **Reason:** requested functional English and Chinese language buttons.
- **Files:** `app/intelligence-desk.tsx`, `app/globals.css`, `app/i18n.mjs`,
  `app/i18n.d.mts`, `app/locales/en.json`, `tests/i18n.test.mjs`,
  `tests/rendered-html.test.mjs`, and `plan.md`.
- **Validation:** `npm run typecheck`, `npm run lint`, and `npm test` passed
  (production build and 18 tests). Tests cover translated/interpolated text,
  language notifications, saved preferences, blocked/corrupt storage, feedback
  switching, domain enum labels, and literal translation coverage. Browser verification confirmed
  English and Chinese labels, market filters, settings, retained draft input,
  and persisted English after reload. A 375px responsive preview confirmed the
  header controls fit. Final review corrections keep blacklist/void audit reasons
  and generated intent titles canonical, and distinguish raw backend errors from
  localizable application feedback.
- **Limitations:** saved research/report text and user-entered names and theses
  retain their original language. External provider/backend error text is shown as
  received. No report regeneration or automatic content translation is introduced.

## 2026-10-03 — Retire Meme arena and clear local business data

- **Type:** removed / changed; one-time database reset.
- **Features:** F06 changed; F07–F10 removed; F13 completed.
- **Effect:** removes Meme navigation, chain views, bubble map/detail drawer,
  blacklist, own-wallet and smart-money management/import, wallet snapshot
  adapter, GMGN dependency/configuration, dedicated research skills/schema,
  report workers/scheduler and related HTTP routes. Stock reports, holdings and
  both interface languages remain available. Stock asset setup now offers only
  A-shares, Hong Kong and US stocks.
- **Reason:** user requested clearing all app data and removing the Meme arena.
- **Database operation:** stopped the backend, verified the target was only local
  `alpha_echo`, exported a PostgreSQL 17 custom-format backup outside Git and
  verified its archive listing before the transactional truncate. Cleared all
  business tables, retained `_sqlx_migrations`, and created only a fresh disabled
  equity schedule. A private backup is stored outside the repository.
  Old data can be restored from that archive;
  this is not a reusable reset endpoint or automatic startup deletion.
- **Files:** frontend workbench/styles/locales/tests, backend config/router/main,
  portfolio/research/settings/briefing modules, additive migration
  `0026_remove_meme_arena.sql`, deleted Meme modules/layout/skills/schema,
  package manifests, `.env.example`, `README.md`, `CONTEXT.md`, architecture and
  research-output-contract documentation, and `plan.md`.
- **Validation:** frontend production build and 15 tests passed; backend 57 tests
  and Rust formatting passed. Live checks confirmed stock list APIs return
  empty arrays, stock intelligence is empty, automatic reports are paused, and
  all retired Meme/wallet/settings/adapter routes return 404 for GET/POST/PUT/DELETE.
  Every remaining business table was checked for zero rows; migration 0026
  removed all three dedicated Meme tables. Browser checks covered all five tabs,
  empty holdings/history/intelligence, Chinese labels and paused stock settings.
  Review corrections reject new Meme instruments and exclude legacy wallet/smart
  money data from new equity report inputs, while preserving historical decoding.
  Generic wallet snapshot/Intent writes are blocked, current stock queries hide
  legacy wallet holdings, and new research outputs reject retired markets.
  Cross-market history keeps the canonical `cross` market value.
  Live rejection checks returned 400 for Meme instrument creation, wallet
  snapshots and wallet-scoped Intent creation without creating data. All five
  report snapshot SQL queries passed PostgreSQL EXPLAIN; migration history
  contains 26 successful versions. Final stock UI list APIs remained empty.
- **Compatibility:** shared historical wallet tables/columns and market enum values
  remain for migration compatibility. Dedicated Meme tables and settings are
  dropped via a new migration; no previously applied migration was edited.
  Local environment credentials were not deleted or disclosed and are no longer
  used by the app. Other PostgreSQL databases were not changed.

## 2026-10-03 — Recover equity report research executor launch

- **Type:** fixed / changed.
- **Features:** F04, F06.
- **Cause:** the configured macOS bundled Codex executable path no longer existed
  after the application bundle layout changed. All four research tasks failed at
  process spawn with ENOENT, so no validated upstream findings were available for
  synthesis. The retained partial report correctly did not invent research.
- **Effect:** resolve PATH-based CLI names and the recognized relocated bundle
  path, preserve explicit custom executables, and check the executable and project
  directory before manual report creation and each CLI invocation. Settings no
  longer label a missing executable as configured. The UI surfaces actual backend
  creation errors. Portable example configuration and setup guidance are updated.
- **Files:** backend config, research and briefing modules; frontend workbench;
  `.env.example`, `README.md`, and `plan.md`. The local ignored environment path
  was corrected without changing model, reasoning effort, or credentials.
- **Validation:** the path-migration regression failed before the fix and passed
  afterward. All 62 backend tests passed, including an unavailable-executor API
  test that performs no database/news work. Frontend typecheck, lint, production
  build and 15 tests passed. Backend restarted; a new manually triggered report
  entered real research without the previous launch failure. Macro research
  completed with validation status `valid`; report progress reached 1 / 4 and
  equity market research started. The original “no validated Research Run”
  assertion now passes. Full report synthesis has not completed at handoff.
- **Data safety:** existing holdings, intent and immutable failed report retained;
  no database reset, history rewrite or validation bypass.

## 2026-10-03 — Generate analysis and report content in English or Chinese

- **Type:** changed.
- **Features:** F03, F04, F06, F12.
- **Effect:** manual reports freeze the interface language at creation; API callers
  can select `en`/`zh`, with English as the API default. Daily schedules expose an
  independent report-language setting, defaulting to English. Macro, market,
  individual diagnosis, proposed Intent and portfolio research, synthesis,
  deterministic diagnosis headings/fallbacks and data-gap sections follow that
  language. English findings are no longer rejected merely for lacking Chinese.
- **Reason:** interface translation alone left the actual analysis in Chinese.
- **Skill changes:** replace unconditional Chinese prose requirements with the
  frozen language while retaining source provenance, cutoff checks, per-position
  coverage and the unconfirmed-draft/non-actionable boundary. IDs, links and
  user-authored facts are preserved rather than translated in place.
- **Files:** new backend report-language module; briefing, research and settings
  modules; workbench and English locale; research Skills/output contract;
  README, CONTEXT, architecture and plan.
- **Validation:** 66 backend tests passed, including English research acceptance,
  rejection of invented citations, English deterministic position notes and
  unconfirmed-draft safety, rendered English gaps, legacy Chinese defaults and
  request/schedule language selection. Frontend typecheck, lint, production build
  and 15 tests passed. Backend restarted; live API rejected `fr` with 422 and
  created a new English report with an immutable `report_language=en` input.
  Existing Chinese report remained unchanged. Full live English synthesis is not
  yet complete. Skill quick-validation was attempted but the system Python lacks
  PyYAML; frontmatter/names/references were unchanged and manually checked.
- **History:** the earlier executor-recovery smoke report later finished partial:
  macro/market research and synthesis validated, but individual diagnosis lacked
  a required instrument binding and portfolio prose failed the prior Chinese
  language check. The language change addresses the latter class; it does not
  bypass or claim to resolve missing evidence bindings.
- **Limitations:** changing UI language does not translate old saved reports or
  alter a running report. Generate a new report to obtain another language.

## 2026-10-03 — Fix rejection of unbound industry evidence in individual diagnoses

- **Type:** fixed.
- **Feature:** F04.
- **Cause:** the validator treated all industry evidence like instrument-bound
  company evidence. The captured failed diagnosis had four correctly bound company
  sources plus a general semiconductor-sales source with a null instrument ID.
  Rejecting that valid sector source discarded the entire individual diagnosis.
  This corrects the earlier description of the failure as a missing required
  company binding: the field existed and its null value was appropriate for the
  general industry evidence.
- **Effect:** permit unbound sector evidence without relationship assertions;
  retain exact bindings for company/product and stock-specific industry evidence.
  General industry data still cannot satisfy company coverage. Bound industry
  transmission paths, publication/observation cutoff and provenance checks remain.
  Binding errors now identify the affected evidence key.
- **Files:** `backend/src/research.rs`, `plan.md`, and `audit.md`.
- **Validation:** regression reproduced the exact rejection before the fix and
  passed afterward. All 66 regular backend tests passed; an opt-in read-only audit
  replay was separately run against the actual saved failed diagnosis and passed.
  Negative assertions cover company/product without binding, unsupported binding,
  relationship claims without an instrument, post-cutoff sector evidence and
  attempts to substitute sector evidence for company coverage.
- **Runtime/data safety:** restarted the backend to apply the fix; the current
  English report had two validated upstream tasks, which checkpoint recovery
  reuses while resuming unfinished diagnosis. Existing terminal reports, holdings
  and Intent versions were not changed. Full resumed report completion is pending.

## 2026-10-03 — Focused Today's Overview layout

- **Features:** F05 / F12, user-visible layout change. Put portfolio conclusions,
  conditional guidance and alerts first; macro/cross-market assessment second;
  account-grouped stock analysis last. Retain Total labels on the opening sections.
- **Cards:** replace dense position metadata and secondary analytical dimensions
  with the seven requested analysis sections. Increase body and diagnosis font
  sizes to 16px and 18px. Keep missing-diagnosis/Intent warnings explicit.
- **News feed:** newest-first dated company/product evidence bound to the exact
  stock, URL deduplication, source/date/original links, compact excerpts, and a
  disclosure for additional items beyond five. Narrow layouts stack columns.
- **Files:** `app/intelligence-desk.tsx`, `app/globals.css`, `app/locales/en.json`,
  `tests/overview.test.mjs`, `tests/rendered-html.test.mjs`, `plan.md`, `audit.md`.
- **Validation:** typecheck, lint, production build and all 19 frontend tests pass.
  New React-rendered regressions cover section order, cross-market inclusion,
  requested card scope, exact stock news binding, ordering/deduplication, source
  links/dates, missing diagnoses and Chinese labels. Browser verification used
  the saved completed English report at 1280px and 390px; no horizontal overflow,
  and narrow cards use single-column layouts. Screenshot saved outside the repo.
- **Data safety:** no holdings, trades, research facts or historical reports were
  changed or regenerated. Existing research fields remain available elsewhere.

## 2026-10-03 — Prepare a sanitized repository for sharing

- **Type:** user-authorized repository history reset and privacy cleanup; no app
  feature removal. F11 documentation is retained with an explicit privacy exception.
- **Preservation:** private archive of the working tree (including uncommitted
  changes and local configuration) and original Git history is stored outside
  this repository with restricted permissions. Generated dependencies/build caches
  are excluded from the archive. The live database is untouched.
- **Cleanup:** relocate `.env` outside the shareable directory; expand exclusions
  for keys, logs, backups, exports and local agent settings. Remove the embedded
  development password from optional Compose configuration. Redact personal
  backup locations and the real holding identifier in earlier development notes;
  the unredacted notes remain in the private backup, not in shared Git history.
  Prior factual audit entries are otherwise retained, not rewritten to hide bugs.
- **History:** create a new Git repository and neutral-author initial commit,
  without old commits, tags, branches, reflogs, remotes or hooks. Old commit IDs
  mentioned in development notes are references only and are not imported.
- **Inspection:** scanned 690 historical Git objects plus current shareable files
  for key/token signatures, credential URLs, personal paths and matches against
  local configured secrets. Flagged historical credential URLs were in developer
  configuration/test fixtures; this is not an exhaustive security certification.
- **Files:** `.gitignore`, `compose.yaml`, `README.md`, `plan.md`, `audit.md`,
  `backend/src/briefings.rs` (remove unused test URL credentials), and Git metadata.
  All current source changes are retained in the new baseline.
- **Validation:** frontend typecheck/lint/build and all 19 tests passed; 66 backend
  tests and Rust formatting passed, with one opt-in database replay ignored.
- **Handoff:** upload is deferred until a repository URL is supplied. New local
  startup requires creating/configuring `.env` from `.env.example`. If a secret
  has ever been exposed, deletion of history is not a substitute for rotation.
- **Migration integrity:** two original SQL migration files have trailing blank
  lines; these are deliberately preserved to avoid changing SQLx checksums.

## 2026-10-03 — Editorial visual system with day/night theme

- **Features:** F05 / F12 visual-only update. `design.md` is the source of the
  design direction: editorial white space, rounded surfaces, black/grey hierarchy
  and restrained silver accents.
- **Implementation:** replace the persistent dark sidebar visual treatment with
  a floating navigation bar and unify cards, inputs, reports, holdings and setup
  surfaces under shared light/dark tokens. Add an accessible day/night control
  whose choice is saved only in browser local storage.
- **Dark mode:** graphite canvas, charcoal surfaces, silver borders and off-white
  text; no saturated blue palette. This is intentionally a presentation setting,
  not an automatic time-based mode, so it never changes unexpectedly while a user
  is recording a trade or reviewing a report.
- **Files:** `design.md`, `app/globals.css`, `app/intelligence-desk.tsx`,
  `app/locales/en.json`, `plan.md`, and `audit.md`.
- **Data and workflow safety:** no API route, backend code, database value,
  research job, trade input, report content or existing interaction flow changed.
- **Validation:** frontend typecheck, lint, Vite production build and all 19
  frontend tests passed. Browser checks confirmed the holdings screen in both
  modes, decision desk in dark mode, local preference persistence after reload,
  and a 390px dark viewport without horizontal overflow. The browser preference
  and viewport were restored to light/default after verification.

## 2026-10-03 — Remove Project Skills from Settings

- **Feature:** F05 user-visible settings simplification.
- **Change:** remove the Project Skills card shown in Settings, including its
  status list, styling, localized labels, client state and `/api/skills` request.
  The Settings screen now ends after the source-snapshot adapter section.
- **Boundary:** this intentionally removes only the exposed settings feature.
  Internal versioned research methods and report provenance remain in the research
  pipeline so reports continue to run with their existing validation safeguards.
- **Files:** `app/intelligence-desk.tsx`, `app/globals.css`,
  `app/locales/en.json`, `plan.md`, and `audit.md`.
- **Validation:** searched all client code and tests for the removed Project Skills
  UI/state/request identifiers; frontend typecheck, lint, Vite production build
  and all 19 frontend tests passed.

## 2026-10-03 — Pin macro context first and rename portfolio eyebrow

- **Feature:** F05 overview hierarchy refinement.
- **Change:** visually pin the macro/cross-market assessment directly after the
  report selector, before the portfolio conclusion card and account diagnostics.
  Rename the portfolio card eyebrow from Total (`总览`) to Advice (`建议`).
- **Boundary:** this is display hierarchy and localized interface copy only. Report
  finding data, research order, trade controls, API requests and persisted content
  are unchanged.
- **Files:** `app/intelligence-desk.tsx`, `app/globals.css`,
  `app/locales/en.json`, `tests/overview.test.mjs`, `plan.md`, and `audit.md`.
- **Validation:** frontend typecheck, lint, Vite production build and all frontend
  tests passed; browser verification confirms macro is visually first and Advice
  appears on the portfolio conclusion card.
