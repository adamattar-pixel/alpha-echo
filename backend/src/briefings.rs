use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use chrono::{DateTime, Duration as ChronoDuration, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Asia::Shanghai;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;

use crate::report_language::ReportLanguage;
use crate::{AppState, error::AppError, news, research};

#[derive(Deserialize)]
struct CreateBriefing {
    #[serde(default = "ReportLanguage::english")]
    language: ReportLanguage,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/briefings", get(list_briefings).post(create_briefing))
        .route("/api/briefings/{id}", get(get_briefing))
}

#[derive(Debug, Serialize, FromRow)]
struct BriefingRun {
    id: Uuid,
    kind: String,
    status: String,
    scheduled_for: Option<DateTime<Utc>>,
    as_of: DateTime<Utc>,
    research_window_start: DateTime<Utc>,
    revision: i32,
    supersedes_id: Option<Uuid>,
    input_snapshot: Value,
    raw_output: Option<Value>,
    rendered_markdown: Option<String>,
    skill_name: Option<String>,
    skill_version: Option<String>,
    model: Option<String>,
    validation_status: Option<String>,
    error: Option<String>,
    data_gaps: Value,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    completed_research_tasks: i64,
    total_research_tasks: i64,
    current_research_tasks: Vec<String>,
}

#[derive(Debug, Serialize, FromRow)]
struct BriefingListRun {
    id: Uuid,
    kind: String,
    status: String,
    as_of: DateTime<Utc>,
    revision: i32,
    supersedes_id: Option<Uuid>,
    rendered_markdown: Option<String>,
    skill_name: Option<String>,
    skill_version: Option<String>,
    model: Option<String>,
    validation_status: Option<String>,
    data_gaps: Value,
    completed_research_tasks: i64,
    total_research_tasks: i64,
    current_research_tasks: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ListParams {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_briefings(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<BriefingListRun>>, AppError> {
    let limit = params.limit.unwrap_or(30).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let runs = sqlx::query_as::<_, BriefingListRun>(
        r#"SELECT br.id, br.kind, br.status, br.as_of, br.revision, br.supersedes_id,
                  br.rendered_markdown, br.skill_name, br.skill_version, br.model,
                  br.validation_status, br.data_gaps,
                  (SELECT count(DISTINCT (rr.market, rr.run_type,
                            COALESCE(rr.input_snapshot->'task_scope'->>'position_snapshot_item_id', '')))
                     FROM research_runs rr
                    WHERE rr.briefing_run_id = br.id
                      AND rr.run_type IN ('macro_context', 'equity_market_context', 'equity_position_diagnosis', 'portfolio_construction')
                      AND rr.status IN ('complete', 'failed')
                      AND COALESCE(rr.validation_status, '') NOT IN ('interrupted', 'superseded')) AS completed_research_tasks,
                  (SELECT 1 + count(DISTINCT position->>'market') + count(*)
                              + CASE WHEN count(DISTINCT position->>'market') > 0 THEN 1 ELSE 0 END
                     FROM jsonb_array_elements(COALESCE(br.input_snapshot->'positions', '[]'::jsonb)) position
                    WHERE position->>'market' IN ('a_share', 'hk', 'us')
                      AND COALESCE((position->>'quantity')::numeric, 0) <> 0) AS total_research_tasks,
                  ARRAY(SELECT rr.skill_name FROM research_runs rr
                         WHERE rr.briefing_run_id = br.id
                           AND rr.run_type IN ('macro_context', 'equity_market_context', 'equity_position_diagnosis', 'portfolio_construction')
                           AND rr.status = 'running'
                         ORDER BY rr.created_at) AS current_research_tasks
           FROM briefing_runs br ORDER BY br.created_at DESC LIMIT $1 OFFSET $2"#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(runs))
}

async fn get_briefing(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let run = sqlx::query_as::<_, BriefingRun>(
        r#"SELECT br.id, br.kind, br.status, br.scheduled_for, br.as_of, br.research_window_start,
                  br.revision, br.supersedes_id, br.input_snapshot, br.raw_output, br.rendered_markdown,
                  br.skill_name, br.skill_version, br.model, br.validation_status, br.error, br.data_gaps,
                  br.created_at, br.started_at, br.completed_at,
                  (SELECT count(DISTINCT (rr.market, rr.run_type,
                            COALESCE(rr.input_snapshot->'task_scope'->>'position_snapshot_item_id', '')))
                     FROM research_runs rr
                    WHERE rr.briefing_run_id = br.id
                      AND rr.run_type IN ('macro_context', 'equity_market_context', 'equity_position_diagnosis', 'portfolio_construction')
                      AND rr.status IN ('complete', 'failed')
                      AND COALESCE(rr.validation_status, '') NOT IN ('interrupted', 'superseded')) AS completed_research_tasks,
                  (SELECT 1 + count(DISTINCT position->>'market') + count(*)
                              + CASE WHEN count(DISTINCT position->>'market') > 0 THEN 1 ELSE 0 END
                     FROM jsonb_array_elements(COALESCE(br.input_snapshot->'positions', '[]'::jsonb)) position
                    WHERE position->>'market' IN ('a_share', 'hk', 'us')
                      AND COALESCE((position->>'quantity')::numeric, 0) <> 0) AS total_research_tasks,
                  ARRAY(SELECT rr.skill_name FROM research_runs rr
                         WHERE rr.briefing_run_id = br.id
                           AND rr.run_type IN ('macro_context', 'equity_market_context', 'equity_position_diagnosis', 'portfolio_construction')
                           AND rr.status = 'running'
                         ORDER BY rr.created_at) AS current_research_tasks
           FROM briefing_runs br WHERE br.id = $1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let items = sqlx::query_as::<_, BriefingItem>(
        "SELECT id, section, sort_order, title, body, severity, source_finding_ids, metadata FROM briefing_items WHERE briefing_run_id = $1 ORDER BY section, sort_order",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let research_runs = sqlx::query_as::<_, ResearchRunSummary>(
        "SELECT id, market, run_type, status, skill_name, skill_version, supporting_skills, model, validation_status, data_gaps FROM research_runs WHERE briefing_run_id = $1 ORDER BY created_at",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        json!({ "run": run, "items": items, "research_runs": research_runs }),
    ))
}

#[derive(Debug, Serialize, FromRow)]
struct BriefingItem {
    id: Uuid,
    section: String,
    sort_order: i32,
    title: String,
    body: String,
    severity: Option<String>,
    source_finding_ids: Value,
    metadata: Value,
}

#[derive(Debug, Serialize, FromRow)]
struct ResearchRunSummary {
    id: Uuid,
    market: String,
    run_type: String,
    status: String,
    skill_name: String,
    skill_version: String,
    supporting_skills: Value,
    model: Option<String>,
    validation_status: Option<String>,
    data_gaps: Value,
}

async fn create_briefing(
    State(state): State<AppState>,
    Json(input): Json<CreateBriefing>,
) -> Result<Json<Value>, AppError> {
    if state.research.enabled {
        state.research.preflight().map_err(AppError::BadRequest)?;
    }
    let positions_as_of = Utc::now();
    let prepared_news = news::prepare_equity_news(&state.pool, positions_as_of)
        .await
        .map_err(AppError::Internal)?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *transaction)
        .await?;
    lock_report_creation(&mut transaction).await?;
    if has_active_report(&mut transaction).await? {
        return Err(AppError::BadRequest(
            "a report is already being generated".into(),
        ));
    }
    let as_of: DateTime<Utc> = sqlx::query_scalar("SELECT transaction_timestamp()")
        .fetch_one(&mut *transaction)
        .await?;
    news::persist_equity_news(&mut transaction, prepared_news, as_of)
        .await
        .map_err(AppError::Internal)?;
    let window_start = as_of - ChronoDuration::hours(12);
    let mut snapshot = capture_input_snapshot(&mut transaction, as_of, window_start).await?;
    snapshot["report_language"] = json!(input.language);

    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO briefing_runs
           (kind, status, scheduled_for, as_of, research_window_start, revision, supersedes_id, input_snapshot)
           VALUES ('report', 'pending', NULL, $1, $2, 1, NULL, $3) RETURNING id"#,
    )
    .bind(as_of)
    .bind(window_start)
    .bind(snapshot)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Json(
        json!({ "id": id, "status": "pending", "as_of": as_of }),
    ))
}

pub(crate) async fn lock_report_creation(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('alpha-echo-report-creation'))")
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn has_active_report(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM briefing_runs WHERE status IN ('pending', 'running'))",
    )
    .fetch_one(&mut **transaction)
    .await
}

async fn capture_input_snapshot(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    as_of: DateTime<Utc>,
    window_start: DateTime<Utc>,
) -> Result<Value, AppError> {
    let positions: Value = sqlx::query_scalar(
        r#"WITH latest AS (
             SELECT DISTINCT ON (ps.account_id, ps.wallet_id)
               ps.id, ps.account_id, ps.wallet_id, ps.as_of, ps.source
             FROM position_snapshots ps
             WHERE ps.as_of <= $1 AND ps.account_id IS NOT NULL
             ORDER BY ps.account_id, ps.wallet_id, ps.as_of DESC, ps.created_at DESC, ps.id DESC
           )
           SELECT COALESCE(jsonb_agg(to_jsonb(position_row) ORDER BY position_row.market, position_row.symbol), '[]'::jsonb)
           FROM (
             SELECT l.id AS snapshot_id, l.as_of, l.source,
                    l.account_id, l.wallet_id,
                    a.name AS owner_name, 'account' AS owner_type,
                    psi.id AS position_snapshot_item_id,
                    i.id AS instrument_id, i.market, i.symbol, i.name AS instrument_name,
                    COALESCE(a.currency, i.currency) AS currency,
                    i.exchange, i.chain, i.contract_address,
                    psi.quantity::text, psi.average_cost::text, psi.market_price::text,
                    psi.market_value::text, psi.unrealized_pnl::text
             FROM latest l
             JOIN position_snapshot_items psi ON psi.snapshot_id = l.id
             JOIN instruments i ON i.id = psi.instrument_id
             JOIN accounts a ON a.id = l.account_id
             WHERE i.market IN ('a_share', 'hk', 'us')
           ) position_row"#,
    )
    .bind(as_of)
    .fetch_one(&mut **transaction)
    .await?;
    let intents: Value = sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(intent_row) ORDER BY intent_row.trade_intent_id), '[]'::jsonb)
           FROM (
             SELECT DISTINCT ON (tiv.trade_intent_id)
               tiv.id AS trade_intent_version_id, tiv.trade_intent_id, tiv.version,
               tiv.title, tiv.strategy_type, tiv.time_horizon, tiv.status, tiv.thesis,
               tiv.risk_budget::text, tiv.target_position::text, tiv.catalysts,
               tiv.invalidation_conditions, tiv.exit_principles, tiv.notes,
               tiv.instrument_ids, tiv.account_id, tiv.wallet_id,
               tiv.position_slice::text, tiv.created_at
             FROM trade_intent_versions tiv
             WHERE tiv.created_at <= $1
             ORDER BY tiv.trade_intent_id, tiv.version DESC
           ) intent_row
           WHERE intent_row.status = 'active' AND intent_row.wallet_id IS NULL
             AND NOT EXISTS (
               SELECT 1 FROM instruments i
               WHERE i.id = ANY(intent_row.instrument_ids)
                 AND i.market NOT IN ('a_share', 'hk', 'us')
             )"#,
    )
    .bind(as_of)
    .fetch_one(&mut **transaction)
    .await?;
    let prior_narratives: Value = sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(narrative_row) ORDER BY narrative_row.market, narrative_row.name), '[]'::jsonb)
           FROM (
             SELECT DISTINCT ON (mn.id)
               mn.id AS narrative_id, mn.market, mn.name, mn.narrative_type,
               ns.as_of, ns.stage, ns.summary, ns.confidence::text,
               ns.capital_flow_inference
             FROM market_narratives mn
             JOIN narrative_snapshots ns ON ns.narrative_id = mn.id
             JOIN research_runs rr ON rr.id = ns.research_run_id
             WHERE ns.as_of < $1 AND rr.validation_status = 'valid'
               AND mn.market IN ('macro', 'a_share', 'hk', 'us', 'cross', 'portfolio')
             ORDER BY mn.id, ns.as_of DESC, ns.created_at DESC
           ) narrative_row"#,
    )
    .bind(as_of)
    .fetch_one(&mut **transaction)
    .await?;
    let mut source_snapshots: Value = sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(source_row) ORDER BY source_row.as_of), '[]'::jsonb)
           FROM (
             SELECT id AS source_snapshot_id, provider, source_type, captured_at,
                    as_of, request, payload, checksum
             FROM source_snapshots
             WHERE as_of >= $1 AND as_of <= $2
               AND source_type NOT LIKE 'meme_%'
               AND source_type <> 'wallet_portfolio' AND provider <> 'gmgn'
           ) source_row"#,
    )
    .bind(window_start)
    .bind(as_of)
    .fetch_one(&mut **transaction)
    .await?;
    research::compact_report_source_snapshots(&mut source_snapshots);
    let market_events: Value = sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(event_row) ORDER BY event_row.occurred_at), '[]'::jsonb)
           FROM (
             SELECT id AS market_event_id, event_key, market, event_type,
                    occurred_at, headline, facts
             FROM market_events
             WHERE occurred_at >= $1 AND occurred_at <= $2
               AND market IN ('macro', 'a_share', 'hk', 'us', 'cross', 'portfolio')
           ) event_row"#,
    )
    .bind(window_start)
    .bind(as_of)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(json!({
        "captured_at": as_of,
        "research_window_start": window_start,
        "positions": positions,
        "trade_intent_versions": intents,
        "prior_narratives": prior_narratives,
        "source_snapshots": source_snapshots,
        "market_events": market_events,
    }))
}

pub async fn run_worker(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    loop {
        interval.tick().await;
        if let Err(error) = process_next(&state).await {
            tracing::error!(?error, "briefing worker iteration failed");
        }
    }
}

#[derive(Debug, Deserialize)]
struct BriefingSchedule {
    #[serde(default = "ReportLanguage::english")]
    language: ReportLanguage,
    timezone: String,
    time: String,
    enabled: bool,
}

pub async fn run_scheduler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        if let Err(error) = recover_stale_runs(&state).await {
            tracing::error!(?error, "stale briefing recovery iteration failed");
        }
        if let Err(error) = ensure_due_scheduled_runs(&state).await {
            tracing::error!(?error, "briefing scheduler iteration failed");
        }
    }
}

pub async fn ensure_due_scheduled_runs(state: &AppState) -> Result<u64, AppError> {
    let value: Value =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'briefing_schedule'")
            .fetch_one(&state.pool)
            .await?;
    let schedule: BriefingSchedule = serde_json::from_value(value)
        .map_err(|error| AppError::BadRequest(format!("invalid briefing schedule: {error}")))?;
    if schedule.timezone != "Asia/Shanghai" {
        return Err(AppError::BadRequest(
            "V1 currently supports only the Asia/Shanghai briefing timezone".into(),
        ));
    }
    if !schedule.enabled {
        return Ok(0);
    }

    let now = Utc::now().with_timezone(&Shanghai);
    let time = parse_schedule_time(&schedule.time)?;
    if now.time().num_seconds_from_midnight() < time.num_seconds_from_midnight() {
        return Ok(0);
    }
    let should_skip: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
             SELECT 1 FROM briefing_runs
             WHERE status IN ('pending', 'running')
                OR (scheduled_for IS NOT NULL
                    AND (scheduled_for AT TIME ZONE 'Asia/Shanghai')::date = $1)
           )"#,
    )
    .bind(now.date_naive())
    .fetch_one(&state.pool)
    .await?;
    if should_skip {
        return Ok(0);
    }
    let positions_as_of = Utc::now();
    let prepared_news = news::prepare_equity_news(&state.pool, positions_as_of)
        .await
        .map_err(AppError::Internal)?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *transaction)
        .await?;
    lock_report_creation(&mut transaction).await?;
    let current_schedule_value: Value = sqlx::query_scalar(
        r#"SELECT value
           FROM app_settings WHERE key = 'briefing_schedule'
           FOR UPDATE"#,
    )
    .fetch_one(&mut *transaction)
    .await?;
    let current_schedule: BriefingSchedule = serde_json::from_value(current_schedule_value)
        .map_err(|error| AppError::BadRequest(format!("invalid briefing schedule: {error}")))?;
    if current_schedule.timezone != "Asia/Shanghai" {
        return Err(AppError::BadRequest(
            "V1 currently supports only the Asia/Shanghai briefing timezone".into(),
        ));
    }
    if !current_schedule.enabled {
        return Ok(0);
    }
    let locked_now = Utc::now().with_timezone(&Shanghai);
    let current_time = parse_schedule_time(&current_schedule.time)?;
    if locked_now.time().num_seconds_from_midnight() < current_time.num_seconds_from_midnight() {
        return Ok(0);
    }
    let current_local_scheduled = locked_now.date_naive().and_time(current_time);
    let scheduled_for = Shanghai
        .from_local_datetime(&current_local_scheduled)
        .single()
        .ok_or_else(|| AppError::BadRequest("invalid local scheduled time".into()))?
        .with_timezone(&Utc);
    if has_active_report(&mut transaction).await? {
        return Ok(0);
    }
    let already_scheduled_today: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
             SELECT 1 FROM briefing_runs
             WHERE scheduled_for IS NOT NULL
               AND (scheduled_for AT TIME ZONE 'Asia/Shanghai')::date = $1
           )"#,
    )
    .bind(locked_now.date_naive())
    .fetch_one(&mut *transaction)
    .await?;
    if already_scheduled_today {
        return Ok(0);
    }
    let as_of: DateTime<Utc> = sqlx::query_scalar("SELECT transaction_timestamp()")
        .fetch_one(&mut *transaction)
        .await?;
    news::persist_equity_news(&mut transaction, prepared_news, as_of)
        .await
        .map_err(AppError::Internal)?;
    let window_start = as_of - ChronoDuration::hours(12);
    let mut input_snapshot = capture_input_snapshot(&mut transaction, as_of, window_start).await?;
    input_snapshot["report_language"] = json!(current_schedule.language);
    let result = sqlx::query(
        r#"INSERT INTO briefing_runs
           (kind, status, scheduled_for, as_of, research_window_start, revision, input_snapshot)
           VALUES ('report', 'pending', $1, $2, $3, 1, $4)"#,
    )
    .bind(scheduled_for)
    .bind(as_of)
    .bind(window_start)
    .bind(input_snapshot)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(result.rows_affected())
}

fn parse_schedule_time(value: &str) -> Result<NaiveTime, AppError> {
    NaiveTime::parse_from_str(value, "%H:%M")
        .map_err(|error| AppError::BadRequest(format!("invalid report time: {error}")))
}

pub async fn recover_stale_runs(state: &AppState) -> Result<u64, AppError> {
    let stale_after_seconds = state
        .research
        .timeout
        .as_secs()
        .saturating_add(300)
        .min(i64::MAX as u64) as i64;
    let result = sqlx::query(
        r#"UPDATE briefing_runs
           SET status = 'pending', heartbeat_at = NULL, lease_token = NULL,
               error = 'recovered after the worker stopped while running'
           WHERE status = 'running'
             AND heartbeat_at < now() - ($1 * interval '1 second')"#,
    )
    .bind(stale_after_seconds)
    .execute(&state.pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn recover_interrupted_runs_on_startup(state: &AppState) -> Result<u64, AppError> {
    let result = sqlx::query(
        r#"UPDATE briefing_runs
           SET status = 'pending', heartbeat_at = NULL, lease_token = NULL,
               error = 'resuming after the research worker restarted'
           WHERE status = 'running'"#,
    )
    .execute(&state.pool)
    .await?;
    Ok(result.rows_affected())
}

async fn process_next(state: &AppState) -> Result<bool, AppError> {
    let mut transaction = state.pool.begin().await?;
    let run: Option<(Uuid, String, DateTime<Utc>)> = sqlx::query_as(
        r#"SELECT id, kind, as_of
           FROM briefing_runs
           WHERE status = 'pending'
           ORDER BY created_at
           FOR UPDATE SKIP LOCKED LIMIT 1"#,
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let Some((id, kind, as_of)) = run else {
        transaction.rollback().await?;
        return Ok(false);
    };
    let lease_token = Uuid::new_v4();
    sqlx::query(
        "UPDATE briefing_runs SET status = 'running', started_at = COALESCE(started_at, now()), heartbeat_at = now(), lease_token = $2, error = NULL WHERE id = $1",
    )
        .bind(id)
        .bind(lease_token)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;

    let result = if state.research.enabled {
        research::execute_briefing(state, id, lease_token)
            .await
            .map_err(AppError::Internal)
    } else {
        finish_without_runner(state, id, lease_token, &kind, as_of).await
    };
    if let Err(error) = result {
        let message = error.to_string();
        if let Err(mark_error) = sqlx::query(
            "UPDATE briefing_runs SET status = 'failed', error = $3, completed_at = now(), heartbeat_at = NULL, lease_token = NULL WHERE id = $1 AND lease_token = $2",
        )
        .bind(id)
        .bind(lease_token)
        .bind(&message)
        .execute(&state.pool)
        .await
        {
            tracing::error!(?mark_error, %id, "failed to persist briefing failure");
        }
        return Err(error);
    }
    Ok(true)
}

async fn finish_without_runner(
    state: &AppState,
    id: Uuid,
    lease_token: Uuid,
    kind: &str,
    as_of: DateTime<Utc>,
) -> Result<(), AppError> {
    // Preserve a partial briefing without claiming that a research or composition
    // Skill ran. No Research Run is created until the Codex runner is configured.
    let data_gaps = json!([
        "Codex research runner is not configured",
        "News, filing, and macro research require the configured Codex runner"
    ]);
    let snapshot: Value =
        sqlx::query_scalar("SELECT input_snapshot FROM briefing_runs WHERE id = $1")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
    let language = ReportLanguage::from_input(&snapshot);
    let mut transaction = state.pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO briefing_items
           (briefing_run_id, section, sort_order, title, body, severity, metadata)
           VALUES ($1, 'system', 1, $3,
                   $4,
                   'info', $2)
           ON CONFLICT (briefing_run_id, section, sort_order)
           DO UPDATE SET title = EXCLUDED.title, body = EXCLUDED.body,
                         severity = EXCLUDED.severity, metadata = EXCLUDED.metadata"#,
    )
    .bind(id)
    .bind(json!({ "no_opportunity_forced": true }))
    .bind(language.text("研究链路待配置", "Research executor not configured"))
    .bind(language.text("已固化持仓与交易意图版本；由于研究执行器尚未配置，本报告没有生成股票诊断或组合判断。", "Holdings and Trade Intent versions were frozen. No equity diagnosis or portfolio assessment was generated because the research executor is not configured."))
    .execute(&mut *transaction)
    .await?;
    let rendered = if language == ReportLanguage::En {
        format!(
            "# Report\n\nAs of: {}\n\nInputs were frozen. The research executor is not configured; no market assessment or trading opportunity was generated.",
            as_of.to_rfc3339()
        )
    } else {
        format!(
            "# {}报告\n\n截止时间：{}\n\n已固化输入。研究执行器尚未配置，本次没有生成市场判断或交易机会。",
            if kind == "report" { "" } else { "历史" },
            as_of.to_rfc3339()
        )
    };
    let updated = sqlx::query(
        r#"UPDATE briefing_runs
           SET status = 'partial', rendered_markdown = $2, validation_status = 'not_run',
               skill_name = NULL, skill_version = NULL, model = NULL,
               data_gaps = $3, error = NULL, completed_at = now(), heartbeat_at = NULL, lease_token = NULL
           WHERE id = $1 AND lease_token = $4"#,
    )
    .bind(id)
    .bind(rendered)
    .bind(data_gaps)
    .bind(lease_token)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(AppError::Internal(
            "briefing lease was lost before fallback persistence".into(),
        ));
    }
    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_language_selection_defaults_to_english_and_accepts_chinese() {
        let manual: CreateBriefing = serde_json::from_value(json!({})).unwrap();
        assert_eq!(manual.language, ReportLanguage::En);
        let chinese: CreateBriefing = serde_json::from_value(json!({"language": "zh"})).unwrap();
        assert_eq!(chinese.language, ReportLanguage::Zh);
        assert!(serde_json::from_value::<CreateBriefing>(json!({"language": "fr"})).is_err());
        let schedule: BriefingSchedule = serde_json::from_value(
            json!({"timezone": "Asia/Shanghai", "time": "19:00", "enabled": false}),
        )
        .unwrap();
        assert_eq!(schedule.language, ReportLanguage::En);
    }

    #[tokio::test]
    async fn unavailable_executor_rejects_creation_before_database_or_news_access() {
        let state = AppState {
            pool: sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://127.0.0.1:1/unused")
                .unwrap(),
            research: crate::config::ResearchRunnerConfig {
                enabled: true,
                codex_binary: std::path::PathBuf::from("/missing/alpha-echo-test-codex"),
                model: Some("test-model".into()),
                reasoning_effort: "medium".into(),
                project_root: std::env::temp_dir(),
                timeout: std::time::Duration::from_secs(1),
                max_attempts: 1,
            },
        };
        assert_eq!(crate::research::runner_label(&state), "not_configured");
        let error = create_briefing(
            State(state),
            Json(CreateBriefing {
                language: ReportLanguage::En,
            }),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, AppError::BadRequest(message) if message.contains("CODEX_BINARY")));
    }

    #[test]
    fn unified_report_schedule_accepts_one_daily_time() {
        assert_eq!(
            parse_schedule_time("19:00").unwrap(),
            NaiveTime::from_hms_opt(19, 0, 0).unwrap()
        );
        assert!(parse_schedule_time("morning").is_err());
        assert!(
            serde_json::from_value::<BriefingSchedule>(json!({
                "timezone": "Asia/Shanghai",
                "time": "19:00",
                "enabled": true
            }))
            .is_ok()
        );
        assert!(
            serde_json::from_value::<BriefingSchedule>(json!({
                "timezone": "Asia/Shanghai",
                "morning": "07:30",
                "evening": "19:00"
            }))
            .is_err()
        );
        let paused = serde_json::from_value::<BriefingSchedule>(json!({
            "timezone": "Asia/Shanghai",
            "time": "19:00",
            "enabled": false
        }))
        .unwrap();
        assert!(!paused.enabled);
    }
}
