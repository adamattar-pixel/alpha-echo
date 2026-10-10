# Alpha Echo V1 architecture

Alpha Echo is a local modular monolith with three business modules.

## Portfolio

Owns deterministic facts: stock accounts, instruments, trades, immutable position snapshots, Trade Intents, and immutable Trade Intent versions. Research code has no write path into these tables. Shared legacy wallet columns and market enums remain readable for schema compatibility, but there is no wallet-management or Meme integration API.

The Web may submit a complete stock CSV snapshot or a single-instrument update. The latter creates another immutable snapshot by copying the prior complete state and replacing only the submitted instrument. Position changes are derived by comparing each owner’s two latest snapshots; they are not mutable facts of their own.

Trade Intent metadata and its account/wallet/instrument/position-slice scope are copied into every immutable version. The current link projection is replaced only after a new version is inserted, so historical briefings retain the prior scope.

Daily entry records buys and sells under stock accounts. New Meme instrument creation is rejected. Public-wallet settings, smart-money imports and the wallet snapshot adapter were removed on 2026-10-03.

## Market Intelligence

Owns captured sources, evidence, market events, Research Runs, findings, narratives, and transmission paths. A Research Run is always tied to a fixed cutoff, research window, input snapshot, Skill version, model, schema version, validation result, and data gaps.

Project Skills live under `skills/` and are versioned with the application. Every run hashes both the complete selected Skill directory and the shared research-standards Skill with SHA-256. Later runs receive all validated upstream findings, events, narratives, and transmission paths, allowing cross-market and portfolio-risk Skills to consume the complete structured context. Deterministic parsing, typed object/provenance validation, publication cutoff enforcement, orchestration, bounded retries, and transactional persistence remain Rust responsibilities.

The generic data-source Adapter stores only the payloads actually supplied for reporting. At report creation, the equity-news Adapter first prepares Google News and Bing News RSS results for every current listed-equity instrument, normalizes original article URLs, and filters by publication time. The backend then opens the briefing repeatable-read transaction, fixes the report cutoff to that transaction's timestamp, verifies that the exact instrument set has not changed during collection, and stores one immutable `equity_news` source snapshot per instrument together with the briefing input; a changed portfolio aborts creation and must be retried. The report snapshot keeps the latest captured news set per instrument, removes duplicate raw feed bodies while retaining feed status, and omits generated quote/research payloads already represented by frozen positions or validated upstream output. Feed failures are captured as data, not silently treated as an empty market. Each Research Run receives a bounded projection of that snapshot: position runs get only the exact holding, owner-matched Intent, shared validated context, and instrument-bound news; market runs get only their market's positions; portfolio construction consumes the structured upstream results instead of repeated raw captures. Live search verifies and deepens the frozen candidates. The frozen briefing input also includes market events inside the selected research window, plus the previous validated narrative state. Codex-returned evidence is captured in its own source snapshot before findings link to normalized evidence rows. Captured-source evidence keeps its original source snapshot foreign key and must match its frozen payload; frozen facts keep a typed object reference in evidence metadata; undated external web evidence is rejected. Research-generated events are also copied into immutable `(research_run_id, event_key)` snapshots so backdated reports never resolve a same-named global event from another cutoff.

New stock reports freeze only account-owned A-share, Hong Kong and US equity positions. Wallet positions, smart-money addresses and captured GMGN/Meme payloads are excluded. Validated equity diagnoses may offer an Intent draft, but only a user's explicit confirmation activates it.

## Retired Meme Arena

The arena, GMGN runner/config/dependency, wallet and smart-money UI/APIs, narrative and survival skills, output schema, bubble map, blacklist, report scheduler and workers were removed on 2026-10-03. Additive migration `0026` drops dedicated report/observation/blacklist tables and removes the Meme schedule; applied historical migrations remain unchanged.

The user's one-time local database reset was performed after a verified external backup. Startup never clears business data. The reset retains migration history and a disabled equity schedule, so the user can start fresh without automatic report creation.

## Briefing

Owns immutable reports. Creating a report freezes references to the latest position snapshots and effective Trade Intent versions at the cutoff. Historical reports cannot be regenerated. The PostgreSQL-backed worker claims pending work with `FOR UPDATE SKIP LOCKED`.

A missing research runner produces a traceable `partial` result. It never produces fabricated research to make a report appear complete. When enabled, the worker creates market-specific Research Runs, invokes Codex with the public schema, persists raw output for audit but promotes only validated outputs, then invokes a no-search composition Skill. A validation retry receives the exact deterministic contract failure and must return a complete corrected replacement. Events, narratives, and transmission paths must be linked by `(research_run_id, stable key)` to a validated Finding before entering composition; every model-composed item cites validated Finding IDs, while reserved system items are Rust-only. Failed tasks and the validated composition are committed together as `partial`; a fully validated composition with no failed tasks becomes `complete`.

When the persisted daily plan is enabled, startup catch-up creates the single due daily report; pausing the plan suppresses both startup catch-up and later scheduler creation without affecting manual reports. Schedule updates and automatic report creation share one PostgreSQL advisory lock, and the scheduler rechecks the enabled flag after acquiring it so a successful pause cannot race with a prepared report. Stale `running` reports return to `pending`. Users may also create a new current report manually, but only one report may be active at a time. Each claim gets a unique lease token; all heartbeats and final state changes compare that token, so a recovered old worker cannot overwrite a newer execution. The worker renews the lease before and after bounded Codex attempts, and stale recovery waits longer than one configured Codex timeout. A recovered execution supersedes interrupted Research Runs inside the same report before retrying.

## Runtime boundaries

Each new report freezes `report_language` (`en` or `zh`) in its input snapshot.
Manual requests use the UI selection (API omission defaults to English); scheduled
reports use the persisted schedule language (English by default). Research,
language validation, deterministic position notes and synthesis share that value.
Legacy inputs without the field retain Chinese, including checkpoint recovery.
Changing the interface language never rewrites historical research or user facts.

- React communicates only through the Axum HTTP API.
- The browser client is a plain local Vite application; no cloud worker or hosting runtime is required.
- Axum listens on `127.0.0.1` by default.
- PostgreSQL is the durable store and lightweight job queue.
- No authentication or multi-tenancy is present in V1.
- No asset-wallet private key or trading interface is accepted. GMGN credentials are no longer read or required.
- Codex/Skills cannot update portfolio facts or execute trades.
- Codex executes with a fixed model, read-only sandbox, `approval=never`, ephemeral sessions, user connectors disabled, a cleared environment with an explicit runtime allowlist, and a bounded timeout/attempt count.
