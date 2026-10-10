use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{AppState, error::AppError};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/data-sources/snapshots",
            get(list_source_snapshots).post(create_source_snapshot),
        )
        .route(
            "/api/data-sources/events",
            get(list_market_events).post(create_market_event),
        )
        .route("/api/intelligence", get(list_intelligence))
        .route("/api/skills", get(list_skills))
        .route("/api/settings", get(get_settings).put(update_settings))
}

#[derive(Debug, Deserialize)]
struct LimitParams {
    limit: Option<i64>,
}

#[derive(Debug, Serialize, FromRow)]
struct SourceSnapshotView {
    id: Uuid,
    provider: String,
    source_type: String,
    captured_at: DateTime<Utc>,
    as_of: DateTime<Utc>,
    request: Value,
    payload: Value,
    checksum: String,
}

async fn list_source_snapshots(
    State(state): State<AppState>,
    Query(params): Query<LimitParams>,
) -> Result<Json<Vec<SourceSnapshotView>>, AppError> {
    let limit = params.limit.unwrap_or(30).clamp(1, 100);
    let rows = sqlx::query_as::<_, SourceSnapshotView>(
        r#"SELECT id, provider, source_type, captured_at, as_of, request, payload, checksum
           FROM source_snapshots ORDER BY captured_at DESC LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct CreateSourceSnapshot {
    provider: String,
    source_type: String,
    as_of: DateTime<Utc>,
    #[serde(default)]
    request: Value,
    payload: Value,
}

async fn create_source_snapshot(
    State(state): State<AppState>,
    Json(input): Json<CreateSourceSnapshot>,
) -> Result<Json<Value>, AppError> {
    if input.provider.trim().is_empty() || input.source_type.trim().is_empty() {
        return Err(AppError::BadRequest(
            "provider and source_type are required".into(),
        ));
    }
    if input.as_of > Utc::now() + chrono::Duration::minutes(5) {
        return Err(AppError::BadRequest(
            "source snapshot as_of cannot be in the future".into(),
        ));
    }
    let checksum = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&input.payload)
                .map_err(|error| AppError::BadRequest(error.to_string()))?
        )
    );
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO source_snapshots
           (provider, source_type, captured_at, as_of, request, payload, checksum)
           VALUES ($1, $2, now(), $3, $4, $5, $6) RETURNING id"#,
    )
    .bind(input.provider.trim())
    .bind(input.source_type.trim())
    .bind(input.as_of)
    .bind(input.request)
    .bind(input.payload)
    .bind(&checksum)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "id": id, "checksum": checksum })))
}

#[derive(Debug, Serialize, FromRow)]
struct MarketEventView {
    id: Uuid,
    event_key: String,
    market: String,
    event_type: String,
    occurred_at: DateTime<Utc>,
    headline: String,
    facts: Value,
}

async fn list_market_events(
    State(state): State<AppState>,
    Query(params): Query<LimitParams>,
) -> Result<Json<Vec<MarketEventView>>, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let rows = sqlx::query_as::<_, MarketEventView>(
        r#"SELECT id, event_key, market, event_type, occurred_at, headline, facts
           FROM market_events ORDER BY occurred_at DESC LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct CreateMarketEvent {
    event_key: String,
    market: String,
    event_type: String,
    occurred_at: DateTime<Utc>,
    headline: String,
    #[serde(default)]
    facts: Value,
}

async fn create_market_event(
    State(state): State<AppState>,
    Json(input): Json<CreateMarketEvent>,
) -> Result<Json<Value>, AppError> {
    if [
        input.event_key.as_str(),
        input.market.as_str(),
        input.event_type.as_str(),
        input.headline.as_str(),
    ]
    .iter()
    .any(|value| value.trim().is_empty())
    {
        return Err(AppError::BadRequest(
            "event_key, market, event_type and headline are required".into(),
        ));
    }
    let id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO market_events
           (event_key, market, event_type, occurred_at, headline, facts)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (event_key) DO UPDATE
             SET market = EXCLUDED.market, event_type = EXCLUDED.event_type,
                 occurred_at = EXCLUDED.occurred_at, headline = EXCLUDED.headline,
                 facts = EXCLUDED.facts
           RETURNING id"#,
    )
    .bind(input.event_key.trim())
    .bind(input.market.trim())
    .bind(input.event_type.trim())
    .bind(input.occurred_at)
    .bind(input.headline.trim())
    .bind(input.facts)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Debug, Deserialize)]
struct IntelligenceParams {
    market: Option<String>,
    limit: Option<i64>,
    briefing_id: Option<Uuid>,
}

async fn list_intelligence(
    State(state): State<AppState>,
    Query(params): Query<IntelligenceParams>,
) -> Result<Json<Value>, AppError> {
    let market = params.market.as_deref();
    let briefing_id = params.briefing_id;
    let limit = if briefing_id.is_some() {
        i64::MAX
    } else {
        params.limit.unwrap_or(100).clamp(1, 300)
    };
    let runs: Value = sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(run_row) ORDER BY run_row.created_at DESC), '[]'::jsonb)
           FROM (
             SELECT id, briefing_run_id, market, run_type, status, as_of,
                    research_window_start, skill_name, skill_version, model,
                    supporting_skills, schema_version, validation_status, error, data_gaps,
                    created_at, completed_at
             FROM research_runs
             WHERE ($1::text IS NULL OR market = $1)
               AND ($3::uuid IS NULL OR briefing_run_id = $3)
             ORDER BY created_at DESC LIMIT $2
           ) run_row"#,
    )
    .bind(market)
    .bind(limit)
    .bind(briefing_id)
    .fetch_one(&state.pool)
    .await?;
    let findings: Value = sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(finding_row) ORDER BY finding_row.created_at DESC), '[]'::jsonb)
           FROM (
             SELECT rf.*, rr.skill_name, rr.skill_version,
                    affected_item.instrument_id AS affected_instrument_id,
                    affected_snapshot.account_id AS affected_account_id,
                    affected_snapshot.wallet_id AS affected_wallet_id,
                    affected_snapshot.as_of AS draft_as_of,
                    COALESCE(
                      affected_item.quantity = current_position.quantity
                      AND affected_item.average_cost IS NOT DISTINCT FROM current_position.average_cost,
                      false
                    ) AND NOT EXISTS (
                      SELECT 1 FROM trades t
                      WHERE t.instrument_id = affected_item.instrument_id
                        AND t.account_id IS NOT DISTINCT FROM affected_snapshot.account_id
                        AND t.wallet_id IS NOT DISTINCT FROM affected_snapshot.wallet_id
                        AND t.side IN ('buy', 'sell')
                        AND t.created_at > affected_snapshot.created_at
                    ) AS draft_position_current,
                    COALESCE((
                      SELECT jsonb_agg(jsonb_build_object(
                        'id', es.id, 'source_type', es.source_type,
                        'coverage_scope', es.metadata->>'coverage_scope',
                        'related_instrument_id', es.metadata->>'related_instrument_id',
                        'frozen_object_type', es.metadata->>'frozen_object_type',
                        'frozen_object_id', es.metadata->>'frozen_object_id', 'title', es.title,
                        'url', es.url, 'publisher', es.publisher,
                        'published_at', es.published_at, 'observed_at', es.observed_at,
                        'excerpt', es.excerpt, 'relevance', fe.relevance
                      ) ORDER BY es.observed_at DESC)
                      FROM finding_evidence fe
                      JOIN evidence_sources es ON es.id = fe.evidence_source_id
                      WHERE fe.finding_id = rf.id
                    ), '[]'::jsonb) AS evidence
             FROM research_findings rf
             JOIN research_runs rr ON rr.id = rf.research_run_id
             LEFT JOIN position_snapshot_items affected_item
               ON rf.affected_object_type = 'position_snapshot_item'
              AND affected_item.id = rf.affected_object_id
             LEFT JOIN position_snapshots affected_snapshot
               ON affected_snapshot.id = affected_item.snapshot_id
             LEFT JOIN LATERAL (
               SELECT current_item.quantity, current_item.average_cost
               FROM position_snapshots current_snapshot
               JOIN position_snapshot_items current_item
                 ON current_item.snapshot_id = current_snapshot.id
                AND current_item.instrument_id = affected_item.instrument_id
               WHERE (affected_snapshot.account_id IS NOT NULL
                      AND current_snapshot.account_id = affected_snapshot.account_id)
                  OR (affected_snapshot.wallet_id IS NOT NULL
                      AND current_snapshot.wallet_id = affected_snapshot.wallet_id)
               ORDER BY current_snapshot.as_of DESC,
                        current_snapshot.created_at DESC,
                        current_snapshot.id DESC
               LIMIT 1
             ) current_position ON true
             WHERE rr.validation_status = 'valid'
               AND ($1::text IS NULL OR rf.market = $1)
               AND ($3::uuid IS NULL OR rr.briefing_run_id = $3)
             ORDER BY rf.created_at DESC LIMIT $2
           ) finding_row"#,
    )
    .bind(market)
    .bind(limit)
    .bind(briefing_id)
    .fetch_one(&state.pool)
    .await?;
    let narratives: Value = sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(narrative_row) ORDER BY narrative_row.as_of DESC), '[]'::jsonb)
           FROM (
             SELECT mn.id, mn.market, mn.name, mn.narrative_type,
                    ns.research_run_id, ns.as_of, ns.stage, ns.summary,
                    ns.confidence::text, ns.capital_flow_inference, ns.evidence
             FROM market_narratives mn
             JOIN narrative_snapshots ns ON ns.narrative_id = mn.id
             JOIN research_runs rr ON rr.id = ns.research_run_id
             WHERE rr.validation_status = 'valid'
               AND ($1::text IS NULL OR mn.market = $1)
               AND ($3::uuid IS NULL OR rr.briefing_run_id = $3)
             ORDER BY ns.as_of DESC LIMIT $2
           ) narrative_row"#,
    )
    .bind(market)
    .bind(limit)
    .bind(briefing_id)
    .fetch_one(&state.pool)
    .await?;
    let paths: Value = sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(path_row) ORDER BY path_row.created_at DESC), '[]'::jsonb)
           FROM (
             SELECT tp.* FROM transmission_paths tp
             JOIN research_runs rr ON rr.id = tp.research_run_id
             WHERE rr.validation_status = 'valid'
               AND ($1::text IS NULL OR tp.target_market = $1)
               AND ($3::uuid IS NULL OR rr.briefing_run_id = $3)
             ORDER BY tp.created_at DESC LIMIT $2
           ) path_row"#,
    )
    .bind(market)
    .bind(limit)
    .bind(briefing_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({
        "market": market,
        "briefing_id": briefing_id,
        "runs": runs,
        "findings": findings,
        "narratives": narratives,
        "transmission_paths": paths,
    })))
}

const REQUIRED_SKILLS: &[&str] = &[
    "apply-investment-research-standards",
    "analyze-macro-context",
    "analyze-equity-market-context",
    "diagnose-equity-positions",
    "assess-portfolio-construction",
    "compose-portfolio-briefing",
];

async fn list_skills(State(state): State<AppState>) -> Json<Value> {
    let skills: Vec<Value> = REQUIRED_SKILLS
        .iter()
        .map(|name| {
            let path = state
                .research
                .project_root
                .join("skills")
                .join(name)
                .join("SKILL.md");
            json!({ "name": name, "status": if path.is_file() { "ready" } else { "missing" } })
        })
        .collect();
    Json(json!({ "skills": skills }))
}

async fn get_settings(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let schedule: Value =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'briefing_schedule'")
            .fetch_one(&state.pool)
            .await?;
    let source_snapshots: i64 = sqlx::query_scalar("SELECT count(*) FROM source_snapshots")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(json!({
        "briefing_schedule": schedule,
        "research_runner": crate::research::runner_label(&state),
        "source_snapshots": source_snapshots,
    })))
}

#[derive(Debug, Deserialize)]
struct UpdateSettings {
    #[serde(default = "crate::report_language::ReportLanguage::english")]
    language: crate::report_language::ReportLanguage,
    timezone: String,
    time: String,
    enabled: bool,
}

async fn update_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateSettings>,
) -> Result<Json<Value>, AppError> {
    if input.timezone != "Asia/Shanghai" {
        return Err(AppError::BadRequest(
            "V1 supports only the Asia/Shanghai timezone".into(),
        ));
    }
    chrono::NaiveTime::parse_from_str(&input.time, "%H:%M")
        .map_err(|error| AppError::BadRequest(format!("invalid report time: {error}")))?;
    let schedule = json!({
        "language": input.language,
        "timezone": input.timezone,
        "time": input.time,
        "enabled": input.enabled,
    });
    let mut transaction = state.pool.begin().await?;
    crate::briefings::lock_report_creation(&mut transaction).await?;
    sqlx::query(
        r#"INSERT INTO app_settings (key, value, updated_at)
           VALUES ('briefing_schedule', $1, now())
           ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()"#,
    )
    .bind(&schedule)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Json(schedule))
}
