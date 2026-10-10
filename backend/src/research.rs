use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Stdio,
};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use tokio::{io::AsyncWriteExt, process::Command, task::JoinSet};
use uuid::Uuid;

use crate::AppState;
use crate::report_language::ReportLanguage;

macro_rules! localized_format {
    ($language:expr, $zh:literal, $en:literal $(, $argument:expr)* $(,)?) => {
        match $language {
            ReportLanguage::Zh => format!($zh $(, $argument)*),
            ReportLanguage::En => format!($en $(, $argument)*),
        }
    };
}

const RESEARCH_SCHEMA_VERSION: &str = "research-output-v1";
const MAX_CODEX_PROMPT_BYTES: usize = 900_000;
const BRIEFING_SCHEMA_VERSION: &str = "briefing-output-v1";

#[derive(Clone, Copy, Debug)]
struct ResearchTask {
    market: &'static str,
    run_type: &'static str,
    skill_name: &'static str,
    position_snapshot_item_id: Option<Uuid>,
}

const MACRO_TASK: ResearchTask = ResearchTask {
    market: "macro",
    run_type: "macro_context",
    skill_name: "analyze-macro-context",
    position_snapshot_item_id: None,
};

const PORTFOLIO_TASK: ResearchTask = ResearchTask {
    market: "portfolio",
    run_type: "portfolio_construction",
    skill_name: "assess-portfolio-construction",
    position_snapshot_item_id: None,
};

fn active_equity_markets(frozen_input: &Value) -> Vec<&'static str> {
    let positions = frozen_input
        .get("positions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    ["a_share", "hk", "us"]
        .into_iter()
        .filter(|market| {
            positions.iter().any(|position| {
                position.get("market").and_then(Value::as_str) == Some(*market)
                    && position
                        .get("quantity")
                        .and_then(Value::as_str)
                        .and_then(|value| value.parse::<f64>().ok())
                        .is_some_and(|quantity| quantity != 0.0)
            })
        })
        .collect()
}

pub fn research_task_count(frozen_input: &Value) -> i64 {
    let markets = active_equity_markets(frozen_input).len() as i64;
    let positions = active_equity_position_count(frozen_input);
    1 + markets + positions + i64::from(markets > 0)
}

fn active_equity_position_count(frozen_input: &Value) -> i64 {
    frozen_input
        .get("positions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|position| {
            matches!(
                position.get("market").and_then(Value::as_str),
                Some("a_share" | "hk" | "us")
            ) && position
                .get("quantity")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<f64>().ok())
                .is_some_and(|quantity| quantity != 0.0)
        })
        .count() as i64
}

fn active_equity_positions(frozen_input: &Value) -> Vec<(&'static str, Uuid)> {
    let positions = frozen_input
        .get("positions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    positions
        .iter()
        .filter_map(|position| {
            let market = match position.get("market").and_then(Value::as_str)? {
                "a_share" => "a_share",
                "hk" => "hk",
                "us" => "us",
                _ => return None,
            };
            let non_zero = position
                .get("quantity")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<f64>().ok())
                .is_some_and(|quantity| quantity != 0.0);
            if !non_zero {
                return None;
            }
            position
                .get("position_snapshot_item_id")
                .and_then(Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok())
                .map(|id| (market, id))
        })
        .collect()
}

fn research_stages(frozen_input: &Value) -> Vec<Vec<ResearchTask>> {
    let markets = active_equity_markets(frozen_input);
    let market_context = markets
        .iter()
        .map(|market| ResearchTask {
            market,
            run_type: "equity_market_context",
            skill_name: "analyze-equity-market-context",
            position_snapshot_item_id: None,
        })
        .collect();
    let position_diagnosis = active_equity_positions(frozen_input)
        .into_iter()
        .map(|(market, position_snapshot_item_id)| ResearchTask {
            market,
            run_type: "equity_position_diagnosis",
            skill_name: "diagnose-equity-positions",
            position_snapshot_item_id: Some(position_snapshot_item_id),
        })
        .collect();
    let portfolio = if markets.is_empty() {
        Vec::new()
    } else {
        vec![PORTFOLIO_TASK]
    };
    vec![
        vec![MACRO_TASK],
        market_context,
        position_diagnosis,
        portfolio,
    ]
}

fn is_reusable_task_run(status: &str, validation_status: Option<&str>) -> bool {
    matches!(status, "complete" | "failed")
        && !matches!(validation_status, Some("interrupted" | "superseded"))
}

fn is_reusable_task_version(
    status: &str,
    validation_status: Option<&str>,
    recorded_skill_version: &str,
    current_skill_version: &str,
) -> bool {
    is_reusable_task_run(status, validation_status)
        && !(status == "failed" && recorded_skill_version != current_skill_version)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ResearchOutput {
    as_of: String,
    skill_name: String,
    data_gaps: Vec<String>,
    evidence: Vec<EvidenceOutput>,
    events: Vec<EventOutput>,
    findings: Vec<FindingOutput>,
    narratives: Vec<NarrativeOutput>,
    transmission_paths: Vec<TransmissionPathOutput>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EvidenceOutput {
    key: String,
    source_type: String,
    coverage_scope: String,
    #[serde(default)]
    related_instrument_id: Option<String>,
    #[serde(default)]
    relationship_basis: Option<String>,
    #[serde(default)]
    relationship_explanation: Option<String>,
    title: String,
    url: Option<String>,
    publisher: Option<String>,
    published_at: Option<String>,
    observed_at: String,
    excerpt: Option<String>,
    source_snapshot_id: Option<String>,
    frozen_object_type: Option<String>,
    frozen_object_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EventOutput {
    event_key: String,
    market: String,
    event_type: String,
    is_scheduled: bool,
    occurred_at: String,
    headline: String,
    facts: Vec<String>,
    evidence_keys: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FindingOutput {
    finding_type: String,
    market: String,
    window_start: String,
    window_end: String,
    affected_object_type: Option<String>,
    affected_object_id: Option<String>,
    fact_summary: String,
    inference: Option<String>,
    impact_direction: Option<String>,
    impact_strength: Option<i16>,
    time_horizon: Option<String>,
    confidence: f64,
    contrary_evidence: Vec<String>,
    confirmation_conditions: Vec<String>,
    invalidation_conditions: Vec<String>,
    conditional_action: Option<String>,
    data_gaps: Vec<String>,
    evidence_keys: Vec<String>,
    related_event_keys: Vec<String>,
    related_narrative_keys: Vec<String>,
    related_transmission_path_keys: Vec<String>,
    analysis: Option<EquityDiagnosticDimensions>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EquityDiagnosticDimensions {
    intent_assessment: String,
    intent_draft: Option<EquityIntentDraft>,
    macro_context: String,
    industry_context: String,
    company_context: String,
    market_pricing: String,
    future_expectation: String,
    expectation_gap: String,
    upside_catalysts: Vec<String>,
    downside_pressures: Vec<String>,
    core_risks: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EquityIntentDraft {
    thesis: String,
    time_horizon: String,
    catalysts: Vec<String>,
    invalidation_conditions: Vec<String>,
}

#[derive(Debug)]
struct FrozenPositionScope {
    instrument_id: Uuid,
    account_id: Option<Uuid>,
    wallet_id: Option<Uuid>,
}

#[derive(Debug)]
struct FrozenIntentScope {
    instrument_ids: HashSet<Uuid>,
    account_id: Option<Uuid>,
    wallet_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NarrativeOutput {
    narrative_key: String,
    market: String,
    name: String,
    narrative_type: String,
    stage: String,
    summary: String,
    confidence: f64,
    capital_flow_inference: Option<String>,
    evidence_keys: Vec<String>,
    instrument_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TransmissionPathOutput {
    path_key: String,
    source_driver: String,
    steps: Vec<String>,
    target_market: String,
    target_instrument_id: Option<String>,
    direction: String,
    confidence: f64,
    evidence_keys: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BriefingOutput {
    as_of: String,
    title: String,
    executive_summary: String,
    data_gaps: Vec<String>,
    items: Vec<BriefingItemOutput>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BriefingItemOutput {
    section: String,
    title: String,
    body: String,
    severity: Option<String>,
    intent_status: Option<String>,
    actionability: Option<String>,
    source_finding_ids: Vec<String>,
}

#[derive(Debug)]
struct DeterministicPositionItem {
    title: String,
    body: String,
}

#[derive(Debug)]
struct FrozenSourceSnapshot {
    as_of: DateTime<Utc>,
    source_type: String,
    payload: Value,
}

#[derive(Debug)]
struct InstrumentLabels {
    symbol: String,
    name: String,
}

#[derive(Debug, Default)]
struct KnownObjects {
    language: ReportLanguage,
    instruments: HashSet<Uuid>,
    instrument_labels: HashMap<Uuid, InstrumentLabels>,
    position_snapshots: HashSet<Uuid>,
    position_snapshot_items: HashSet<Uuid>,
    position_item_markets: HashMap<Uuid, String>,
    position_item_scopes: HashMap<Uuid, FrozenPositionScope>,
    position_instruments: HashMap<Uuid, HashSet<Uuid>>,
    trade_intents: HashSet<Uuid>,
    trade_intent_versions: HashSet<Uuid>,
    trade_intent_version_scopes: HashMap<Uuid, FrozenIntentScope>,
    accounts: HashSet<Uuid>,
    market_narratives: HashSet<Uuid>,
    market_events: HashSet<Uuid>,
    research_findings: HashSet<Uuid>,
    source_snapshots: HashMap<Uuid, FrozenSourceSnapshot>,
}

impl KnownObjects {
    fn validate(&self, object_type: &str, id: Uuid) -> Result<(), String> {
        let known = match object_type {
            "instrument" => self.instruments.contains(&id),
            "position_snapshot" => self.position_snapshots.contains(&id),
            "position_snapshot_item" => self.position_snapshot_items.contains(&id),
            "trade_intent" => self.trade_intents.contains(&id),
            "trade_intent_version" => self.trade_intent_versions.contains(&id),
            "account" => self.accounts.contains(&id),
            "market_narrative" => self.market_narratives.contains(&id),
            "market_event" => self.market_events.contains(&id),
            "research_finding" => self.research_findings.contains(&id),
            "source_snapshot" => self.source_snapshots.contains_key(&id),
            other => return Err(format!("unsupported object type {other}")),
        };
        if known {
            Ok(())
        } else {
            Err(format!(
                "{object_type} object {id} is absent from the frozen input"
            ))
        }
    }

    fn matching_intent_versions(&self, position_item_id: Uuid) -> HashSet<Uuid> {
        let Some(position) = self.position_item_scopes.get(&position_item_id) else {
            return HashSet::new();
        };
        self.trade_intent_version_scopes
            .iter()
            .filter_map(|(version_id, intent)| {
                let instrument_matches = intent.instrument_ids.contains(&position.instrument_id);
                let owner_matches = match (intent.account_id, intent.wallet_id) {
                    (Some(account_id), None) => position.account_id == Some(account_id),
                    (None, Some(wallet_id)) => position.wallet_id == Some(wallet_id),
                    (None, None) => false,
                    (Some(_), Some(_)) => false,
                };
                (instrument_matches && owner_matches).then_some(*version_id)
            })
            .collect()
    }
}

pub fn runner_label(state: &AppState) -> &'static str {
    if state.research.enabled && state.research.preflight().is_ok() {
        "configured"
    } else {
        "not_configured"
    }
}

async fn renew_briefing_lease(
    state: &AppState,
    briefing_id: Uuid,
    lease_token: Uuid,
) -> Result<(), String> {
    let updated = sqlx::query(
        "UPDATE briefing_runs SET heartbeat_at = now() WHERE id = $1 AND status = 'running' AND lease_token = $2",
    )
    .bind(briefing_id)
    .bind(lease_token)
    .execute(&state.pool)
    .await
    .map_err(|error| error.to_string())?;
    if updated.rows_affected() != 1 {
        return Err("briefing lease is no longer owned by this worker".into());
    }
    Ok(())
}

pub async fn execute_briefing(
    state: &AppState,
    briefing_id: Uuid,
    lease_token: Uuid,
) -> Result<(), String> {
    renew_briefing_lease(state, briefing_id, lease_token).await?;
    let (kind, as_of, window_start, frozen_input): (String, DateTime<Utc>, DateTime<Utc>, Value) =
        sqlx::query_as(
            "SELECT kind, as_of, research_window_start, input_snapshot FROM briefing_runs WHERE id = $1",
        )
        .bind(briefing_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"UPDATE research_runs
           SET status = 'failed', validation_status = 'interrupted',
               error = 'interrupted before the worker resumed this task', completed_at = now()
           WHERE briefing_run_id = $1 AND status = 'running'"#,
    )
    .bind(briefing_id)
    .execute(&state.pool)
    .await
    .map_err(|error| error.to_string())?;

    let language = ReportLanguage::from_input(&frozen_input);
    let scoped_positions = active_equity_positions(&frozen_input).len() as i64;
    let active_positions = active_equity_position_count(&frozen_input);
    if scoped_positions != active_positions {
        return finish_research_partial(
            state,
            briefing_id,
            lease_token,
            &kind,
            as_of,
            vec![localized_format!(language,
                "持仓研究输入无效：{} 个股票持仓缺少合法 position_snapshot_item_id",
                "Invalid research input: {} equity holdings lack a valid position_snapshot_item_id",
                active_positions - scoped_positions
            )],
            language.text("股票持仓缺少可审计的快照明细标识，本报告拒绝漏诊。", "Holdings lack auditable snapshot-item identifiers; this report refuses to omit their diagnoses."),
        )
        .await;
    }

    let mut failed_skills = Vec::new();
    for stage in research_stages(&frozen_input) {
        failed_skills.extend(
            execute_research_stage(
                state,
                briefing_id,
                lease_token,
                as_of,
                window_start,
                &frozen_input,
                stage,
            )
            .await?,
        );
    }

    let upstream = load_validated_findings(state, briefing_id)
        .await
        .map_err(|error| error.to_string())?;
    let complete_runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM research_runs WHERE briefing_run_id = $1 AND status = 'complete' AND validation_status = 'valid'",
    )
    .bind(briefing_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| error.to_string())?;

    if complete_runs == 0 {
        return finish_research_partial(
            state,
            briefing_id,
            lease_token,
            &kind,
            as_of,
            failed_skills,
            language.text(
                "没有通过验证的 Research Run，未调用报告合成 Skill。",
                "No validated Research Run is available; report synthesis was not invoked.",
            ),
        )
        .await;
    }

    let compose_skill = "compose-portfolio-briefing";
    let validated_events = load_validated_events(state, briefing_id)
        .await
        .map_err(|error| error.to_string())?;
    let validated_narratives = load_validated_narratives(state, briefing_id)
        .await
        .map_err(|error| error.to_string())?;
    let validated_transmission_paths = load_validated_transmission_paths(state, briefing_id)
        .await
        .map_err(|error| error.to_string())?;
    let mut composition_frozen_input = frozen_input.clone();
    remove_composition_raw_sources(&mut composition_frozen_input);
    let compose_input = json!({
        "briefing_id": briefing_id,
        "kind": kind,
        "as_of": as_of,
        "research_window_start": window_start,
        "frozen_portfolio": composition_frozen_input,
        "validated_findings": upstream,
        "validated_events": validated_events,
        "validated_narratives": validated_narratives,
        "validated_transmission_paths": validated_transmission_paths,
        "failed_research_tasks": failed_skills,
    });
    renew_briefing_lease(state, briefing_id, lease_token).await?;
    match execute_composition(
        state,
        briefing_id,
        lease_token,
        as_of,
        compose_skill,
        compose_input,
        &failed_skills,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            failed_skills.push(format!("{compose_skill}: {error}"));
            finish_research_partial(
                state,
                briefing_id,
                lease_token,
                &kind,
                as_of,
                failed_skills,
                language.text(
                    "专项研究已保留，但报告合成未通过验证。",
                    "Research results were retained, but report synthesis did not pass validation.",
                ),
            )
            .await
        }
    }
}

async fn build_research_input(
    state: &AppState,
    briefing_id: Uuid,
    as_of: DateTime<Utc>,
    window_start: DateTime<Utc>,
    frozen_input: &Value,
) -> Result<Value, String> {
    let upstream = load_validated_findings(state, briefing_id)
        .await
        .map_err(|error| error.to_string())?;
    let upstream_events = load_validated_events(state, briefing_id)
        .await
        .map_err(|error| error.to_string())?;
    let upstream_narratives = load_validated_narratives(state, briefing_id)
        .await
        .map_err(|error| error.to_string())?;
    let upstream_transmission_paths = load_validated_transmission_paths(state, briefing_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "briefing_id": briefing_id,
        "as_of": as_of,
        "research_window_start": window_start,
        "frozen_portfolio": frozen_input,
        "validated_upstream_findings": upstream,
        "validated_upstream_events": upstream_events,
        "validated_upstream_narratives": upstream_narratives,
        "validated_upstream_transmission_paths": upstream_transmission_paths,
    }))
}

fn scope_research_input(input: &mut Value, task: ResearchTask) {
    if task.run_type == "macro_context" {
        remove_raw_generated_snapshots(input);
        clear_array(input, "trade_intent_versions");
        return;
    }
    if task.run_type == "equity_market_context" {
        if let Some(positions) = input
            .get_mut("frozen_portfolio")
            .and_then(|portfolio| portfolio.get_mut("positions"))
            .and_then(Value::as_array_mut)
        {
            positions.retain(|position| {
                position.get("market").and_then(Value::as_str) == Some(task.market)
            });
        }
        remove_raw_generated_snapshots(input);
        clear_array(input, "trade_intent_versions");
        return;
    }
    if task.run_type == "portfolio_construction" {
        clear_array(input, "source_snapshots");
        return;
    }
    let Some(position_item_id) = task.position_snapshot_item_id else {
        return;
    };
    let position_id = position_item_id.to_string();
    let target_position = input
        .get("frozen_portfolio")
        .and_then(|portfolio| portfolio.get("positions"))
        .and_then(Value::as_array)
        .and_then(|positions| {
            positions.iter().find(|position| {
                position
                    .get("position_snapshot_item_id")
                    .and_then(Value::as_str)
                    == Some(position_id.as_str())
            })
        });
    let target_instrument_id = target_position
        .and_then(|position| position.get("instrument_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let target_account_id = target_position
        .and_then(|position| position.get("account_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let target_wallet_id = target_position
        .and_then(|position| position.get("wallet_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let Some(portfolio) = input
        .get_mut("frozen_portfolio")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if let Some(positions) = portfolio.get_mut("positions").and_then(Value::as_array_mut) {
        positions.retain(|position| {
            position
                .get("position_snapshot_item_id")
                .and_then(Value::as_str)
                == Some(position_id.as_str())
        });
    }
    if let Some(instrument_id) = target_instrument_id.as_deref() {
        if let Some(intents) = portfolio
            .get_mut("trade_intent_versions")
            .and_then(Value::as_array_mut)
        {
            intents.retain(|intent| {
                let instrument_matches = intent
                    .get("instrument_ids")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(instrument_id)));
                let owner_matches =
                    match (target_account_id.as_deref(), target_wallet_id.as_deref()) {
                        (Some(account_id), None) => {
                            intent.get("account_id").and_then(Value::as_str) == Some(account_id)
                        }
                        (None, Some(wallet_id)) => {
                            intent.get("wallet_id").and_then(Value::as_str) == Some(wallet_id)
                        }
                        _ => false,
                    };
                instrument_matches && owner_matches
            });
        }
        if let Some(snapshots) = portfolio
            .get_mut("source_snapshots")
            .and_then(Value::as_array_mut)
        {
            let latest_equity_news_id = snapshots
                .iter()
                .filter(|snapshot| {
                    snapshot.get("source_type").and_then(Value::as_str) == Some("equity_news")
                        && snapshot
                            .get("payload")
                            .and_then(|payload| payload.get("instrument_id"))
                            .and_then(Value::as_str)
                            == Some(instrument_id)
                })
                .max_by_key(|snapshot| snapshot.get("as_of").and_then(Value::as_str))
                .and_then(|snapshot| snapshot.get("source_snapshot_id"))
                .cloned();
            snapshots.retain(|snapshot| {
                match snapshot.get("source_type").and_then(Value::as_str) {
                    Some("equity_news") => {
                        snapshot.get("source_snapshot_id") == latest_equity_news_id.as_ref()
                    }
                    Some("market_quote" | "research_evidence") => false,
                    _ => true,
                }
            });
            compact_equity_news_snapshots(snapshots);
        }
    }
}

fn clear_array(input: &mut Value, field: &str) {
    if let Some(values) = input
        .get_mut("frozen_portfolio")
        .and_then(|portfolio| portfolio.get_mut(field))
        .and_then(Value::as_array_mut)
    {
        values.clear();
    }
}

fn remove_composition_raw_sources(frozen_input: &mut Value) {
    if let Some(source_snapshots) = frozen_input
        .get_mut("source_snapshots")
        .and_then(Value::as_array_mut)
    {
        source_snapshots.clear();
    }
}

fn remove_raw_generated_snapshots(input: &mut Value) {
    if let Some(snapshots) = input
        .get_mut("frozen_portfolio")
        .and_then(|portfolio| portfolio.get_mut("source_snapshots"))
        .and_then(Value::as_array_mut)
    {
        snapshots.retain(|snapshot| {
            !matches!(
                snapshot.get("source_type").and_then(Value::as_str),
                Some("equity_news" | "market_quote" | "research_evidence")
            )
        });
        compact_equity_news_snapshots(snapshots);
    }
}

fn compact_equity_news_snapshots(snapshots: &mut [Value]) {
    for snapshot in snapshots {
        if snapshot.get("source_type").and_then(Value::as_str) == Some("equity_news")
            && let Some(payload) = snapshot.get_mut("payload").and_then(Value::as_object_mut)
            && let Some(feeds) = payload
                .remove("feeds")
                .and_then(|feeds| feeds.as_array().cloned())
        {
            let feed_status = feeds
                .iter()
                .filter_map(Value::as_object)
                .map(|feed| {
                    json!({
                        "provider": feed.get("provider").cloned().unwrap_or(Value::Null),
                        "url": feed.get("url").cloned().unwrap_or(Value::Null),
                        "error": feed.get("error").cloned().unwrap_or(Value::Null),
                        "item_count": feed.get("items").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                    })
                })
                .collect::<Vec<_>>();
            payload.insert("feed_status".into(), json!(feed_status));
        }
    }
}

pub(crate) fn compact_report_source_snapshots(source_snapshots: &mut Value) {
    let Some(snapshots) = source_snapshots.as_array_mut() else {
        return;
    };
    let mut latest_news = HashMap::<String, (String, String)>::new();
    for snapshot in snapshots.iter() {
        if snapshot.get("source_type").and_then(Value::as_str) != Some("equity_news") {
            continue;
        }
        let Some(instrument_id) = snapshot
            .get("payload")
            .and_then(|payload| payload.get("instrument_id"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Some(snapshot_id) = snapshot.get("source_snapshot_id").and_then(Value::as_str) else {
            continue;
        };
        let as_of = snapshot
            .get("as_of")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let candidate = (as_of.to_string(), snapshot_id.to_string());
        let entry = latest_news
            .entry(instrument_id.to_string())
            .or_insert_with(|| candidate.clone());
        if candidate > *entry {
            *entry = candidate;
        }
    }
    snapshots.retain(
        |snapshot| match snapshot.get("source_type").and_then(Value::as_str) {
            Some("equity_news") => {
                let instrument_id = snapshot
                    .get("payload")
                    .and_then(|payload| payload.get("instrument_id"))
                    .and_then(Value::as_str);
                let snapshot_id = snapshot.get("source_snapshot_id").and_then(Value::as_str);
                instrument_id
                    .and_then(|instrument_id| latest_news.get(instrument_id))
                    .is_some_and(|(_, latest_id)| Some(latest_id.as_str()) == snapshot_id)
            }
            Some("market_quote" | "research_evidence") => false,
            _ => true,
        },
    );
    compact_equity_news_snapshots(snapshots);
}

fn bound_research_prompt_input(
    input: &mut Value,
    task: ResearchTask,
    as_of: DateTime<Utc>,
    window_start: DateTime<Utc>,
) -> Result<(), String> {
    loop {
        let prompt_bytes = research_prompt(task, as_of, window_start, input).len();
        if prompt_bytes <= MAX_CODEX_PROMPT_BYTES {
            return Ok(());
        }
        let snapshots = input
            .get_mut("frozen_portfolio")
            .and_then(|portfolio| portfolio.get_mut("source_snapshots"))
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                format!(
                    "research prompt is {prompt_bytes} bytes after deterministic scoping and has no optional source payloads to compact"
                )
            })?;
        let candidate = snapshots
            .iter()
            .enumerate()
            .filter(|(_, snapshot)| {
                snapshot
                    .get("payload")
                    .and_then(|payload| payload.get("truncated_for_model_input"))
                    .and_then(Value::as_bool)
                    != Some(true)
            })
            .map(|(index, snapshot)| {
                let payload_bytes = snapshot
                    .get("payload")
                    .and_then(|payload| serde_json::to_vec(payload).ok())
                    .map_or(0, |payload| payload.len());
                let stable_id = snapshot
                    .get("source_snapshot_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                (payload_bytes, stable_id, index)
            })
            .max();
        let Some((payload_bytes, _, index)) = candidate else {
            return Err(format!(
                "research prompt remains {prompt_bytes} bytes after all optional source payloads were compacted"
            ));
        };
        let snapshot = &mut snapshots[index];
        let original = snapshot.get("payload").cloned().unwrap_or(Value::Null);
        let mut compact = serde_json::Map::new();
        for field in ["instrument_id", "market", "symbol", "name", "feed_status"] {
            if let Some(value) = original.get(field) {
                compact.insert(field.into(), value.clone());
            }
        }
        compact.insert("truncated_for_model_input".into(), Value::Bool(true));
        compact.insert("original_payload_bytes".into(), json!(payload_bytes));
        compact.insert(
            "data_gap".into(),
            Value::String(
                "原始来源正文超过模型输入预算，已保留来源元数据；需通过实时搜索重新验证。".into(),
            ),
        );
        snapshot["payload"] = Value::Object(compact);
    }
}

async fn reusable_task_run(
    state: &AppState,
    briefing_id: Uuid,
    task: &ResearchTask,
    current_skill_version: &str,
) -> Result<Option<(String, Option<String>)>, String> {
    let rows: Vec<(String, Option<String>, Option<String>, String)> = sqlx::query_as(
        r#"SELECT status, validation_status, error, skill_version
           FROM research_runs
           WHERE briefing_run_id = $1 AND market = $2 AND run_type = $3
             AND COALESCE(input_snapshot->'task_scope'->>'position_snapshot_item_id', '') = $4
           ORDER BY created_at DESC"#,
    )
    .bind(briefing_id)
    .bind(task.market)
    .bind(task.run_type)
    .bind(
        task.position_snapshot_item_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .find(|(status, validation_status, _, skill_version)| {
            is_reusable_task_version(
                status,
                validation_status.as_deref(),
                skill_version,
                current_skill_version,
            )
        })
        .map(|(status, _, error, _)| (status, error)))
}

#[allow(clippy::too_many_arguments)]
async fn execute_research_stage(
    state: &AppState,
    briefing_id: Uuid,
    lease_token: Uuid,
    as_of: DateTime<Utc>,
    window_start: DateTime<Utc>,
    frozen_input: &Value,
    tasks: Vec<ResearchTask>,
) -> Result<Vec<String>, String> {
    renew_briefing_lease(state, briefing_id, lease_token).await?;
    let mut failed_skills = Vec::new();
    let mut pending = Vec::new();
    for task in tasks {
        let current_skill_version = hash_skill(&state.research.project_root, task.skill_name)?;
        match reusable_task_run(state, briefing_id, &task, &current_skill_version).await? {
            Some((status, error)) if status == "failed" => failed_skills.push(format!(
                "{}: {}",
                task.skill_name,
                error.unwrap_or_else(|| "research task failed".into())
            )),
            Some(_) => {}
            None => pending.push(task),
        }
    }
    if pending.is_empty() {
        return Ok(failed_skills);
    }

    let run_input =
        build_research_input(state, briefing_id, as_of, window_start, frozen_input).await?;
    let mut running = JoinSet::new();
    for task in pending {
        let owned_state = state.clone();
        let mut task_input = run_input.clone();
        scope_research_input(&mut task_input, task);
        task_input["task_scope"] = json!({
            "market": task.market,
            "run_type": task.run_type,
            "position_snapshot_item_id": task.position_snapshot_item_id,
        });
        let spawned_task = task;
        running.spawn(async move {
            let result = execute_research_task(
                &owned_state,
                briefing_id,
                lease_token,
                as_of,
                window_start,
                task_input,
                spawned_task,
            )
            .await;
            (spawned_task, result)
        });
    }
    while let Some(joined) = running.join_next().await {
        let (task, result) = joined.map_err(|error| format!("research stage failed: {error}"))?;
        if let Err(error) = result {
            tracing::warn!(%briefing_id, skill = task.skill_name, %error, "research task failed");
            failed_skills.push(format!("{}: {}", task.skill_name, error));
        }
    }
    Ok(failed_skills)
}

async fn execute_research_task(
    state: &AppState,
    briefing_id: Uuid,
    lease_token: Uuid,
    as_of: DateTime<Utc>,
    window_start: DateTime<Utc>,
    mut input_snapshot: Value,
    task: ResearchTask,
) -> Result<Vec<Uuid>, String> {
    let skill_version = hash_skill(&state.research.project_root, task.skill_name)?;
    let standards_version = hash_skill(
        &state.research.project_root,
        "apply-investment-research-standards",
    )?;
    let supporting_skills = json!({
        "apply-investment-research-standards": standards_version,
    });
    let (next_input_snapshot, preflight) = tokio::task::spawn_blocking(move || {
        let preflight = bound_research_prompt_input(&mut input_snapshot, task, as_of, window_start);
        (input_snapshot, preflight)
    })
    .await
    .map_err(|error| format!("research input preflight task failed: {error}"))?;
    input_snapshot = next_input_snapshot;
    let run_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO research_runs
           (briefing_run_id, market, run_type, status, as_of, research_window_start,
            input_snapshot, skill_name, skill_version, supporting_skills, model,
            schema_version, started_at)
           VALUES ($1, $2, $3, 'running', $4, $5, $6, $7, $8, $9, $10, $11, now())
           RETURNING id"#,
    )
    .bind(briefing_id)
    .bind(task.market)
    .bind(task.run_type)
    .bind(as_of)
    .bind(window_start)
    .bind(&input_snapshot)
    .bind(task.skill_name)
    .bind(&skill_version)
    .bind(&supporting_skills)
    .bind(&state.research.model)
    .bind(RESEARCH_SCHEMA_VERSION)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| error.to_string())?;

    if let Err(error) = preflight {
        sqlx::query(
            "UPDATE research_runs SET status = 'failed', validation_status = 'invalid', error = $2, completed_at = now() WHERE id = $1",
        )
        .bind(run_id)
        .bind(&error)
        .execute(&state.pool)
        .await
        .map_err(|database_error| database_error.to_string())?;
        return Err(error);
    }

    let prompt = research_prompt(task, as_of, window_start, &input_snapshot);
    let schema = state
        .research
        .project_root
        .join("schemas/research-output-v1.json");
    let known_objects = collect_known_objects(&input_snapshot)?;
    let mut last_error = String::new();
    let mut attempt_prompt = prompt.clone();
    for attempt in 1..=state.research.max_attempts {
        renew_briefing_lease(state, briefing_id, lease_token).await?;
        match run_codex(state, &schema, &attempt_prompt, true).await {
            Ok(raw) => {
                renew_briefing_lease(state, briefing_id, lease_token).await?;
                save_research_raw_output(state, run_id, &raw).await?;
                match parse_and_validate_research(&raw, task, as_of, window_start, &known_objects) {
                    Ok(output) => match persist_research_output(
                        state,
                        briefing_id,
                        lease_token,
                        run_id,
                        task,
                        as_of,
                        &skill_version,
                        output,
                    )
                    .await
                    {
                        Ok(ids) => return Ok(ids),
                        Err(error) => {
                            last_error = format!("attempt {attempt} persistence failed: {error}")
                        }
                    },
                    Err(error) => {
                        last_error = format!("attempt {attempt} validation failed: {error}");
                        attempt_prompt = validation_retry_prompt(&prompt, &error);
                    }
                }
            }
            Err(error) => last_error = format!("attempt {attempt} execution failed: {error}"),
        }
    }
    sqlx::query(
        "UPDATE research_runs SET status = 'failed', validation_status = 'invalid', error = $2, completed_at = now() WHERE id = $1",
    )
    .bind(run_id)
    .bind(&last_error)
    .execute(&state.pool)
    .await
    .map_err(|error| error.to_string())?;
    Err(last_error)
}

async fn save_research_raw_output(state: &AppState, run_id: Uuid, raw: &str) -> Result<(), String> {
    let audit_value = raw_audit_value(raw);
    sqlx::query("UPDATE research_runs SET raw_output = $2 WHERE id = $1")
        .bind(run_id)
        .bind(audit_value)
        .execute(&state.pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn research_prompt(
    task: ResearchTask,
    as_of: DateTime<Utc>,
    window_start: DateTime<Utc>,
    input: &Value,
) -> String {
    let equity_news_policy = if task.run_type == "equity_position_diagnosis" {
        let exact_scope = task
            .position_snapshot_item_id
            .map(|id| {
                format!(
                    "- Diagnose exactly the frozen position item `{id}` and no other holding.\n"
                )
            })
            .unwrap_or_default();
        format!(
            r#"
Equity news lookback policy:
{exact_scope}
- `{window_start}` is only the boundary for what counts as NEW in this briefing. It is not a search cutoff.
- For every scoped holding, search backward adaptively until you have the latest material company baseline: the most recent earnings/results release, current guidance or management outlook, and material issuer/exchange/regulatory events. Start with roughly 90 days; extend to 180 days, then to the latest fiscal or annual result when coverage is sparse. Never cross the hard future cutoff `{as_of}`. An empty incremental window is not an empty diagnosis: use the latest verified baseline and its publication date.
- Process holdings one by one. Use explicit symbol + issuer-name queries and primary-source queries for the relevant exchange/regulator and issuer investor-relations site. Target at least two distinct company-specific sources when available; do not substitute shared macro or index news for company evidence.
- Separate evidence published on or after `{window_start}` as this-run updates from older evidence as recent baseline. When there is no incremental update, make that recent baseline the primary company-news context and explicitly incorporate it into company_context, future_expectation, expectation_gap, catalysts, pressures, and risks. If no company-specific source can be verified even after reaching the latest fiscal or annual result, state that exact gap and the source classes searched; do not fill the slot with generic market news.
- For an ETF or leveraged product, collect issuer product documents plus recent underlying-index/theme evidence, including leverage decay, tracking, NAV/premium-discount, and rebalance risks where relevant.
"#,
            window_start = window_start.to_rfc3339(),
            as_of = as_of.to_rfc3339(),
            exact_scope = exact_scope,
        )
    } else {
        String::new()
    };
    format!(
        r#"You are executing one Alpha Echo research run.
Read and follow `skills/{skill}/SKILL.md` and `skills/apply-investment-research-standards/SKILL.md` completely.
This run is scoped to market `{market}` and run type `{run_type}`. Ignore frozen positions outside that market unless the Skill performs portfolio aggregation.
You may use live web search, but you must not write files, modify portfolio facts, access PostgreSQL, or execute trades.
Use only information published no later than {as_of}. For live/frozen evidence set `observed_at` to exactly {as_of}; for `captured_source`, copy the referenced source snapshot's `as_of`.
Every non-null `published_at` must be an RFC3339 date-time or YYYY-MM-DD publication date. Never use a quarter, month, or other partial date.
Every finding window_start must equal {window_start}; every window_end and top-level as_of must equal {as_of}.
Return JSON only and conform exactly to the supplied output schema. `skill_name` must be `{skill}`.
Write every human-readable title, summary, inference, condition, action, risk, catalyst, pressure, and data-gap string in {language}. Keep tickers, official names, URLs, keys, object types, and IDs unchanged.
Evidence uses run-local unique string keys. Every finding must cite at least one evidence key.
Classify every evidence row with `coverage_scope`: `position` for frozen holding/Intent facts, `company` for issuer/competitor/supply-chain company evidence, `product` for ETF/ETP provider and product-mechanics evidence, `industry`, `market`, or `macro`. For `company`/`product`, set `related_instrument_id` to the exact scoped holding instrument. Material `industry` policy or supply-chain evidence may also bind the exact scoped instrument only with `relationship_basis="source_names_instrument"` and a title/excerpt that names the symbol/company, or `relationship_basis="direct_transmission"` and a {language} `relationship_explanation` stating the direct causal chain. Every `direct_transmission` evidence row must be used by a finding-linked transmission path: the finding cites the evidence and path key, while that path cites the same evidence and targets the exact related instrument. It remains industry evidence and never satisfies company-news coverage. All non-industry evidence sets both relationship fields to null; market/macro/position evidence also sets the instrument ID to null. Frozen portfolio facts cannot be company/product/industry evidence. Every equity diagnosis must cite company/product evidence bound to its exact instrument, or explicitly state in `data_gaps` that company-level/product-level evidence was not found and name the issuer/IR, exchange, regulator, or product-provider source classes searched.
For a frozen portfolio fact, use source_type `frozen_input`, set `frozen_object_type` and `frozen_object_id` to the exact typed object in frozen_portfolio, and set source_snapshot_id, URL, and published_at to null. A row under `positions` is not an object type named `position`: cite its exact holding as `position_snapshot_item` + `position_snapshot_item_id`, its security identity as `instrument` + `instrument_id`, or its enclosing snapshot as `position_snapshot` + `snapshot_id`. Only for a legacy frozen row that has no `position_snapshot_item_id`, fall back to `position_snapshot` + `snapshot_id` and record that traceability gap.
For a supplied captured source, use source_type `captured_source`, set `source_snapshot_id` to its exact frozen source_snapshot_id, and set frozen_object_type/frozen_object_id to null. Its observed_at cannot be later than that snapshot's as_of. For an `equity_news` snapshot, copy the exact item URL and published_at from its payload; the {language} evidence title may summarize the original title, but provenance fields may not be invented.
For any other source, published_at and a non-empty URL are mandatory and published_at cannot be later than as_of. Set all provenance ID fields to null.
Market events after as_of are allowed only for an upcoming calendar item with `is_scheduled=true`; its supporting evidence must still satisfy the publication cutoff.
Every event, narrative, and transmission path must also be covered by at least one finding through its related-key arrays, so the final report can cite a validated Finding. For a scheduled calendar item use a finding scoped to `market` with a null object ID.
Affected object references must use the exact supported type and ID pair from the frozen input. Supported ID-bearing types are `instrument`, `position_snapshot`, `position_snapshot_item`, `trade_intent`, `trade_intent_version`, `account`, `market_narrative`, `market_event`, `research_finding`, and `source_snapshot`. Use type `portfolio` or `market` with a null ID for aggregate findings. Conditional actions require both non-empty confirmation and invalidation conditions.
Only `equity_position_diagnosis` findings may populate `analysis`; every other finding must set it to null.
For an equity diagnosis, set `analysis.intent_assessment` to `matched` only when the exact account/instrument has an active frozen Trade Intent version; cite and affect that version and set `analysis.intent_draft` to null. If it is `missing`, affect the position item, set impact direction/strength and conditional action to null, expose the missing Intent in data_gaps, and populate `analysis.intent_draft` in {language} as an evidence-based proposal for the user to review. The draft is not the user's existing thesis, is not active, and cannot justify a trade until the user confirms it.
Do not force an opportunity. Do not infer precise net flows from incomplete data. Conditional actions require confirmation and invalidation conditions.
{equity_news_policy}

Frozen input and validated upstream research:
{input}"#,
        skill = task.skill_name,
        market = task.market,
        run_type = task.run_type,
        as_of = as_of.to_rfc3339(),
        window_start = window_start.to_rfc3339(),
        equity_news_policy = equity_news_policy,
        language = ReportLanguage::from_input(input).name(),
        input = serde_json::to_string(input).unwrap_or_else(|_| "{}".into()),
    )
}

fn captured_evidence_matches_snapshot(
    evidence: &EvidenceOutput,
    snapshot: &FrozenSourceSnapshot,
) -> bool {
    let payload_contains_url = |url: &str| {
        fn contains(value: &Value, url: &str) -> bool {
            match value {
                Value::Object(object) => {
                    object.get("url").and_then(Value::as_str) == Some(url)
                        || object.values().any(|value| contains(value, url))
                }
                Value::Array(values) => values.iter().any(|value| contains(value, url)),
                _ => false,
            }
        }
        contains(&snapshot.payload, url)
    };
    if evidence
        .url
        .as_deref()
        .is_some_and(|url| !payload_contains_url(url))
    {
        return false;
    }
    if snapshot.source_type != "equity_news" {
        return true;
    }
    let (Some(instrument_id), Some(url), Some(published_at)) = (
        evidence.related_instrument_id.as_deref(),
        evidence.url.as_deref(),
        evidence.published_at.as_deref(),
    ) else {
        return false;
    };
    if snapshot
        .payload
        .get("instrument_id")
        .and_then(Value::as_str)
        != Some(instrument_id)
    {
        return false;
    }
    snapshot
        .payload
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|item| {
            item.get("url").and_then(Value::as_str) == Some(url)
                && item
                    .get("published_at")
                    .and_then(Value::as_str)
                    .is_some_and(|value| publication_times_match(published_at, value))
        })
}

fn frozen_news_source_names_instrument(
    evidence: &EvidenceOutput,
    snapshot: &FrozenSourceSnapshot,
    labels: Option<&InstrumentLabels>,
) -> bool {
    if snapshot.source_type != "equity_news" {
        return false;
    }
    let (Some(url), Some(labels)) = (evidence.url.as_deref(), labels) else {
        return false;
    };
    snapshot
        .payload
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("url").and_then(Value::as_str) == Some(url))
        .filter_map(|item| item.get("title").and_then(Value::as_str))
        .any(|title| {
            let symbol_matches = title
                .split(|character: char| {
                    !character.is_ascii_alphanumeric() && character != '.' && character != '-'
                })
                .any(|token| token.eq_ignore_ascii_case(labels.symbol.trim()));
            let normalized_title = title.to_lowercase();
            let normalized_name = labels.name.trim().to_lowercase();
            let name_matches =
                normalized_name.chars().count() >= 2 && normalized_title.contains(&normalized_name);
            symbol_matches || name_matches
        })
}

fn parse_and_validate_research(
    raw: &str,
    task: ResearchTask,
    as_of: DateTime<Utc>,
    window_start: DateTime<Utc>,
    known: &KnownObjects,
) -> Result<ResearchOutput, String> {
    let output: ResearchOutput =
        serde_json::from_str(raw).map_err(|error| format!("invalid JSON: {error}"))?;
    if parse_time(&output.as_of)? != as_of {
        return Err("top-level as_of does not match the frozen cutoff".into());
    }
    if output.skill_name != task.skill_name {
        return Err("skill_name does not match the invoked Skill".into());
    }
    let allowed_markets = ["macro", "a_share", "hk", "us", "cross", "portfolio"];
    let mut evidence_keys = HashSet::new();
    let mut event_keys = HashSet::new();
    let scoped_instrument_id = task
        .position_snapshot_item_id
        .and_then(|id| known.position_item_scopes.get(&id))
        .map(|scope| scope.instrument_id);
    for evidence in &output.evidence {
        if evidence.key.trim().is_empty()
            || evidence.source_type.trim().is_empty()
            || !matches!(
                evidence.coverage_scope.as_str(),
                "position" | "company" | "product" | "industry" | "market" | "macro"
            )
            || evidence.title.trim().is_empty()
            || !evidence_keys.insert(evidence.key.as_str())
        {
            return Err("evidence keys must be non-empty and unique".into());
        }
        if matches!(
            evidence.coverage_scope.as_str(),
            "company" | "product" | "industry"
        ) {
            if evidence.source_type == "frozen_input" {
                return Err(
                    "company/product/industry evidence cannot be a frozen portfolio fact".into(),
                );
            }
            // Unbound industry evidence describes shared sector conditions;
            // it is not company coverage or proof of a stock-specific link.
            if evidence.coverage_scope == "industry" && evidence.related_instrument_id.is_none() {
                if evidence.relationship_basis.is_some()
                    || evidence.relationship_explanation.is_some()
                {
                    return Err(format!(
                        "unbound industry evidence {} cannot declare an instrument relationship",
                        evidence.key
                    ));
                }
            } else {
                let instrument_id = parse_required_uuid(
                    evidence.related_instrument_id.as_deref(),
                    "related_instrument_id",
                )
                .map_err(|error| format!("evidence {}: {error}", evidence.key))?;
                known.validate("instrument", instrument_id)?;
                if scoped_instrument_id.is_some_and(|scoped| scoped != instrument_id) {
                    return Err(format!(
                        "evidence {} is not bound to the exact scoped holding instrument",
                        evidence.key
                    ));
                }
                if evidence.coverage_scope == "industry" {
                    match evidence.relationship_basis.as_deref() {
                        Some("source_names_instrument") => {
                            let names_instrument = evidence
                                .source_snapshot_id
                                .as_deref()
                                .and_then(|id| Uuid::parse_str(id).ok())
                                .and_then(|id| known.source_snapshots.get(&id))
                                .is_some_and(|snapshot| {
                                    frozen_news_source_names_instrument(
                                        evidence,
                                        snapshot,
                                        known.instrument_labels.get(&instrument_id),
                                    )
                                });
                            if !names_instrument {
                                return Err(format!(
                                    "industry evidence {} is not backed by frozen source text naming its instrument",
                                    evidence.key
                                ));
                            }
                        }
                        Some("direct_transmission")
                            if evidence.relationship_explanation.as_deref().is_some_and(
                                |value| {
                                    value.chars().count() >= 8
                                        && is_report_text(value, known.language)
                                },
                            ) => {}
                        _ => {
                            return Err(format!(
                                "industry evidence {} has no validated relationship basis",
                                evidence.key
                            ));
                        }
                    }
                } else if evidence.relationship_basis.is_some()
                    || evidence.relationship_explanation.is_some()
                {
                    return Err(format!(
                        "non-industry evidence {} cannot declare an industry relationship",
                        evidence.key
                    ));
                }
            }
        } else if evidence.related_instrument_id.is_some() {
            return Err(
                "only company/product/industry evidence may bind a related instrument".into(),
            );
        }
        let observed = parse_time(&evidence.observed_at)?;
        if observed > as_of {
            return Err(format!(
                "evidence {} violates the observed-at cutoff",
                evidence.key
            ));
        }
        match evidence.source_type.as_str() {
            "frozen_input" => {
                if evidence.url.is_some()
                    || evidence.published_at.is_some()
                    || evidence.source_snapshot_id.is_some()
                {
                    return Err(format!(
                        "frozen evidence {} has incompatible web or snapshot provenance",
                        evidence.key
                    ));
                }
                validate_typed_reference(
                    evidence.frozen_object_type.as_deref(),
                    evidence.frozen_object_id.as_deref(),
                    known,
                )?;
            }
            "captured_source" => {
                if evidence.frozen_object_type.is_some() || evidence.frozen_object_id.is_some() {
                    return Err(format!(
                        "captured source {} cannot reference a frozen object",
                        evidence.key
                    ));
                }
                let snapshot_id = parse_required_uuid(
                    evidence.source_snapshot_id.as_deref(),
                    "source_snapshot_id",
                )?;
                let snapshot = known.source_snapshots.get(&snapshot_id).ok_or_else(|| {
                    format!("source snapshot {snapshot_id} is absent from the frozen input")
                })?;
                if observed > snapshot.as_of {
                    return Err(format!(
                        "captured source {} was observed after its frozen snapshot cutoff",
                        evidence.key
                    ));
                }
                if let Some(published_at) = &evidence.published_at
                    && parse_publication_time(published_at)? > as_of
                {
                    return Err(format!(
                        "evidence {} violates the publication cutoff",
                        evidence.key
                    ));
                }
                if !captured_evidence_matches_snapshot(evidence, snapshot) {
                    return Err(format!(
                        "captured source {} is not present in its frozen snapshot payload",
                        evidence.key
                    ));
                }
            }
            _ => {
                if evidence.source_snapshot_id.is_some()
                    || evidence.frozen_object_type.is_some()
                    || evidence.frozen_object_id.is_some()
                    || evidence
                        .url
                        .as_deref()
                        .is_none_or(|value| value.trim().is_empty())
                {
                    return Err(format!(
                        "external evidence {} has incomplete or incompatible provenance",
                        evidence.key
                    ));
                }
                let published =
                    parse_publication_time(evidence.published_at.as_deref().ok_or_else(|| {
                        format!("external evidence {} has no publication time", evidence.key)
                    })?)?;
                if published > as_of {
                    return Err(format!(
                        "evidence {} violates the publication cutoff",
                        evidence.key
                    ));
                }
            }
        }
    }
    for event in &output.events {
        if event.event_key.trim().is_empty()
            || event.event_type.trim().is_empty()
            || event.headline.trim().is_empty()
            || event.facts.is_empty()
            || event.facts.iter().any(|fact| fact.trim().is_empty())
            || !allowed_markets.contains(&event.market.as_str())
            || !event_keys.insert(event.event_key.as_str())
            || (!event.is_scheduled && parse_time(&event.occurred_at)? > as_of)
        {
            return Err("market event is missing an identity or violates cutoff".into());
        }
        if !is_report_text(&event.headline, known.language)
            || event
                .facts
                .iter()
                .any(|fact| !is_report_text(fact, known.language))
        {
            return Err(
                "market event headline and facts must be text in the report language".into(),
            );
        }
        validate_nonempty_evidence_keys(&event.evidence_keys, &evidence_keys)?;
    }
    let narrative_keys = output
        .narratives
        .iter()
        .map(|narrative| narrative.narrative_key.as_str())
        .collect::<HashSet<_>>();
    if narrative_keys.len() != output.narratives.len()
        || narrative_keys.iter().any(|key| key.trim().is_empty())
    {
        return Err("narrative keys must be non-empty and unique".into());
    }
    let path_keys = output
        .transmission_paths
        .iter()
        .map(|path| path.path_key.as_str())
        .collect::<HashSet<_>>();
    if path_keys.len() != output.transmission_paths.len()
        || path_keys.iter().any(|key| key.trim().is_empty())
    {
        return Err("transmission path keys must be non-empty and unique".into());
    }
    let expected_position_items = if task.run_type == "equity_position_diagnosis" {
        if let Some(position_snapshot_item_id) = task.position_snapshot_item_id {
            HashSet::from([position_snapshot_item_id])
        } else {
            known
                .position_item_markets
                .iter()
                .filter_map(|(id, market)| (market == task.market).then_some(*id))
                .collect::<HashSet<_>>()
        }
    } else {
        HashSet::new()
    };
    let mut diagnosed_position_items = HashSet::new();
    for finding in &output.findings {
        if finding.finding_type.trim().is_empty()
            || !allowed_markets.contains(&finding.market.as_str())
        {
            return Err(format!("invalid finding market {}", finding.market));
        }
        if parse_time(&finding.window_start)? != window_start
            || parse_time(&finding.window_end)? != as_of
        {
            return Err("finding window does not match the frozen research window".into());
        }
        if finding.fact_summary.trim().is_empty()
            || !(0.0..=1.0).contains(&finding.confidence)
            || finding.impact_direction.as_deref().is_some_and(|value| {
                !matches!(
                    value,
                    "strengthened" | "weakened" | "invalidated" | "unchanged"
                )
            })
            || finding
                .impact_strength
                .is_some_and(|value| !(0..=5).contains(&value))
        {
            return Err("finding confidence or impact strength is outside its range".into());
        }
        if !is_report_text(&finding.fact_summary, known.language)
            || finding.inference.as_deref().is_some_and(|value| {
                value.trim().is_empty() || !is_report_text(value, known.language)
            })
            || finding.conditional_action.as_deref().is_some_and(|value| {
                value.trim().is_empty() || !is_report_text(value, known.language)
            })
            || finding
                .contrary_evidence
                .iter()
                .chain(&finding.confirmation_conditions)
                .chain(&finding.invalidation_conditions)
                .chain(&finding.data_gaps)
                .any(|value| value.trim().is_empty() || !is_report_text(value, known.language))
        {
            return Err("all displayed finding fields must be text in the report language".into());
        }
        if finding.finding_type == "equity_position_diagnosis" {
            if task.run_type != "equity_position_diagnosis" || finding.market != task.market {
                return Err(
                    "equity position diagnosis must match its scoped diagnosis task".into(),
                );
            }
            let legacy_snapshot_scope = expected_position_items.is_empty()
                && finding.affected_object_type.as_deref() == Some("position_snapshot");
            if (!matches!(
                finding.affected_object_type.as_deref(),
                Some("trade_intent_version" | "position_snapshot_item")
            ) && !legacy_snapshot_scope)
                || finding.affected_object_id.is_none()
            {
                return Err(
                    "equity position diagnosis must identify a frozen position item or Trade Intent version"
                        .into(),
                );
            }
            let mut diagnosed_position_item = None;
            if !expected_position_items.is_empty() {
                let cited_items = finding
                    .evidence_keys
                    .iter()
                    .filter_map(|key| output.evidence.iter().find(|evidence| evidence.key == *key))
                    .filter(|evidence| {
                        evidence.frozen_object_type.as_deref() == Some("position_snapshot_item")
                    })
                    .filter_map(|evidence| evidence.frozen_object_id.as_deref())
                    .filter_map(|id| Uuid::parse_str(id).ok())
                    .collect::<HashSet<_>>();
                if cited_items.len() != 1 {
                    return Err(
                        "each equity position diagnosis must cite exactly one frozen position item"
                            .into(),
                    );
                }
                let cited_item = *cited_items.iter().next().expect("checked one item");
                if !expected_position_items.contains(&cited_item) {
                    return Err(
                        "equity position diagnosis cites a position outside its scoped market"
                            .into(),
                    );
                }
                if !diagnosed_position_items.insert(cited_item) {
                    return Err("a frozen equity position was diagnosed more than once".into());
                }
                diagnosed_position_item = Some(cited_item);
            }
            let has_company_or_product_evidence = finding.evidence_keys.iter().any(|key| {
                output.evidence.iter().any(|evidence| {
                    evidence.key == *key
                        && matches!(evidence.coverage_scope.as_str(), "company" | "product")
                        && diagnosed_position_item.is_some_and(|position_item_id| {
                            known
                                .position_item_scopes
                                .get(&position_item_id)
                                .is_some_and(|scope| {
                                    evidence
                                        .related_instrument_id
                                        .as_deref()
                                        .and_then(|id| Uuid::parse_str(id).ok())
                                        == Some(scope.instrument_id)
                                })
                        })
                })
            });
            let exposes_coverage_gap = finding.data_gaps.iter().any(|gap| {
                let english_gap = gap.to_ascii_lowercase();
                if known.language == ReportLanguage::En {
                    return (english_gap.contains("company") || english_gap.contains("product"))
                        && english_gap.contains("evidence")
                        && english_gap.contains("search")
                        && [
                            "issuer",
                            "investor relations",
                            "exchange",
                            "regulator",
                            "provider",
                            "sec",
                            "hkex",
                            "sse",
                            "szse",
                        ]
                        .iter()
                        .any(|source| english_gap.contains(source));
                }
                (gap.contains("公司级证据")
                    || gap.contains("产品级证据")
                    || gap.contains("公司特定证据"))
                    && gap.contains("已检索")
                    && [
                        "发行人",
                        "投资者关系",
                        "交易所",
                        "监管",
                        "产品提供方",
                        "SEC",
                        "HKEX",
                        "上交所",
                        "深交所",
                    ]
                    .iter()
                    .any(|source_class| gap.contains(source_class))
            });
            if !has_company_or_product_evidence && !exposes_coverage_gap {
                return Err(
                    "each equity diagnosis requires company/product evidence or an explicit coverage gap"
                        .into(),
                );
            }
            let analysis = finding.analysis.as_ref().ok_or_else(|| {
                "equity position diagnosis requires structured diagnostic dimensions".to_string()
            })?;
            if !matches!(analysis.intent_assessment.as_str(), "matched" | "missing") {
                return Err("equity diagnosis intent_assessment must be matched or missing".into());
            }
            if [
                analysis.macro_context.as_str(),
                analysis.industry_context.as_str(),
                analysis.company_context.as_str(),
                analysis.market_pricing.as_str(),
                analysis.future_expectation.as_str(),
                analysis.expectation_gap.as_str(),
            ]
            .iter()
            .any(|value| value.trim().is_empty() || !is_report_text(value, known.language))
            {
                return Err(
                    "equity diagnostic dimensions must be non-empty text in the report language"
                        .into(),
                );
            }
            if analysis
                .upside_catalysts
                .iter()
                .chain(&analysis.downside_pressures)
                .chain(&analysis.core_risks)
                .any(|value| value.trim().is_empty() || !is_report_text(value, known.language))
            {
                return Err(
                    "equity diagnostic list items must be text in the report language".into(),
                );
            }
            if let Some(position_item_id) = diagnosed_position_item {
                let matching_intents = known.matching_intent_versions(position_item_id);
                match analysis.intent_assessment.as_str() {
                    "matched" => {
                        if analysis.intent_draft.is_some() {
                            return Err(
                                "a matched Trade Intent diagnosis cannot propose a replacement draft"
                                    .into(),
                            );
                        }
                        let affected_intent_id = match (
                            finding.affected_object_type.as_deref(),
                            finding.affected_object_id.as_deref(),
                        ) {
                            (Some("trade_intent_version"), Some(id)) => Uuid::parse_str(id)
                                .map_err(
                                    |_| "equity diagnosis has an invalid Trade Intent version ID",
                                )?,
                            _ => {
                                return Err(
                                    "a matched equity diagnosis must affect its Trade Intent version"
                                        .into(),
                                );
                            }
                        };
                        if !matching_intents.contains(&affected_intent_id) {
                            return Err(
                                "equity diagnosis Trade Intent does not match the frozen position scope"
                                    .into(),
                            );
                        }
                        if finding.impact_direction.is_none() {
                            return Err(
                                "a matched equity diagnosis requires an Intent impact direction"
                                    .into(),
                            );
                        }
                        let cites_intent = finding.evidence_keys.iter().any(|key| {
                            output.evidence.iter().any(|evidence| {
                                evidence.key == *key
                                    && evidence.frozen_object_type.as_deref()
                                        == Some("trade_intent_version")
                                    && evidence
                                        .frozen_object_id
                                        .as_deref()
                                        .and_then(|id| Uuid::parse_str(id).ok())
                                        == Some(affected_intent_id)
                            })
                        });
                        if !cites_intent {
                            return Err(
                                "a matched equity diagnosis must cite its frozen Trade Intent version"
                                    .into(),
                            );
                        }
                    }
                    "missing" => {
                        if !matching_intents.is_empty() {
                            return Err(
                                "equity diagnosis marks its Intent missing although a matching active version exists"
                                    .into(),
                            );
                        }
                        if finding.affected_object_type.as_deref() != Some("position_snapshot_item")
                            || finding
                                .affected_object_id
                                .as_deref()
                                .and_then(|id| Uuid::parse_str(id).ok())
                                != Some(position_item_id)
                            || finding.impact_direction.is_some()
                            || finding.impact_strength.is_some()
                            || finding.conditional_action.is_some()
                            || finding.data_gaps.is_empty()
                        {
                            return Err(
                                "a diagnosis without Trade Intent must remain position-scoped, non-directional, non-actionable, and expose the gap"
                                    .into(),
                            );
                        }
                        let draft = analysis.intent_draft.as_ref().ok_or(
                            "a diagnosis without Trade Intent requires a proposed intent draft",
                        )?;
                        if draft.thesis.trim().is_empty()
                            || !is_report_text(&draft.thesis, known.language)
                            || !matches!(
                                draft.time_horizon.as_str(),
                                "short" | "swing" | "medium" | "long"
                            )
                            || draft.invalidation_conditions.is_empty()
                            || draft
                                .catalysts
                                .iter()
                                .chain(&draft.invalidation_conditions)
                                .any(|value| {
                                    value.trim().is_empty()
                                        || !is_report_text(value, known.language)
                                })
                        {
                            return Err(
                                "proposed Trade Intent must be a complete review draft in the report language"
                                    .into(),
                            );
                        }
                    }
                    _ => unreachable!("intent assessment enum checked above"),
                }
            }
        } else if finding.analysis.is_some() {
            return Err("only equity position diagnoses may include diagnostic dimensions".into());
        }
        validate_nonempty_evidence_keys(&finding.evidence_keys, &evidence_keys)?;
        for event_key in &finding.related_event_keys {
            if !event_keys.contains(event_key.as_str()) {
                return Err(format!("finding references unknown event key {event_key}"));
            }
        }
        for narrative_key in &finding.related_narrative_keys {
            if !narrative_keys.contains(narrative_key.as_str()) {
                return Err(format!(
                    "finding references unknown narrative key {narrative_key}"
                ));
            }
        }
        for path_key in &finding.related_transmission_path_keys {
            if !path_keys.contains(path_key.as_str()) {
                return Err(format!(
                    "finding references unknown transmission path key {path_key}"
                ));
            }
        }
        validate_affected_reference(
            finding.affected_object_type.as_deref(),
            finding.affected_object_id.as_deref(),
            known,
        )?;
        if let Some(action) = finding.conditional_action.as_deref() {
            if action.trim().is_empty() {
                return Err("conditional_action cannot be blank".into());
            }
            if finding.confirmation_conditions.is_empty()
                || finding.invalidation_conditions.is_empty()
                || finding
                    .confirmation_conditions
                    .iter()
                    .chain(&finding.invalidation_conditions)
                    .any(|condition| condition.trim().is_empty())
            {
                return Err(
                    "a conditional action requires confirmation and invalidation conditions".into(),
                );
            }
        }
    }
    for evidence in output.evidence.iter().filter(|evidence| {
        evidence.coverage_scope == "industry"
            && evidence.relationship_basis.as_deref() == Some("direct_transmission")
    }) {
        let instrument_id = parse_required_uuid(
            evidence.related_instrument_id.as_deref(),
            "related_instrument_id",
        )?;
        let has_structured_path = output.findings.iter().any(|finding| {
            finding.evidence_keys.iter().any(|key| key == &evidence.key)
                && finding
                    .related_transmission_path_keys
                    .iter()
                    .any(|path_key| {
                        output.transmission_paths.iter().any(|path| {
                            path.path_key == *path_key
                                && path.evidence_keys.iter().any(|key| key == &evidence.key)
                                && path
                                    .target_instrument_id
                                    .as_deref()
                                    .and_then(|id| Uuid::parse_str(id).ok())
                                    == Some(instrument_id)
                        })
                    })
        });
        if !has_structured_path {
            return Err(format!(
                "industry evidence {} has no finding-linked direct transmission path to its instrument",
                evidence.key
            ));
        }
    }
    if diagnosed_position_items != expected_position_items {
        return Err("equity position diagnoses do not cover every frozen scoped position".into());
    }
    for event_key in event_keys {
        if !output.findings.iter().any(|finding| {
            finding
                .related_event_keys
                .iter()
                .any(|key| key == event_key)
        }) {
            return Err(format!(
                "market event {event_key} has no citable validated finding"
            ));
        }
    }
    for narrative_key in narrative_keys {
        if !output.findings.iter().any(|finding| {
            finding
                .related_narrative_keys
                .iter()
                .any(|key| key == narrative_key)
        }) {
            return Err(format!(
                "narrative {narrative_key} has no citable validated finding"
            ));
        }
    }
    for path_key in path_keys {
        if !output.findings.iter().any(|finding| {
            finding
                .related_transmission_path_keys
                .iter()
                .any(|key| key == path_key)
        }) {
            return Err(format!(
                "transmission path {path_key} has no citable validated finding"
            ));
        }
    }
    for narrative in &output.narratives {
        if !allowed_markets.contains(&narrative.market.as_str())
            || narrative.name.trim().is_empty()
            || narrative.stage.trim().is_empty()
            || narrative.summary.trim().is_empty()
            || !matches!(narrative.narrative_type.as_str(), "explicit" | "latent")
            || !(0.0..=1.0).contains(&narrative.confidence)
        {
            return Err("invalid narrative type or confidence".into());
        }
        if !is_report_text(&narrative.name, known.language)
            || !is_report_text(&narrative.summary, known.language)
            || narrative
                .capital_flow_inference
                .as_deref()
                .is_some_and(|value| !is_report_text(value, known.language))
        {
            return Err("narrative fields must be text in the report language".into());
        }
        validate_nonempty_evidence_keys(&narrative.evidence_keys, &evidence_keys)?;
        for id in &narrative.instrument_ids {
            let id = Uuid::parse_str(id).map_err(|_| format!("invalid instrument ID {id}"))?;
            known.validate("instrument", id)?;
        }
    }
    for path in &output.transmission_paths {
        if path.source_driver.trim().is_empty()
            || path.steps.is_empty()
            || path.steps.iter().any(|step| step.trim().is_empty())
            || !allowed_markets.contains(&path.target_market.as_str())
            || path.direction.trim().is_empty()
            || !(0.0..=1.0).contains(&path.confidence)
        {
            return Err("invalid transmission path".into());
        }
        if !is_report_text(&path.source_driver, known.language)
            || path
                .steps
                .iter()
                .any(|step| !is_report_text(step, known.language))
        {
            return Err("transmission path fields must be text in the report language".into());
        }
        validate_nonempty_evidence_keys(&path.evidence_keys, &evidence_keys)?;
        if let Some(id) = &path.target_instrument_id {
            let id = Uuid::parse_str(id).map_err(|_| format!("invalid instrument ID {id}"))?;
            known.validate("instrument", id)?;
        }
    }
    Ok(output)
}

fn validate_evidence_keys(keys: &[String], known: &HashSet<&str>) -> Result<(), String> {
    for key in keys {
        if !known.contains(key.as_str()) {
            return Err(format!("unknown evidence key {key}"));
        }
    }
    Ok(())
}

fn validate_nonempty_evidence_keys(keys: &[String], known: &HashSet<&str>) -> Result<(), String> {
    if keys.is_empty() {
        return Err("a factual output must cite at least one evidence key".into());
    }
    validate_evidence_keys(keys, known)
}

fn parse_required_uuid(value: Option<&str>, field: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value.ok_or_else(|| format!("{field} is required"))?)
        .map_err(|_| format!("invalid {field}"))
}

fn validate_typed_reference(
    object_type: Option<&str>,
    object_id: Option<&str>,
    known: &KnownObjects,
) -> Result<(), String> {
    let object_type = object_type.ok_or("frozen_object_type is required")?;
    let id = parse_required_uuid(object_id, "frozen_object_id")?;
    known.validate(object_type, id)
}

fn validate_affected_reference(
    object_type: Option<&str>,
    object_id: Option<&str>,
    known: &KnownObjects,
) -> Result<(), String> {
    match (object_type, object_id) {
        (None, None) => {
            Err("every finding must identify an affected object or aggregate scope".into())
        }
        (Some("portfolio" | "market"), None) => Ok(()),
        (Some(object_type), Some(object_id)) => {
            let id = Uuid::parse_str(object_id)
                .map_err(|_| format!("invalid affected object ID {object_id}"))?;
            known.validate(object_type, id)
        }
        _ => Err("affected_object_type and affected_object_id do not form a valid pair".into()),
    }
}

fn collect_known_objects(value: &Value) -> Result<KnownObjects, String> {
    fn add_uuid(target: &mut HashSet<Uuid>, row: &Value, key: &str) {
        if let Some(value) = row.get(key).and_then(Value::as_str)
            && let Ok(id) = Uuid::parse_str(value)
        {
            target.insert(id);
        }
    }
    let portfolio = value
        .get("frozen_portfolio")
        .ok_or("research input has no frozen_portfolio")?;
    let mut known = KnownObjects {
        language: ReportLanguage::from_input(value),
        ..KnownObjects::default()
    };
    for row in portfolio
        .get("positions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        add_uuid(&mut known.position_snapshots, row, "snapshot_id");
        add_uuid(
            &mut known.position_snapshot_items,
            row,
            "position_snapshot_item_id",
        );
        add_uuid(&mut known.instruments, row, "instrument_id");
        add_uuid(&mut known.accounts, row, "account_id");
        if let Some(instrument_id) = row
            .get("instrument_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
        {
            known.instrument_labels.insert(
                instrument_id,
                InstrumentLabels {
                    symbol: row
                        .get("symbol")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    name: row
                        .get("instrument_name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                },
            );
        }
        if let (Some(item_id), Some(market)) = (
            row.get("position_snapshot_item_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok()),
            row.get("market").and_then(Value::as_str),
        ) {
            known.position_item_markets.insert(item_id, market.into());
        }
        if let (Some(item_id), Some(instrument_id)) = (
            row.get("position_snapshot_item_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok()),
            row.get("instrument_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok()),
        ) {
            known.position_item_scopes.insert(
                item_id,
                FrozenPositionScope {
                    instrument_id,
                    account_id: row
                        .get("account_id")
                        .and_then(Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok()),
                    wallet_id: row
                        .get("wallet_id")
                        .and_then(Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok()),
                },
            );
        }
        if let (Some(snapshot_id), Some(instrument_id)) = (
            row.get("snapshot_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok()),
            row.get("instrument_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok()),
        ) {
            known
                .position_instruments
                .entry(snapshot_id)
                .or_default()
                .insert(instrument_id);
        }
    }
    for row in portfolio
        .get("trade_intent_versions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let version_id = row
            .get("trade_intent_version_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok());
        add_uuid(
            &mut known.trade_intent_versions,
            row,
            "trade_intent_version_id",
        );
        add_uuid(&mut known.trade_intents, row, "trade_intent_id");
        add_uuid(&mut known.accounts, row, "account_id");
        let instrument_ids = row
            .get("instrument_ids")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|value| Uuid::parse_str(value).ok())
            .collect::<HashSet<_>>();
        known.instruments.extend(instrument_ids.iter().copied());
        if let Some(version_id) = version_id {
            known.trade_intent_version_scopes.insert(
                version_id,
                FrozenIntentScope {
                    instrument_ids,
                    account_id: row
                        .get("account_id")
                        .and_then(Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok()),
                    wallet_id: row
                        .get("wallet_id")
                        .and_then(Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok()),
                },
            );
        }
    }
    for row in portfolio
        .get("prior_narratives")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        add_uuid(&mut known.market_narratives, row, "narrative_id");
    }
    for row in portfolio
        .get("source_snapshots")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let (Some(id), Some(as_of)) = (
            row.get("source_snapshot_id").and_then(Value::as_str),
            row.get("as_of").and_then(Value::as_str),
        ) && let (Ok(id), Ok(as_of)) = (Uuid::parse_str(id), parse_time(as_of))
        {
            known.source_snapshots.insert(
                id,
                FrozenSourceSnapshot {
                    as_of,
                    source_type: row
                        .get("source_type")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    payload: row.get("payload").cloned().unwrap_or(Value::Null),
                },
            );
        }
    }
    for row in portfolio
        .get("market_events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        add_uuid(&mut known.market_events, row, "market_event_id");
    }
    for row in value
        .get("validated_upstream_findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        add_uuid(&mut known.research_findings, row, "id");
    }
    for row in value
        .get("validated_upstream_narratives")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        add_uuid(&mut known.market_narratives, row, "narrative_id");
    }
    Ok(known)
}

#[allow(clippy::too_many_arguments)]
async fn persist_research_output(
    state: &AppState,
    briefing_id: Uuid,
    lease_token: Uuid,
    run_id: Uuid,
    task: ResearchTask,
    as_of: DateTime<Utc>,
    skill_version: &str,
    output: ResearchOutput,
) -> Result<Vec<Uuid>, String> {
    let raw_output = serde_json::to_value(&output).map_err(|error| error.to_string())?;
    let evidence_payload =
        serde_json::to_value(&output.evidence).map_err(|error| error.to_string())?;
    let checksum =
        sha256_hex(&serde_json::to_vec(&evidence_payload).map_err(|error| error.to_string())?);
    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|error| error.to_string())?;
    let owned: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM briefing_runs
           WHERE id = $1 AND status = 'running' AND lease_token = $2
           FOR UPDATE"#,
    )
    .bind(briefing_id)
    .bind(lease_token)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    if owned.is_none() {
        return Err("briefing lease was lost before research persistence".into());
    }
    let source_snapshot_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO source_snapshots
           (provider, source_type, captured_at, as_of, request, payload, checksum)
           VALUES ('local_codex', 'research_evidence', now(), $1, $2, $3, $4)
           RETURNING id"#,
    )
    .bind(as_of)
    .bind(json!({ "research_run_id": run_id, "skill_name": task.skill_name, "skill_version": skill_version }))
    .bind(&evidence_payload)
    .bind(checksum)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;

    let mut evidence_ids = HashMap::new();
    for evidence in &output.evidence {
        let referenced_snapshot_id = evidence
            .source_snapshot_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|error| error.to_string())?;
        let id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO evidence_sources
               (source_snapshot_id, source_type, title, url, publisher, published_at,
                observed_at, excerpt, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id"#,
        )
        .bind(referenced_snapshot_id.unwrap_or(source_snapshot_id))
        .bind(&evidence.source_type)
        .bind(&evidence.title)
        .bind(&evidence.url)
        .bind(&evidence.publisher)
        .bind(parse_optional_publication_time(
            evidence.published_at.as_deref(),
        )?)
        .bind(parse_time(&evidence.observed_at)?)
        .bind(&evidence.excerpt)
        .bind(json!({
            "research_run_id": run_id,
            "evidence_key": evidence.key,
            "coverage_scope": evidence.coverage_scope,
            "related_instrument_id": evidence.related_instrument_id,
            "relationship_basis": evidence.relationship_basis,
            "relationship_explanation": evidence.relationship_explanation,
            "aggregate_source_snapshot_id": source_snapshot_id,
            "source_snapshot_id": evidence.source_snapshot_id,
            "frozen_object_type": evidence.frozen_object_type,
            "frozen_object_id": evidence.frozen_object_id,
        }))
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        evidence_ids.insert(evidence.key.as_str(), id);
    }

    for event in &output.events {
        sqlx::query(
            r#"INSERT INTO research_run_events
               (research_run_id, event_key, market, event_type, is_scheduled,
                occurred_at, headline, facts, evidence_keys)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        )
        .bind(run_id)
        .bind(&event.event_key)
        .bind(&event.market)
        .bind(&event.event_type)
        .bind(event.is_scheduled)
        .bind(parse_time(&event.occurred_at)?)
        .bind(&event.headline)
        .bind(json!(event.facts))
        .bind(json!(event.evidence_keys))
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query(
            r#"INSERT INTO market_events
               (event_key, market, event_type, occurred_at, headline, facts)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (event_key) DO NOTHING"#,
        )
        .bind(&event.event_key)
        .bind(&event.market)
        .bind(&event.event_type)
        .bind(parse_time(&event.occurred_at)?)
        .bind(&event.headline)
        .bind(json!({
            "facts": event.facts,
            "is_scheduled": event.is_scheduled,
            "evidence_keys": event.evidence_keys,
            "research_run_id": run_id,
        }))
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    }

    let mut finding_ids = Vec::new();
    for finding in &output.findings {
        let affected_id = finding
            .affected_object_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|error| error.to_string())?;
        let id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO research_findings
               (research_run_id, finding_type, market, window_start, window_end,
                affected_object_type, affected_object_id, fact_summary, inference,
                impact_direction, impact_strength, time_horizon, confidence,
                contrary_evidence, confirmation_conditions, invalidation_conditions,
                conditional_action, data_gaps, related_event_keys,
                related_narrative_keys, related_transmission_path_keys,
                diagnostic_dimensions)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                       $13::numeric, $14, $15, $16, $17, $18, $19, $20, $21, $22)
               RETURNING id"#,
        )
        .bind(run_id)
        .bind(&finding.finding_type)
        .bind(&finding.market)
        .bind(parse_time(&finding.window_start)?)
        .bind(parse_time(&finding.window_end)?)
        .bind(&finding.affected_object_type)
        .bind(affected_id)
        .bind(&finding.fact_summary)
        .bind(&finding.inference)
        .bind(&finding.impact_direction)
        .bind(finding.impact_strength)
        .bind(&finding.time_horizon)
        .bind(finding.confidence.to_string())
        .bind(json!(finding.contrary_evidence))
        .bind(json!(finding.confirmation_conditions))
        .bind(json!(finding.invalidation_conditions))
        .bind(&finding.conditional_action)
        .bind(json!(finding.data_gaps))
        .bind(json!(finding.related_event_keys))
        .bind(json!(finding.related_narrative_keys))
        .bind(json!(finding.related_transmission_path_keys))
        .bind(finding.analysis.as_ref().map(|analysis| json!(analysis)))
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        for key in &finding.evidence_keys {
            let evidence_id = evidence_ids
                .get(key.as_str())
                .ok_or_else(|| format!("unknown evidence key {key}"))?;
            sqlx::query(
                "INSERT INTO finding_evidence (finding_id, evidence_source_id, relevance) VALUES ($1, $2, 'supports')",
            )
            .bind(id)
            .bind(evidence_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
        }
        finding_ids.push(id);
    }

    persist_narratives(&mut transaction, run_id, as_of, &output.narratives).await?;
    persist_transmission_paths(&mut transaction, run_id, &output.transmission_paths).await?;
    sqlx::query(
        r#"UPDATE research_runs
           SET status = 'complete', raw_output = $2, validation_status = 'valid',
               error = NULL, data_gaps = $3, completed_at = now()
           WHERE id = $1"#,
    )
    .bind(run_id)
    .bind(raw_output)
    .bind(json!(output.data_gaps))
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(finding_ids)
}

async fn persist_narratives(
    transaction: &mut Transaction<'_, Postgres>,
    run_id: Uuid,
    as_of: DateTime<Utc>,
    narratives: &[NarrativeOutput],
) -> Result<(), String> {
    for narrative in narratives {
        let narrative_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO market_narratives (market, name, narrative_type)
               VALUES ($1, $2, $3)
               ON CONFLICT (market, name) DO UPDATE SET narrative_type = EXCLUDED.narrative_type
               RETURNING id"#,
        )
        .bind(&narrative.market)
        .bind(&narrative.name)
        .bind(&narrative.narrative_type)
        .fetch_one(&mut **transaction)
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query(
            r#"INSERT INTO narrative_snapshots
               (narrative_id, research_run_id, as_of, stage, summary, confidence,
                capital_flow_inference, evidence)
               VALUES ($1, $2, $3, $4, $5, $6::numeric, $7, $8)"#,
        )
        .bind(narrative_id)
        .bind(run_id)
        .bind(as_of)
        .bind(&narrative.stage)
        .bind(&narrative.summary)
        .bind(narrative.confidence.to_string())
        .bind(&narrative.capital_flow_inference)
        .bind(json!({
            "narrative_key": narrative.narrative_key,
            "evidence_keys": narrative.evidence_keys,
        }))
        .execute(&mut **transaction)
        .await
        .map_err(|error| error.to_string())?;
        for instrument_id in &narrative.instrument_ids {
            sqlx::query(
                r#"INSERT INTO narrative_instruments (narrative_id, instrument_id, role)
                   VALUES ($1, $2, 'mentioned')
                   ON CONFLICT (narrative_id, instrument_id) DO UPDATE SET role = EXCLUDED.role"#,
            )
            .bind(narrative_id)
            .bind(Uuid::parse_str(instrument_id).map_err(|error| error.to_string())?)
            .execute(&mut **transaction)
            .await
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

async fn persist_transmission_paths(
    transaction: &mut Transaction<'_, Postgres>,
    run_id: Uuid,
    paths: &[TransmissionPathOutput],
) -> Result<(), String> {
    for path in paths {
        let instrument_id = path
            .target_instrument_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|error| error.to_string())?;
        sqlx::query(
            r#"INSERT INTO transmission_paths
               (research_run_id, source_driver, steps, target_market,
                target_instrument_id, direction, confidence)
               VALUES ($1, $2, $3, $4, $5, $6, $7::numeric)"#,
        )
        .bind(run_id)
        .bind(&path.source_driver)
        .bind(json!({
            "path_key": path.path_key,
            "path": path.steps,
            "evidence_keys": path.evidence_keys,
        }))
        .bind(&path.target_market)
        .bind(instrument_id)
        .bind(&path.direction)
        .bind(path.confidence.to_string())
        .execute(&mut **transaction)
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn execute_composition(
    state: &AppState,
    briefing_id: Uuid,
    lease_token: Uuid,
    as_of: DateTime<Utc>,
    skill_name: &str,
    input: Value,
    failed_skills: &[String],
) -> Result<(), String> {
    let language = ReportLanguage::from_input(&input);
    let skill_version = hash_skill(&state.research.project_root, skill_name)?;
    let schema = state
        .research
        .project_root
        .join("schemas/briefing-output-v1.json");
    let prompt = format!(
        r#"Read and follow `skills/{skill_name}/SKILL.md` completely. Compose one Alpha Echo briefing from only the validated research context in the input. Do not browse or add facts. Events, narratives, and transmission paths are included only when a validated Finding from the same research_run_id links to their stable key; match them by `(research_run_id, key)`. Every item must cite at least one existing source_finding_id. Return JSON only and conform exactly to the supplied output schema. Top-level as_of must equal {as_of}. Write the title, executive summary, every item title/body, and every data gap in {language}; keep tickers and identifiers unchanged. For each position_diagnosis item whose Finding has `intent_assessment=missing`, set `intent_status=draft_unconfirmed`, `actionability=non_actionable`, explicitly say `{confirmation_label}`, and do not recommend buying, selling, adding, or reducing. For a matched Intent set `intent_status=active` and `actionability=conditional`. All non-position items set both fields to null. Do not emit a `system` item; deterministic run-status items are created only by Rust. It is correct to state that there is no material change or no high-quality opportunity when supported by findings.

Frozen composition input:
{}"#,
        serde_json::to_string(&input).map_err(|error| error.to_string())?,
        as_of = as_of.to_rfc3339(),
        language = language.name(),
        confirmation_label = language.text("待用户确认", "awaiting user confirmation"),
    );
    if prompt.len() > MAX_CODEX_PROMPT_BYTES {
        return Err(format!(
            "composition prompt is {} bytes after raw source removal and exceeds the safe Codex input budget",
            prompt.len()
        ));
    }
    let valid_finding_ids = collect_finding_ids(&input);
    let required_position_finding_ids =
        collect_finding_ids_by_type(&input, "equity_position_diagnosis");
    let position_intent_assessments = collect_position_intent_assessments(&input);
    let deterministic_missing_items = collect_deterministic_missing_items(&input);
    let mut last_error = String::new();
    let mut attempt_prompt = prompt.clone();
    for attempt in 1..=state.research.max_attempts {
        renew_briefing_lease(state, briefing_id, lease_token).await?;
        match run_codex(state, &schema, &attempt_prompt, false).await {
            Ok(raw) => {
                renew_briefing_lease(state, briefing_id, lease_token).await?;
                save_briefing_raw_output(state, briefing_id, &raw).await?;
                match parse_and_validate_briefing_in_language(
                    &raw,
                    as_of,
                    &valid_finding_ids,
                    &required_position_finding_ids,
                    &position_intent_assessments,
                    &deterministic_missing_items,
                    language,
                ) {
                    Ok(output) => {
                        return persist_briefing_output(
                            state,
                            briefing_id,
                            lease_token,
                            skill_name,
                            &skill_version,
                            output,
                            failed_skills,
                            language,
                        )
                        .await;
                    }
                    Err(error) => {
                        last_error = format!("attempt {attempt} validation failed: {error}");
                        attempt_prompt = validation_retry_prompt(&prompt, &error);
                    }
                }
            }
            Err(error) => last_error = format!("attempt {attempt} execution failed: {error}"),
        }
    }
    Err(last_error)
}

#[cfg(test)]
fn parse_and_validate_briefing(
    raw: &str,
    as_of: DateTime<Utc>,
    valid: &HashSet<Uuid>,
    required: &HashSet<Uuid>,
    assessments: &HashMap<Uuid, String>,
    deterministic: &HashMap<Uuid, DeterministicPositionItem>,
) -> Result<BriefingOutput, String> {
    parse_and_validate_briefing_in_language(
        raw,
        as_of,
        valid,
        required,
        assessments,
        deterministic,
        ReportLanguage::Zh,
    )
}

fn parse_and_validate_briefing_in_language(
    raw: &str,
    as_of: DateTime<Utc>,
    valid_finding_ids: &HashSet<Uuid>,
    required_position_finding_ids: &HashSet<Uuid>,
    position_intent_assessments: &HashMap<Uuid, String>,
    deterministic_missing_items: &HashMap<Uuid, DeterministicPositionItem>,
    language: ReportLanguage,
) -> Result<BriefingOutput, String> {
    let mut output: BriefingOutput =
        serde_json::from_str(raw).map_err(|error| format!("invalid JSON: {error}"))?;
    if parse_time(&output.as_of)? != as_of
        || output.title.trim().is_empty()
        || output.executive_summary.trim().is_empty()
        || output.items.is_empty()
    {
        return Err("briefing cutoff is wrong or the briefing has no items".into());
    }
    if !is_report_text(&output.title, language)
        || !is_report_text(&output.executive_summary, language)
    {
        return Err("briefing title and executive summary must use the report language".into());
    }
    for item in &mut output.items {
        let missing_id = item.source_finding_ids.iter().find_map(|id| {
            let id = Uuid::parse_str(id).ok()?;
            deterministic_missing_items.get(&id).map(|body| (id, body))
        });
        if let Some((_id, deterministic)) = missing_id {
            item.title.clone_from(&deterministic.title);
            item.body.clone_from(&deterministic.body);
        }
    }
    let mut covered_position_findings = HashSet::new();
    for item in &output.items {
        if item.section.trim().is_empty()
            || item.title.trim().is_empty()
            || item.body.trim().is_empty()
        {
            return Err("briefing items require a section, title, and body".into());
        }
        if !is_report_text(&item.title, language) || !is_report_text(&item.body, language) {
            return Err(format!(
                "briefing item {} must have a title and body in the report language",
                item.title
            ));
        }
        if item.section == "system" || item.source_finding_ids.is_empty() {
            return Err(format!(
                "briefing item {} is a reserved system item or has no validated finding citation",
                item.title
            ));
        }
        let mut position_finding_count = 0;
        for id in &item.source_finding_ids {
            let id = Uuid::parse_str(id).map_err(|_| format!("invalid finding ID {id}"))?;
            if !valid_finding_ids.contains(&id) {
                return Err(format!("briefing cites an unknown finding {id}"));
            }
            if required_position_finding_ids.contains(&id) {
                position_finding_count += 1;
                if item.section != "position_diagnosis" {
                    return Err(format!(
                        "position diagnosis finding {id} must be presented in a position_diagnosis item"
                    ));
                }
                match position_intent_assessments.get(&id).map(String::as_str) {
                    Some("missing") => {
                        if item.intent_status.as_deref() != Some("draft_unconfirmed")
                            || item.actionability.as_deref() != Some("non_actionable")
                            || !item
                                .body
                                .contains(language.text("待用户确认", "awaiting user confirmation"))
                            || !deterministic_missing_items.contains_key(&id)
                        {
                            return Err(format!(
                                "missing-Intent position item {id} must remain an unconfirmed, non-actionable draft"
                            ));
                        }
                    }
                    Some("matched") => {
                        if item.intent_status.as_deref() != Some("active")
                            || item.actionability.as_deref() != Some("conditional")
                            || !has_position_news_context(&item.body)
                            || !deterministic_missing_items.contains_key(&id)
                        {
                            return Err(format!(
                                "matched-Intent position item {id} must declare active conditional status and show either incremental news plus baseline or the latest verified context"
                            ));
                        }
                    }
                    _ => {
                        return Err(format!(
                            "position finding {id} has no validated Intent state"
                        ));
                    }
                }
                covered_position_findings.insert(id);
            }
        }
        if item.section == "position_diagnosis" && position_finding_count != 1 {
            return Err(format!(
                "each position_diagnosis item must cite exactly one position diagnosis finding: {}",
                item.title
            ));
        }
        if item.section != "position_diagnosis"
            && (item.intent_status.is_some() || item.actionability.is_some())
        {
            return Err("only position_diagnosis items may declare Intent state".into());
        }
    }
    if covered_position_findings != *required_position_finding_ids {
        let missing = required_position_finding_ids
            .difference(&covered_position_findings)
            .map(Uuid::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "briefing is missing individual position diagnoses for findings: {missing}"
        ));
    }
    Ok(output)
}

fn has_position_news_context(body: &str) -> bool {
    body.contains("最近动态")
        || (body.contains("本轮新增") && body.contains("近期背景"))
        || body.contains("Latest developments")
        || (body.contains("New this run") && body.contains("Recent background"))
}

fn is_chinese_text(value: &str) -> bool {
    let han = value
        .chars()
        .filter(|character| matches!(character, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}'))
        .count();
    let mut latin = 0;
    let mut segment = String::new();
    let flush_segment = |segment: &mut String, latin: &mut usize| {
        if !is_machine_identifier(segment) {
            *latin += segment
                .chars()
                .filter(|character| character.is_ascii_alphabetic())
                .count();
        }
        segment.clear();
    };
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '.' | ':' | '/' | '_') {
            segment.push(character);
        } else {
            flush_segment(&mut segment, &mut latin);
        }
    }
    flush_segment(&mut segment, &mut latin);
    han >= 2 && han.saturating_mul(2) >= latin
}

fn is_report_text(value: &str, language: ReportLanguage) -> bool {
    if language == ReportLanguage::Zh {
        return is_chinese_text(value);
    }
    let latin = value.chars().filter(char::is_ascii_alphabetic).count();
    let han = value
        .chars()
        .filter(|c| matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}'))
        .count();
    latin >= 4 && latin > han.saturating_mul(2)
}

fn is_machine_identifier(token: &str) -> bool {
    let token = token.trim_matches(|character: char| {
        !character.is_ascii_alphanumeric() && !matches!(character, '-' | '.' | ':' | '/' | '_')
    });
    if token.is_empty() {
        return false;
    }
    let is_uuid = token.len() == 36
        && token
            .chars()
            .enumerate()
            .all(|(index, character)| match index {
                8 | 13 | 18 | 23 => character == '-',
                _ => character.is_ascii_hexdigit(),
            });
    let is_url = token.starts_with("http://") || token.starts_with("https://");
    let is_numeric_code = token
        .chars()
        .all(|character| character.is_ascii_digit() || matches!(character, '-' | '.' | ':' | '/'));
    let is_short_market_code = token.len() <= 12
        && token.chars().any(|character| character.is_ascii_digit())
        && token
            .chars()
            .any(|character| character.is_ascii_uppercase())
        && token.chars().all(|character| {
            character.is_ascii_uppercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '.' | ':' | '/' | '_')
        });
    is_uuid || is_url || is_numeric_code || is_short_market_code
}

async fn save_briefing_raw_output(
    state: &AppState,
    briefing_id: Uuid,
    raw: &str,
) -> Result<(), String> {
    let audit_value = raw_audit_value(raw);
    sqlx::query("UPDATE briefing_runs SET raw_output = $2 WHERE id = $1")
        .bind(briefing_id)
        .bind(audit_value)
        .execute(&state.pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn raw_audit_value(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| {
        let tail = raw.chars().rev().take(20_000).collect::<String>();
        json!({
            "invalid_json_tail": tail.chars().rev().collect::<String>(),
            "truncated": raw.chars().count() > 20_000,
        })
    })
}

#[cfg(test)]
fn render_briefing_markdown(output: &BriefingOutput, data_gaps: &[String]) -> String {
    render_briefing_markdown_in_language(output, data_gaps, ReportLanguage::Zh)
}

fn render_briefing_markdown_in_language(
    output: &BriefingOutput,
    data_gaps: &[String],
    language: ReportLanguage,
) -> String {
    let mut rendered = format!("# {}\n\n{}\n", output.title, output.executive_summary);
    for item in &output.items {
        rendered.push_str(&format!("\n## {}\n\n{}\n", item.title, item.body));
    }
    if !data_gaps.is_empty() {
        rendered.push_str(language.text("\n## 数据缺口\n\n", "\n## Data gaps\n\n"));
        for gap in data_gaps {
            rendered.push_str(&format!("- {gap}\n"));
        }
    }
    rendered
}

async fn persist_briefing_output(
    state: &AppState,
    briefing_id: Uuid,
    lease_token: Uuid,
    skill_name: &str,
    skill_version: &str,
    output: BriefingOutput,
    failed_skills: &[String],
    language: ReportLanguage,
) -> Result<(), String> {
    let raw_output = serde_json::to_value(&output).map_err(|error| error.to_string())?;
    let final_status = if failed_skills.is_empty() {
        "complete"
    } else {
        "partial"
    };
    let mut data_gaps = output.data_gaps.clone();
    data_gaps.extend(failed_skills.iter().cloned());
    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|error| error.to_string())?;
    for (index, item) in output.items.iter().enumerate() {
        let source_ids = item
            .source_finding_ids
            .iter()
            .map(|id| Uuid::parse_str(id).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        sqlx::query(
            r#"INSERT INTO briefing_items
               (briefing_run_id, section, sort_order, title, body, severity,
                source_finding_ids, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (briefing_run_id, section, sort_order)
               DO UPDATE SET title = EXCLUDED.title, body = EXCLUDED.body,
                             severity = EXCLUDED.severity,
                             source_finding_ids = EXCLUDED.source_finding_ids,
                             metadata = EXCLUDED.metadata"#,
        )
        .bind(briefing_id)
        .bind(&item.section)
        .bind((index + 1) as i32)
        .bind(&item.title)
        .bind(&item.body)
        .bind(&item.severity)
        .bind(json!(source_ids))
        .bind(json!({
            "validated": true,
            "intent_status": item.intent_status,
            "actionability": item.actionability,
        }))
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    }
    let rendered = render_briefing_markdown_in_language(&output, &data_gaps, language);
    let updated = sqlx::query(
        r#"UPDATE briefing_runs
           SET status = $2, raw_output = $3, rendered_markdown = $4, skill_name = $5,
               skill_version = $6, model = $7, schema_version = $8,
               validation_status = 'valid', error = NULL, data_gaps = $9,
               completed_at = now(), heartbeat_at = NULL, lease_token = NULL
           WHERE id = $1 AND lease_token = $10"#,
    )
    .bind(briefing_id)
    .bind(final_status)
    .bind(raw_output)
    .bind(rendered)
    .bind(skill_name)
    .bind(skill_version)
    .bind(&state.research.model)
    .bind(BRIEFING_SCHEMA_VERSION)
    .bind(json!(data_gaps))
    .bind(lease_token)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    if updated.rows_affected() != 1 {
        return Err("briefing lease was lost before composition persistence".into());
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())
}

async fn finish_research_partial(
    state: &AppState,
    briefing_id: Uuid,
    lease_token: Uuid,
    _kind: &str,
    as_of: DateTime<Utc>,
    data_gaps: Vec<String>,
    reason: &str,
) -> Result<(), String> {
    let snapshot: Value =
        sqlx::query_scalar("SELECT input_snapshot FROM briefing_runs WHERE id = $1")
            .bind(briefing_id)
            .fetch_one(&state.pool)
            .await
            .map_err(|error| error.to_string())?;
    let language = ReportLanguage::from_input(&snapshot);
    let rendered = localized_format!(
        language,
        "# 报告\n\n截止时间：{}\n\n{}\n\n研究失败和数据缺口已保留在本报告记录中。",
        "# Report\n\nAs of: {}\n\n{}\n\nResearch failures and data gaps are retained in this report record.",
        as_of.to_rfc3339(),
        reason,
    );
    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"INSERT INTO briefing_items
           (briefing_run_id, section, sort_order, title, body, severity, metadata)
           VALUES ($1, 'system', 1, $4, $2, 'warning', $3)
           ON CONFLICT (briefing_run_id, section, sort_order)
           DO UPDATE SET title = EXCLUDED.title, body = EXCLUDED.body,
                         severity = EXCLUDED.severity, metadata = EXCLUDED.metadata"#,
    )
    .bind(briefing_id)
    .bind(reason)
    .bind(json!({ "validated_only": true }))
    .bind(language.text("研究链路部分完成", "Research pipeline partially completed"))
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    let updated = sqlx::query(
        r#"UPDATE briefing_runs
           SET status = 'partial', rendered_markdown = $2, validation_status = 'partial',
               error = NULL, data_gaps = $3, completed_at = now(), heartbeat_at = NULL, lease_token = NULL
           WHERE id = $1 AND lease_token = $4"#,
    )
    .bind(briefing_id)
    .bind(rendered)
    .bind(json!(data_gaps))
    .bind(lease_token)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    if updated.rows_affected() != 1 {
        return Err("briefing lease was lost before partial result persistence".into());
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn load_validated_findings(
    state: &AppState,
    briefing_id: Uuid,
) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(finding_row) ORDER BY finding_row.created_at), '[]'::jsonb)
           FROM (
             SELECT rf.*, rr.skill_name, rr.skill_version, rr.model,
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
             WHERE rr.briefing_run_id = $1
               AND rr.status = 'complete' AND rr.validation_status = 'valid'
           ) finding_row"#,
    )
    .bind(briefing_id)
    .fetch_one(&state.pool)
    .await
}

async fn load_validated_events(state: &AppState, briefing_id: Uuid) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(event_row) ORDER BY event_row.occurred_at), '[]'::jsonb)
           FROM (
             SELECT rre.*
             FROM research_run_events rre
             JOIN research_runs rr ON rr.id = rre.research_run_id
             WHERE rr.briefing_run_id = $1
               AND rr.status = 'complete' AND rr.validation_status = 'valid'
               AND EXISTS (
                 SELECT 1 FROM research_findings rf
                 WHERE rf.research_run_id = rre.research_run_id
                   AND rf.related_event_keys ? rre.event_key
               )
           ) event_row"#,
    )
    .bind(briefing_id)
    .fetch_one(&state.pool)
    .await
}

async fn load_validated_narratives(
    state: &AppState,
    briefing_id: Uuid,
) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(narrative_row) ORDER BY narrative_row.as_of), '[]'::jsonb)
           FROM (
             SELECT ns.research_run_id, mn.id AS narrative_id,
                    mn.market, mn.name, mn.narrative_type,
                    ns.id AS narrative_snapshot_id, ns.as_of, ns.stage, ns.summary,
                    ns.confidence::text, ns.capital_flow_inference, ns.evidence
             FROM narrative_snapshots ns
             JOIN market_narratives mn ON mn.id = ns.narrative_id
             JOIN research_runs rr ON rr.id = ns.research_run_id
             WHERE rr.briefing_run_id = $1
               AND rr.status = 'complete' AND rr.validation_status = 'valid'
           ) narrative_row"#,
    )
    .bind(briefing_id)
    .fetch_one(&state.pool)
    .await
}

async fn load_validated_transmission_paths(
    state: &AppState,
    briefing_id: Uuid,
) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT COALESCE(jsonb_agg(to_jsonb(path_row) ORDER BY path_row.created_at), '[]'::jsonb)
           FROM (
             SELECT tp.* FROM transmission_paths tp
             JOIN research_runs rr ON rr.id = tp.research_run_id
             WHERE rr.briefing_run_id = $1
               AND rr.status = 'complete' AND rr.validation_status = 'valid'
           ) path_row"#,
    )
    .bind(briefing_id)
    .fetch_one(&state.pool)
    .await
}

fn collect_finding_ids(value: &Value) -> HashSet<Uuid> {
    value
        .get("validated_findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|finding| finding.get("id").and_then(Value::as_str))
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect()
}

fn collect_finding_ids_by_type(value: &Value, finding_type: &str) -> HashSet<Uuid> {
    value
        .get("validated_findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|finding| finding.get("finding_type").and_then(Value::as_str) == Some(finding_type))
        .filter_map(|finding| finding.get("id").and_then(Value::as_str))
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect()
}

fn collect_position_intent_assessments(value: &Value) -> HashMap<Uuid, String> {
    value
        .get("validated_findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|finding| {
            finding.get("finding_type").and_then(Value::as_str) == Some("equity_position_diagnosis")
        })
        .filter_map(|finding| {
            let id = finding
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok())?;
            let assessment = finding
                .get("diagnostic_dimensions")
                .and_then(|analysis| analysis.get("intent_assessment"))
                .and_then(Value::as_str)?;
            Some((id, assessment.to_string()))
        })
        .collect()
}

fn is_incremental_news(published_at: Option<&str>, window_start: Option<&DateTime<Utc>>) -> bool {
    match (published_at, window_start) {
        (Some(published_at), Some(window_start)) => parse_publication_start_time(published_at)
            .is_ok_and(|published_at| published_at >= *window_start),
        _ => false,
    }
}

fn collect_deterministic_missing_items(value: &Value) -> HashMap<Uuid, DeterministicPositionItem> {
    let language = ReportLanguage::from_input(value);
    let positions = value
        .get("frozen_portfolio")
        .and_then(|portfolio| portfolio.get("positions"))
        .and_then(Value::as_array);
    value
        .get("validated_findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|finding| {
            if finding.get("finding_type").and_then(Value::as_str)
                != Some("equity_position_diagnosis")
            {
                return None;
            }
            let id = finding
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok())?;
            let dimensions = finding.get("diagnostic_dimensions")?;
            let assessment = dimensions.get("intent_assessment").and_then(Value::as_str)?;
            let affected_id = if finding.get("affected_object_type").and_then(Value::as_str)
                == Some("position_snapshot_item")
            {
                finding.get("affected_object_id").and_then(Value::as_str)
            } else {
                finding
                    .get("evidence")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .find(|evidence| {
                        evidence.get("frozen_object_type").and_then(Value::as_str)
                            == Some("position_snapshot_item")
                    })
                    .and_then(|evidence| evidence.get("frozen_object_id"))
                    .and_then(Value::as_str)
            }?;
            let position = positions?
                .iter()
                .find(|row| {
                    row.get("position_snapshot_item_id").and_then(Value::as_str)
                        == Some(affected_id)
                })?;
            let instrument_id = position.get("instrument_id").and_then(Value::as_str)?;
            let symbol = position
                .get("symbol")
                .and_then(Value::as_str)
                .unwrap_or(language.text("个股", "Stock"));
            let name = position
                .get("instrument_name")
                .and_then(Value::as_str)
                .unwrap_or(symbol);
            let text = |object: &Value, key: &str, fallback: &str| {
                object
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or(fallback)
                    .to_string()
            };
            let list = |object: &Value, key: &str, fallback: &str| {
                object
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(|item| format!("- {item}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .filter(|items| !items.is_empty())
                    .unwrap_or_else(|| format!("- {fallback}"))
            };
            let incremental_start = finding
                .get("window_start")
                .and_then(Value::as_str)
                .and_then(|value| parse_time(value).ok());
            let mut new_evidence = Vec::<(DateTime<Utc>, String)>::new();
            let mut baseline_evidence = Vec::<(DateTime<Utc>, String)>::new();
            let mut industry_evidence = Vec::<(DateTime<Utc>, String)>::new();
            for evidence in finding
                .get("evidence")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|evidence| {
                    let is_position_relevant = matches!(
                        evidence.get("coverage_scope").and_then(Value::as_str),
                        Some("company" | "product" | "industry")
                    );
                    let is_exactly_bound = evidence
                        .get("related_instrument_id")
                        .and_then(Value::as_str)
                        == Some(instrument_id);
                    is_position_relevant && is_exactly_bound
                })
            {
                let Some(title) = evidence.get("title").and_then(Value::as_str) else {
                    continue;
                };
                let publisher = evidence
                    .get("publisher")
                    .and_then(Value::as_str)
                    .unwrap_or(language.text("来源未标注", "Publisher not supplied"));
                let Some(published) = evidence.get("published_at").and_then(Value::as_str) else {
                    continue;
                };
                let Ok(published_at) = parse_publication_time(published) else {
                    continue;
                };
                let date = published
                    .get(..10)
                    .map(|value| format!(" · {value}"))
                    .unwrap_or_default();
                let rendered = match evidence.get("url").and_then(Value::as_str) {
                    Some(url) => format!("- [{title}]({url})（{publisher}{date}）"),
                    None => format!("- {title}（{publisher}{date}）"),
                };
                match evidence.get("coverage_scope").and_then(Value::as_str) {
                    Some("industry") => industry_evidence.push((published_at, rendered)),
                    _ if is_incremental_news(Some(published), incremental_start.as_ref()) => {
                        new_evidence.push((published_at, rendered));
                    }
                    _ => baseline_evidence.push((published_at, rendered)),
                }
            }
            new_evidence.sort_by(|left, right| right.0.cmp(&left.0));
            baseline_evidence.sort_by(|left, right| right.0.cmp(&left.0));
            industry_evidence.sort_by(|left, right| right.0.cmp(&left.0));
            let new_evidence = new_evidence
                .into_iter()
                .map(|(_, rendered)| rendered)
                .collect::<Vec<_>>();
            let baseline_evidence = baseline_evidence
                .into_iter()
                .map(|(_, rendered)| rendered)
                .collect::<Vec<_>>();
            let industry_evidence = industry_evidence
                .into_iter()
                .map(|(_, rendered)| rendered)
                .collect::<Vec<_>>();
            let news = if new_evidence.is_empty() && !baseline_evidence.is_empty() {
                localized_format!(language,
                    "**最近动态**\n{}\n\n_增量窗口内暂无更新；以上为截至报告时点最近可验证的公司或产品信息，并已纳入本次诊断。_", "**Latest developments**\n{}\n\n_No updates in the incremental window; the latest verified company or product information above is included in this diagnosis._",
                    baseline_evidence.join("\n")
                )
            } else {
                localized_format!(language,
                    "**本轮新增**\n{}\n\n**近期背景**\n{}", "**New this run**\n{}\n\n**Recent background**\n{}",
                    if new_evidence.is_empty() {
                        language.text("- 增量窗口内暂无更新。", "- No updates in the incremental window.").into()
                    } else {
                        new_evidence.join("\n")
                    },
                    if baseline_evidence.is_empty() {
                        language.text("- 已回溯至最近财报或年度披露，但仍未获得可验证的公司或产品基线。", "- No verifiable company or product baseline was found after searching the latest results and annual disclosures.")
                            .into()
                    } else {
                        baseline_evidence.join("\n")
                    }
                )
            };
            let industry_news = if industry_evidence.is_empty() {
                language.text("- 本报告没有获得精确绑定该标的的行业或政策动态。", "- No industry or policy developments precisely linked to this holding were verified.").to_string()
            } else {
                industry_evidence.join("\n")
            };
            let logic_section = if assessment == "missing" {
                let draft = dimensions.get("intent_draft")?;
                localized_format!(language,
                    "### 持仓逻辑草稿\n**模型草稿，待用户确认：** {}\n\n**关键催化剂**\n{}\n\n**判断失效条件**\n{}\n\n该草稿不可操作，不构成买卖或仓位调整建议。", "### Proposed holding rationale\n**Model draft, awaiting user confirmation:** {}\n\n**Key catalysts**\n{}\n\n**Invalidation conditions**\n{}\n\nThis draft is non-actionable and is not a recommendation to trade or resize a position.",
                    text(draft, "thesis", language.text("当前证据不足以代写持仓理由。", "Evidence is insufficient to propose a holding rationale.")),
                    list(draft, "catalysts", language.text("尚无明确近期催化。", "No clear near-term catalyst verified.")),
                    list(
                        draft,
                        "invalidation_conditions",
                        language.text("需要用户补充可验证的失效条件。", "The user needs to specify verifiable invalidation conditions.")
                    ),
                )
            } else {
                localized_format!(language,
                    "### 持仓逻辑与条件\n**状态：已匹配用户确认的 Trade Intent。**\n\n**确认条件**\n{}\n\n**判断失效条件**\n{}\n\n**条件式响应**\n{}", "### Holding rationale and conditions\n**Status: matched to the user-confirmed Trade Intent.**\n\n**Confirmation conditions**\n{}\n\n**Invalidation conditions**\n{}\n\n**Conditional response**\n{}",
                    list(finding, "confirmation_conditions", language.text("暂无新增确认条件。", "No new confirmation conditions.")),
                    list(finding, "invalidation_conditions", language.text("暂无新增失效条件。", "No new invalidation conditions.")),
                    text(
                        finding,
                        "conditional_action",
                        language.text("当前没有经过验证的条件式调整。", "No validated conditional adjustment at present.")
                    ),
                )
            };
            let body = localized_format!(language,
                "### 诊断结论\n{}\n\n### 公司与产品动态\n{}\n\n### 行业与政策动态\n{}\n\n### 未来预期\n{}\n\n### 预期差\n{}\n\n### 宏观环境\n{}\n\n### 行业环境\n{}\n\n### 公司层面\n{}\n\n### 市场定价\n{}\n\n### 潜在利好\n{}\n\n### 潜在利空\n{}\n\n### 核心风险\n{}\n\n{}", "### Diagnosis\n{}\n\n### Company and product developments\n{}\n\n### Industry and policy developments\n{}\n\n### Future expectations\n{}\n\n### Expectation gap\n{}\n\n### Macro context\n{}\n\n### Industry context\n{}\n\n### Company context\n{}\n\n### Market pricing\n{}\n\n### Upside catalysts\n{}\n\n### Downside pressures\n{}\n\n### Core risks\n{}\n\n{}",
                text(finding, "fact_summary", language.text("本轮证据不足，暂时无法形成明确诊断。", "Insufficient evidence for a clear diagnosis.")),
                news,
                industry_news,
                text(dimensions, "future_expectation", language.text("尚未形成可验证的未来预期。", "No verifiable future expectation established.")),
                text(dimensions, "expectation_gap", language.text("尚未识别明确预期差。", "No clear expectation gap identified.")),
                text(dimensions, "macro_context", language.text("宏观影响暂不明确。", "Macro impact remains unclear.")),
                text(dimensions, "industry_context", language.text("行业影响暂不明确。", "Industry impact remains unclear.")),
                text(dimensions, "company_context", language.text("公司层面暂无新增关键信息。", "No new material company information.")),
                text(dimensions, "market_pricing", language.text("市场定价信号暂不明确。", "Market pricing signals remain unclear.")),
                list(dimensions, "upside_catalysts", language.text("尚未识别新增利好。", "No new upside catalyst identified.")),
                list(dimensions, "downside_pressures", language.text("尚未识别新增利空。", "No new downside pressure identified.")),
                list(dimensions, "core_risks", language.text("仍需持续观察信息与价格变化。", "Continue monitoring information and price changes.")),
                logic_section,
            );
            Some((
                id,
                DeterministicPositionItem {
                    title: localized_format!(language, "{name}（{symbol}）持仓诊断", "{name} ({symbol}) holding diagnosis"),
                    body,
                },
            ))
        })
        .collect()
}

pub(crate) async fn run_codex(
    state: &AppState,
    schema: &Path,
    prompt: &str,
    enable_search: bool,
) -> Result<String, String> {
    state.research.preflight()?;
    if !schema.is_file() {
        return Err(format!("output schema is missing: {}", schema.display()));
    }
    let output_path = std::env::temp_dir().join(format!("alpha-echo-{}.json", Uuid::new_v4()));
    let mut command = Command::new(&state.research.codex_binary);
    command.env_clear();
    for key in ["HOME", "CODEX_HOME", "PATH", "TMPDIR", "LANG", "LC_ALL"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    if enable_search {
        command.arg("--search");
    }
    command
        .args(["-s", "read-only", "-a", "never", "-C"])
        .arg(&state.research.project_root)
        .arg("exec")
        .args([
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "--disable",
            "plugins",
            "--disable",
            "remote_plugin",
            "--disable",
            "apps",
            "--disable",
            "recommended_plugins",
            "--disable",
            "enable_mcp_apps",
            "--disable",
            "skill_mcp_dependency_install",
            "--disable",
            "hooks",
            "--disable",
            "multi_agent",
            "--disable",
            "multi_agent_v2",
            "--disable",
            "computer_use",
            "--disable",
            "browser_use",
            "--disable",
            "in_app_browser",
            "--color",
            "never",
            "--output-schema",
        ])
        .arg(schema)
        .arg("--output-last-message")
        .arg(&output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(model) = &state.research.model {
        command.arg("--model").arg(model);
    }
    command.arg("--config").arg(format!(
        "model_reasoning_effort=\"{}\"",
        state.research.reasoning_effort
    ));
    command.arg("-");
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start Codex: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Codex stdin is unavailable".to_string())?;
    stdin
        .write_all(prompt.as_bytes())
        .await
        .map_err(|error| format!("could not write Codex prompt: {error}"))?;
    drop(stdin);
    let result = match tokio::time::timeout(state.research.timeout, child.wait_with_output()).await
    {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            let _ = tokio::fs::remove_file(&output_path).await;
            return Err(format!("Codex process failed: {error}"));
        }
        Err(_) => {
            let _ = tokio::fs::remove_file(&output_path).await;
            return Err(format!(
                "Codex exceeded the {:?} timeout",
                state.research.timeout
            ));
        }
    };
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let message = format!(
            "Codex exited with {}: {}",
            result.status,
            tail(&stderr, 4000)
        );
        let _ = tokio::fs::remove_file(&output_path).await;
        return Err(message);
    }
    let raw = tokio::fs::read_to_string(&output_path)
        .await
        .map_err(|error| format!("Codex did not write its final output: {error}"));
    let _ = tokio::fs::remove_file(&output_path).await;
    raw
}

pub(crate) fn hash_skill(project_root: &Path, skill_name: &str) -> Result<String, String> {
    let directory = project_root.join("skills").join(skill_name);
    if !directory.join("SKILL.md").is_file() {
        return Err(format!("project Skill is missing: {skill_name}"));
    }
    let mut files = Vec::new();
    collect_files(&directory, &mut files).map_err(|error| error.to_string())?;
    files.sort();
    let mut hasher = Sha256::new();
    for file in files {
        let relative = file
            .strip_prefix(&directory)
            .map_err(|error| error.to_string())?;
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(std::fs::read(&file).map_err(|error| error.to_string())?);
        hasher.update([0]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| format!("invalid RFC3339 timestamp {value}: {error}"))
}

fn parse_publication_time(value: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Ok(value.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(23, 59, 59))
        .map(|value| value.and_utc())
        .ok_or_else(|| {
            format!("invalid publication timestamp {value}: expected RFC3339 or YYYY-MM-DD")
        })
}

fn parse_publication_start_time(value: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Ok(value.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|value| value.and_utc())
        .ok_or_else(|| {
            format!("invalid publication timestamp {value}: expected RFC3339 or YYYY-MM-DD")
        })
}

fn publication_times_match(evidence_value: &str, frozen_value: &str) -> bool {
    let evidence_date = NaiveDate::parse_from_str(evidence_value, "%Y-%m-%d").ok();
    let frozen_date = NaiveDate::parse_from_str(frozen_value, "%Y-%m-%d").ok();
    if let Some(frozen_date) = frozen_date {
        return evidence_date == Some(frozen_date);
    }
    if evidence_date.is_some() {
        let evidence_date = evidence_date.or_else(|| {
            parse_publication_time(evidence_value)
                .ok()
                .map(|value| value.date_naive())
        });
        let frozen_date = parse_publication_time(frozen_value)
            .ok()
            .map(|value| value.date_naive());
        return evidence_date.is_some() && evidence_date == frozen_date;
    }
    match (
        parse_publication_time(evidence_value),
        parse_publication_time(frozen_value),
    ) {
        (Ok(evidence), Ok(frozen)) => evidence == frozen,
        _ => false,
    }
}

fn parse_optional_publication_time(value: Option<&str>) -> Result<Option<DateTime<Utc>>, String> {
    value.map(parse_publication_time).transpose()
}

fn tail(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .rev()
        .take(max_chars)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn validation_retry_prompt(base_prompt: &str, validation_error: &str) -> String {
    format!(
        "{base_prompt}\n\nPrevious output did not pass deterministic validation. Generate a complete replacement from scratch and correct this exact contract failure:\n{}\nReturn only the corrected JSON; do not discuss the error.",
        tail(validation_error, 2_000)
    )
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[ignore = "read-only local audit replay; requires ALPHA_ECHO_REPLAY_RUN_ID and DATABASE_URL"]
    async fn replay_saved_equity_diagnosis_without_rewriting_history() {
        dotenvy::dotenv().ok();
        let run_id = Uuid::parse_str(
            &std::env::var("ALPHA_ECHO_REPLAY_RUN_ID").expect("set a saved research run ID"),
        )
        .unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&std::env::var("DATABASE_URL").unwrap())
            .await
            .unwrap();
        let mut transaction = pool.begin().await.unwrap();
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *transaction)
            .await
            .unwrap();
        let (as_of, window, input, raw): (DateTime<Utc>, DateTime<Utc>, Value, Value) = sqlx::query_as(
            "SELECT as_of, research_window_start, input_snapshot, raw_output FROM research_runs WHERE id = $1 AND skill_name = 'diagnose-equity-positions'"
        ).bind(run_id).fetch_one(&mut *transaction).await.unwrap();
        let market = match input["task_scope"]["market"].as_str().unwrap() {
            "a_share" => "a_share",
            "hk" => "hk",
            "us" => "us",
            _ => panic!("not a listed-equity run"),
        };
        let task = ResearchTask {
            market,
            run_type: "equity_position_diagnosis",
            skill_name: "diagnose-equity-positions",
            position_snapshot_item_id: input["task_scope"]["position_snapshot_item_id"]
                .as_str()
                .and_then(|id| Uuid::parse_str(id).ok()),
        };
        let known = collect_known_objects(&input).unwrap();
        let output =
            parse_and_validate_research(&raw.to_string(), task, as_of, window, &known).unwrap();
        assert_eq!(output.findings.len(), 1);
        assert_eq!(output.findings[0].finding_type, "equity_position_diagnosis");
        transaction.rollback().await.unwrap();
    }
    #[test]
    fn english_research_is_validated_in_its_frozen_language() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = as_of - chrono::Duration::hours(12);
        let mut value: Value =
            serde_json::from_str(&empty_output(&as_of.to_rfc3339(), MACRO_TASK.skill_name))
                .unwrap();
        value["evidence"] = json!([{
            "key": "official", "source_type": "official_release", "coverage_scope": "macro",
            "title": "Official policy release", "url": "https://example.com/policy", "publisher": "Official issuer",
            "published_at": window.to_rfc3339(), "observed_at": as_of.to_rfc3339(), "excerpt": null,
            "source_snapshot_id": null, "frozen_object_type": null, "frozen_object_id": null
        }]);
        value["findings"] = json!([{
            "finding_type": "macro_context", "market": "macro", "window_start": window.to_rfc3339(), "window_end": as_of.to_rfc3339(),
            "affected_object_type": "market", "affected_object_id": null, "fact_summary": "Policy conditions remain unchanged.",
            "inference": null, "impact_direction": null, "impact_strength": null, "time_horizon": null, "confidence": 0.7,
            "contrary_evidence": [], "confirmation_conditions": [], "invalidation_conditions": [], "conditional_action": null,
            "data_gaps": [], "evidence_keys": ["official"], "related_event_keys": [], "related_narrative_keys": [], "related_transmission_path_keys": [], "analysis": null
        }]);
        let known = KnownObjects {
            language: ReportLanguage::En,
            ..KnownObjects::default()
        };
        parse_and_validate_research(&value.to_string(), MACRO_TASK, as_of, window, &known).unwrap();
        assert!(
            parse_and_validate_research(
                &value.to_string(),
                MACRO_TASK,
                as_of,
                window,
                &KnownObjects::default()
            )
            .is_err()
        );
        value["findings"][0]["evidence_keys"] = json!(["invented"]);
        assert!(
            parse_and_validate_research(&value.to_string(), MACRO_TASK, as_of, window, &known)
                .is_err()
        );
        let prompt = research_prompt(
            MACRO_TASK,
            as_of,
            window,
            &json!({"frozen_portfolio": {"report_language": "en"}}),
        );
        assert!(prompt.contains("in English"));
        assert!(!prompt.contains("in Simplified Chinese"));
    }

    #[test]
    fn english_position_notes_preserve_draft_safety_and_citations() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let id = Uuid::new_v4();
        for assessment in ["missing", "matched"] {
            let input = json!({
                "frozen_portfolio": {"report_language": "en", "positions": [{
                    "position_snapshot_item_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", "instrument_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc", "symbol": "ACME", "instrument_name": "Acme"
                }]},
                "validated_findings": [{
                    "id": id, "finding_type": "equity_position_diagnosis", "affected_object_type": "position_snapshot_item", "affected_object_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                    "fact_summary": "Results support the current outlook.", "diagnostic_dimensions": {"intent_assessment": assessment, "intent_draft": {"thesis": "Await evidence of sustainable growth.", "catalysts": [], "invalidation_conditions": []}},
                    "evidence": [{"coverage_scope": "company", "related_instrument_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc", "title": "Quarterly results", "publisher": "Acme IR", "published_at": "2026-08-01T00:00:00Z", "url": "https://example.com/results"}]
                }]
            });
            let deterministic = collect_deterministic_missing_items(&input);
            let body = &deterministic[&id].body;
            assert!(body.contains("Latest developments"));
            assert!(body.contains("https://example.com/results"));
            assert!(!body.chars().any(|c| matches!(c, '\u{4e00}'..='\u{9fff}')));
            let mut raw = json!({"as_of": as_of.to_rfc3339(), "title": "Portfolio report", "executive_summary": "The latest verified holding outlook.", "data_gaps": [], "items": [{
                "section": "position_diagnosis", "title": "Acme diagnosis", "body": "Placeholder narrative", "severity": null,
                "intent_status": if assessment == "missing" {"draft_unconfirmed"} else {"active"}, "actionability": if assessment == "missing" {"non_actionable"} else {"conditional"}, "source_finding_ids": [id]
            }]});
            let ids = HashSet::from([id]);
            let assessments = HashMap::from([(id, assessment.to_string())]);
            let output = parse_and_validate_briefing_in_language(
                &raw.to_string(),
                as_of,
                &ids,
                &ids,
                &assessments,
                &deterministic,
                ReportLanguage::En,
            )
            .unwrap();
            assert_eq!(output.items[0].body, *body);
            let rendered = render_briefing_markdown_in_language(
                &output,
                &["Coverage remains limited.".into()],
                ReportLanguage::En,
            );
            assert!(rendered.contains("## Data gaps"));
            if assessment == "missing" {
                assert!(body.contains("awaiting user confirmation"));
                raw["items"][0]["actionability"] = json!("conditional");
                assert!(
                    parse_and_validate_briefing_in_language(
                        &raw.to_string(),
                        as_of,
                        &ids,
                        &ids,
                        &assessments,
                        &deterministic,
                        ReportLanguage::En
                    )
                    .is_err()
                );
            }
        }
    }
    use super::*;

    #[test]
    fn research_output_schema_is_accepted_as_a_strict_structured_output() {
        fn assert_strict_objects_are_fully_required(schema: &Value, path: &str) {
            let is_object = match schema.get("type") {
                Some(Value::String(kind)) => kind == "object",
                Some(Value::Array(kinds)) => {
                    kinds.iter().any(|kind| kind.as_str() == Some("object"))
                }
                _ => false,
            };
            if is_object
                && schema.get("additionalProperties").and_then(Value::as_bool) == Some(false)
            {
                let properties = schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .expect("strict object schema must declare properties");
                let required = schema
                    .get("required")
                    .and_then(Value::as_array)
                    .expect("strict object schema must declare required")
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<HashSet<_>>();
                let missing = properties
                    .keys()
                    .filter(|key| !required.contains(key.as_str()))
                    .cloned()
                    .collect::<Vec<_>>();
                assert!(
                    missing.is_empty(),
                    "strict object at {path} has properties missing from required: {missing:?}"
                );
            }

            match schema {
                Value::Array(values) => {
                    for (index, value) in values.iter().enumerate() {
                        assert_strict_objects_are_fully_required(
                            value,
                            &format!("{path}[{index}]"),
                        );
                    }
                }
                Value::Object(values) => {
                    for (key, value) in values {
                        assert_strict_objects_are_fully_required(value, &format!("{path}.{key}"));
                    }
                }
                _ => {}
            }
        }

        let schema: Value =
            serde_json::from_str(include_str!("../../schemas/research-output-v1.json"))
                .expect("research output schema must be valid JSON");
        assert_strict_objects_are_fully_required(&schema, "research-output-v1");
    }

    #[test]
    fn publication_dates_are_accepted_with_conservative_day_precision() {
        let parsed = parse_publication_time("2026-05-14").unwrap();
        assert_eq!(parsed.to_rfc3339(), "2026-05-14T23:59:59+00:00");
        let lower_bound = parse_publication_start_time("2026-05-14").unwrap();
        assert_eq!(lower_bound.to_rfc3339(), "2026-05-14T00:00:00+00:00");
        assert!(publication_times_match(
            "2026-05-14",
            "2026-05-14T08:30:00Z"
        ));
        assert!(!publication_times_match(
            "2026-05-14T08:30:00Z",
            "2026-05-14"
        ));
        assert!(!is_incremental_news(
            Some("2026-05-14"),
            Some(
                &DateTime::parse_from_rfc3339("2026-05-14T12:00:00Z")
                    .unwrap()
                    .into()
            )
        ));
        assert!(parse_publication_time("not-a-date").is_err());
    }

    #[test]
    fn validation_retry_prompt_explains_the_exact_contract_failure() {
        let prompt = validation_retry_prompt(
            "base research request",
            "industry evidence ev1 has no finding-linked direct transmission path",
        );
        assert!(prompt.contains("base research request"));
        assert!(prompt.contains("Generate a complete replacement from scratch"));
        assert!(prompt.contains("ev1 has no finding-linked direct transmission path"));
    }

    #[test]
    fn execution_plan_parallelizes_only_independent_market_work() {
        let frozen = json!({
            "positions": [
                { "market": "a_share", "quantity": "100", "position_snapshot_item_id": "11111111-1111-4111-8111-111111111111" },
                { "market": "us", "quantity": "10", "position_snapshot_item_id": "22222222-2222-4222-8222-222222222222" },
                { "market": "crypto", "quantity": "2" },
                { "market": "meme", "quantity": "3" }
            ]
        });
        let stages = research_stages(&frozen);
        assert_eq!(
            stages.iter().map(Vec::len).collect::<Vec<_>>(),
            [1, 2, 2, 1]
        );
        assert_eq!(stages[0][0].skill_name, "analyze-macro-context");
        assert!(
            stages[1]
                .iter()
                .all(|task| task.run_type == "equity_market_context")
        );
        assert!(
            stages[2]
                .iter()
                .all(|task| task.run_type == "equity_position_diagnosis")
        );
        assert_eq!(stages[3][0].skill_name, "assess-portfolio-construction");
        assert!(
            stages
                .iter()
                .flatten()
                .all(|task| !matches!(task.market, "crypto" | "meme"))
        );
        assert_eq!(research_task_count(&frozen), 6);
        assert_eq!(research_task_count(&json!({ "positions": [] })), 1);
    }

    #[test]
    fn execution_plan_creates_one_diagnosis_for_each_position_item() {
        let frozen = json!({
            "positions": [
                { "market": "us", "quantity": "10", "position_snapshot_item_id": "11111111-1111-4111-8111-111111111111" },
                { "market": "us", "quantity": "20", "position_snapshot_item_id": "22222222-2222-4222-8222-222222222222" },
                { "market": "hk", "quantity": "30", "position_snapshot_item_id": "33333333-3333-4333-8333-333333333333" }
            ]
        });

        let stages = research_stages(&frozen);
        assert_eq!(research_task_count(&frozen), 7);
        assert_eq!(stages[2].len(), 3);
        assert_eq!(
            stages[2]
                .iter()
                .filter_map(|task| task.position_snapshot_item_id)
                .collect::<HashSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn position_diagnosis_input_contains_only_its_scoped_holding() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let instrument = Uuid::new_v4();
        let first_account = Uuid::new_v4();
        let second_account = Uuid::new_v4();
        let task = ResearchTask {
            market: "us",
            run_type: "equity_position_diagnosis",
            skill_name: "diagnose-equity-positions",
            position_snapshot_item_id: Some(second),
        };
        let mut input = json!({
            "frozen_portfolio": {
                "positions": [
                    { "position_snapshot_item_id": first.to_string(), "instrument_id": instrument.to_string(), "account_id": first_account.to_string(), "symbol": "FIRST" },
                    { "position_snapshot_item_id": second.to_string(), "instrument_id": instrument.to_string(), "account_id": second_account.to_string(), "symbol": "SECOND" }
                ],
                "trade_intent_versions": [
                    { "trade_intent_version_id": Uuid::new_v4().to_string(), "instrument_ids": [instrument.to_string()], "account_id": first_account.to_string() },
                    { "trade_intent_version_id": Uuid::new_v4().to_string(), "instrument_ids": [instrument.to_string()], "account_id": second_account.to_string() }
                ]
            }
        });

        scope_research_input(&mut input, task);

        assert_eq!(
            input["frozen_portfolio"]["positions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            input["frozen_portfolio"]["positions"][0]["symbol"],
            "SECOND"
        );
        assert_eq!(
            input["frozen_portfolio"]["trade_intent_versions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            input["frozen_portfolio"]["trade_intent_versions"][0]["account_id"],
            second_account.to_string()
        );
    }

    #[test]
    fn research_task_input_excludes_irrelevant_raw_snapshots_and_stays_bounded() {
        let us_position = Uuid::new_v4();
        let hk_position = Uuid::new_v4();
        let us_instrument = Uuid::new_v4();
        let hk_instrument = Uuid::new_v4();
        let large_feed = "x".repeat(500_000);
        let input = json!({
            "frozen_portfolio": {
                "positions": [
                    { "position_snapshot_item_id": us_position, "instrument_id": us_instrument, "market": "us", "quantity": "10" },
                    { "position_snapshot_item_id": hk_position, "instrument_id": hk_instrument, "market": "hk", "quantity": "20" }
                ],
                "trade_intent_versions": [],
                "source_snapshots": [
                    { "source_snapshot_id": Uuid::new_v4(), "source_type": "equity_news", "as_of": "2026-08-14T10:00:00Z", "payload": { "instrument_id": us_instrument, "items": [], "feeds": [large_feed] } },
                    { "source_snapshot_id": Uuid::new_v4(), "source_type": "equity_news", "as_of": "2026-08-14T11:00:00Z", "payload": { "instrument_id": us_instrument, "items": [{"title":"latest"}], "feeds": [large_feed] } },
                    { "source_snapshot_id": Uuid::new_v4(), "source_type": "equity_news", "as_of": "2026-08-14T11:00:00Z", "payload": { "instrument_id": hk_instrument, "items": [], "feeds": [large_feed] } },
                    { "source_snapshot_id": Uuid::new_v4(), "source_type": "market_quote", "as_of": "2026-08-14T11:00:00Z", "payload": { "raw": large_feed } }
                ]
            },
            "validated_upstream_findings": [],
            "validated_upstream_events": [],
            "validated_upstream_narratives": [],
            "validated_upstream_transmission_paths": []
        });

        let mut macro_input = input.clone();
        scope_research_input(&mut macro_input, MACRO_TASK);
        assert!(
            macro_input["frozen_portfolio"]["source_snapshots"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );

        let mut market_input = input.clone();
        scope_research_input(
            &mut market_input,
            ResearchTask {
                market: "us",
                run_type: "equity_market_context",
                skill_name: "analyze-equity-market-context",
                position_snapshot_item_id: None,
            },
        );
        assert_eq!(
            market_input["frozen_portfolio"]["positions"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert!(
            market_input["frozen_portfolio"]["source_snapshots"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );

        let mut position_input = input.clone();
        scope_research_input(
            &mut position_input,
            ResearchTask {
                market: "us",
                run_type: "equity_position_diagnosis",
                skill_name: "diagnose-equity-positions",
                position_snapshot_item_id: Some(us_position),
            },
        );
        let position_sources = position_input["frozen_portfolio"]["source_snapshots"]
            .as_array()
            .unwrap();
        assert_eq!(position_sources.len(), 1);
        assert_eq!(
            position_sources[0]["payload"]["items"][0]["title"],
            "latest"
        );
        assert!(position_sources[0]["payload"].get("feeds").is_none());
        assert!(serde_json::to_string(&position_input).unwrap().len() < 750_000);

        let mut portfolio_input = input;
        scope_research_input(&mut portfolio_input, PORTFOLIO_TASK);
        assert!(
            portfolio_input["frozen_portfolio"]["source_snapshots"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );
    }

    #[test]
    fn report_snapshot_keeps_only_latest_compact_news_per_instrument() {
        let instrument = Uuid::new_v4();
        let large_feed = "x".repeat(100_000);
        let mut snapshots = json!([
            { "source_snapshot_id": Uuid::new_v4(), "source_type": "equity_news", "as_of": "2026-08-14T10:00:00Z", "payload": { "instrument_id": instrument, "items": [{"title":"old"}], "feeds": [{"provider":"google_news","url":"https://example.com/feed","items":[large_feed],"error":null}] } },
            { "source_snapshot_id": Uuid::new_v4(), "source_type": "equity_news", "as_of": "2026-08-14T11:00:00Z", "payload": { "instrument_id": instrument, "items": [{"title":"latest"}], "feeds": [{"provider":"google_news","url":"https://example.com/feed","items":[large_feed],"error":null}] } },
            { "source_snapshot_id": Uuid::new_v4(), "source_type": "market_quote", "as_of": "2026-08-14T11:00:00Z", "payload": { "raw": large_feed } },
            { "source_snapshot_id": Uuid::new_v4(), "source_type": "research_evidence", "as_of": "2026-08-14T11:00:00Z", "payload": { "raw": large_feed } }
        ]);

        compact_report_source_snapshots(&mut snapshots);

        let snapshots = snapshots.as_array().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0]["payload"]["items"][0]["title"], "latest");
        assert!(snapshots[0]["payload"].get("feeds").is_none());
        assert_eq!(snapshots[0]["payload"]["feed_status"][0]["item_count"], 1);
    }

    #[test]
    fn oversized_optional_source_payload_is_compacted_before_codex() {
        let mut input = json!({
            "frozen_portfolio": {
                "positions": [],
                "trade_intent_versions": [],
                "source_snapshots": [{
                    "source_snapshot_id": Uuid::new_v4(),
                    "source_type": "manual_adapter",
                    "as_of": "2026-08-14T11:00:00Z",
                    "payload": { "raw": "x".repeat(MAX_CODEX_PROMPT_BYTES + 100_000) }
                }]
            },
            "validated_upstream_findings": [],
            "validated_upstream_events": [],
            "validated_upstream_narratives": [],
            "validated_upstream_transmission_paths": [],
            "task_scope": { "market": "macro", "run_type": "macro_context" }
        });
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T12:00:00Z")
            .unwrap()
            .into();
        let window_start = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .into();

        bound_research_prompt_input(&mut input, MACRO_TASK, as_of, window_start).unwrap();

        assert!(
            research_prompt(MACRO_TASK, as_of, window_start, &input).len()
                <= MAX_CODEX_PROMPT_BYTES
        );
        assert_eq!(
            input["frozen_portfolio"]["source_snapshots"][0]["payload"]["truncated_for_model_input"],
            true
        );
        assert!(
            input["frozen_portfolio"]["source_snapshots"][0]["payload"]["data_gap"]
                .as_str()
                .is_some_and(|value| value.contains("实时搜索"))
        );
    }

    #[test]
    fn composition_never_repeats_raw_source_payloads() {
        let mut frozen = json!({
            "positions": [{ "position_snapshot_item_id": Uuid::new_v4() }],
            "source_snapshots": [{
                "source_snapshot_id": Uuid::new_v4(),
                "payload": { "raw": "x".repeat(MAX_CODEX_PROMPT_BYTES + 100_000) }
            }]
        });

        remove_composition_raw_sources(&mut frozen);

        assert_eq!(frozen["positions"].as_array().map(Vec::len), Some(1));
        assert!(
            frozen["source_snapshots"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );
        assert!(serde_json::to_vec(&frozen).unwrap().len() < 1_000);
    }

    #[test]
    fn equity_news_search_is_not_cut_off_by_the_incremental_window() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = as_of - chrono::Duration::hours(12);
        let task = ResearchTask {
            market: "us",
            run_type: "equity_position_diagnosis",
            skill_name: "diagnose-equity-positions",
            position_snapshot_item_id: None,
        };
        let prompt = research_prompt(task, as_of, window, &json!({}));
        assert!(prompt.contains("not a search cutoff"));
        assert!(prompt.contains("latest material company baseline"));
        assert!(prompt.contains("90 days"));
        assert!(prompt.contains("latest fiscal or annual result"));
        assert!(prompt.contains("empty incremental window is not an empty diagnosis"));
        assert!(prompt.contains("company-specific sources"));
        assert!(prompt.contains("RFC3339 date-time or YYYY-MM-DD"));
        assert!(prompt.contains("finding-linked transmission path"));
    }

    #[test]
    fn news_without_a_publication_time_is_never_marked_as_incremental() {
        let window = DateTime::parse_from_rfc3339("2026-08-13T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(!is_incremental_news(None, Some(&window)));
        assert!(!is_incremental_news(
            Some("2026-08-13T11:59:59Z"),
            Some(&window)
        ));
        assert!(is_incremental_news(
            Some("2026-08-13T12:00:00Z"),
            Some(&window)
        ));
    }

    #[test]
    fn recent_company_evidence_fills_an_empty_incremental_window() {
        let finding_id = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
        let input = json!({
            "frozen_portfolio": {
                "positions": [{
                    "position_snapshot_item_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                    "instrument_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
                    "symbol": "VRT",
                    "instrument_name": "Vertiv"
                }]
            },
            "validated_findings": [{
                "id": finding_id,
                "finding_type": "equity_position_diagnosis",
                "affected_object_type": "position_snapshot_item",
                "affected_object_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                "window_start": "2026-08-14T00:00:00Z",
                "fact_summary": "最近财报构成当前判断基线。",
                "diagnostic_dimensions": {
                    "intent_assessment": "missing",
                    "intent_draft": {
                        "thesis": "等待用户确认。",
                        "catalysts": [],
                        "invalidation_conditions": []
                    }
                },
                "evidence": [
                    {
                        "coverage_scope": "company",
                        "related_instrument_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
                        "title": "Vertiv第一季度业绩",
                        "publisher": "Vertiv Investor Relations",
                        "published_at": "2026-04-22T00:00:00Z",
                        "url": "https://example.com/vrt-q1"
                    },
                    {
                        "coverage_scope": "company",
                        "related_instrument_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
                        "title": "Vertiv公布第二季度业绩并上调指引",
                        "publisher": "Vertiv Investor Relations",
                        "published_at": "2026-07-29T00:00:00Z",
                        "url": "https://example.com/vrt-q2"
                    },
                    {
                        "coverage_scope": "company",
                        "related_instrument_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
                        "title": "发布时间未知的材料",
                        "publisher": "未知来源",
                        "published_at": null,
                        "url": null
                    }
                ]
            }]
        });

        let items = collect_deterministic_missing_items(&input);
        let body = &items.get(&finding_id).unwrap().body;
        assert!(body.contains("**最近动态**"));
        assert!(body.contains("Vertiv公布第二季度业绩并上调指引"));
        assert!(body.find("第二季度").unwrap() < body.find("第一季度").unwrap());
        assert!(!body.contains("发布时间未知的材料"));
        assert!(body.contains("已纳入本次诊断"));
        assert!(!body.contains("尚未获得足以建立"));
        assert!(has_position_news_context(body));
    }

    #[test]
    fn chinese_text_allows_official_names_and_identifiers() {
        assert!(is_chinese_text(
            "已检索CSOP Asset Management Limited产品网站、HKEX产品资料及监管资料，但缺少最新产品文件。"
        ));
        assert!(is_chinese_text(
            "缺少账户5be9a831-bd0d-4f9b-8543-ae0bb8eb0807匹配的active Trade Intent version；草案仅供用户审核。"
        ));
        assert!(!is_chinese_text(
            "This entire paragraph is written in English with only 一个汉字 token."
        ));
        assert!(!is_chinese_text(
            "这是 outlook2026 remains-weak guidance2 unchanged3 despite4 demand5 softness6"
        ));
        assert!(!is_chinese_text(
            "这是 THIS REPORT REMAINS COMPLETELY WRONG AND UNSUPPORTED"
        ));
        assert!(is_machine_identifier(
            "5be9a831-bd0d-4f9b-8543-ae0bb8eb0807"
        ));
        assert!(is_machine_identifier("3067.HK"));
        assert!(!is_machine_identifier("REPORT"));
        assert!(!is_machine_identifier("outlook2026"));
    }

    #[test]
    fn short_ticker_must_be_a_distinct_token_in_frozen_news() {
        let evidence = EvidenceOutput {
            key: "industry".into(),
            source_type: "captured_source".into(),
            coverage_scope: "industry".into(),
            related_instrument_id: Some(Uuid::new_v4().to_string()),
            relationship_basis: Some("source_names_instrument".into()),
            relationship_explanation: None,
            title: "行业动态".into(),
            url: Some("https://example.com/news".into()),
            publisher: None,
            published_at: Some("2026-08-14T00:00:00Z".into()),
            observed_at: "2026-08-14T00:00:00Z".into(),
            excerpt: None,
            source_snapshot_id: Some(Uuid::new_v4().to_string()),
            frozen_object_type: None,
            frozen_object_id: None,
        };
        let mut snapshot = FrozenSourceSnapshot {
            as_of: Utc::now(),
            source_type: "equity_news".into(),
            payload: json!({"items": [{"title": "Artificial intelligence demand rises", "url": "https://example.com/news"}]}),
        };
        let labels = InstrumentLabels {
            symbol: "AI".into(),
            name: "C3.ai".into(),
        };
        assert!(!frozen_news_source_names_instrument(
            &evidence,
            &snapshot,
            Some(&labels)
        ));
        snapshot.payload["items"][0]["title"] = json!("AI shares rise on policy news");
        assert!(frozen_news_source_names_instrument(
            &evidence,
            &snapshot,
            Some(&labels)
        ));
    }

    #[test]
    fn resume_reuses_terminal_tasks_but_retries_interrupted_work() {
        assert!(is_reusable_task_run("complete", Some("valid")));
        assert!(is_reusable_task_run("failed", Some("invalid")));
        assert!(!is_reusable_task_run("running", None));
        assert!(!is_reusable_task_run("failed", Some("interrupted")));
        assert!(!is_reusable_task_run("failed", Some("superseded")));
        assert!(is_reusable_task_version(
            "failed",
            Some("invalid"),
            "sha256:same",
            "sha256:same"
        ));
        assert!(!is_reusable_task_version(
            "failed",
            Some("invalid"),
            "sha256:old",
            "sha256:new"
        ));
        assert!(is_reusable_task_version(
            "complete",
            Some("valid"),
            "sha256:old",
            "sha256:new"
        ));
    }

    fn empty_output(as_of: &str, skill: &str) -> String {
        json!({
            "as_of": as_of,
            "skill_name": skill,
            "data_gaps": ["no supported evidence"],
            "evidence": [],
            "events": [],
            "findings": [],
            "narratives": [],
            "transmission_paths": []
        })
        .to_string()
    }

    #[test]
    fn accepts_an_explicit_no_finding_result() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = DateTime::parse_from_rfc3339("2026-08-13T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let task = MACRO_TASK;
        let output = parse_and_validate_research(
            &empty_output(&as_of.to_rfc3339(), task.skill_name),
            task,
            as_of,
            window,
            &KnownObjects::default(),
        )
        .unwrap();
        assert!(output.findings.is_empty());
    }

    #[test]
    fn equity_diagnosis_requires_all_dimensions_and_exact_position_coverage() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = as_of - chrono::Duration::hours(12);
        let position_item_id = Uuid::new_v4();
        let task = ResearchTask {
            market: "us",
            run_type: "equity_position_diagnosis",
            skill_name: "diagnose-equity-positions",
            position_snapshot_item_id: None,
        };
        let mut known = KnownObjects::default();
        known.position_snapshot_items.insert(position_item_id);
        known
            .position_item_markets
            .insert(position_item_id, "us".into());
        let mut value: Value =
            serde_json::from_str(&empty_output(&as_of.to_rfc3339(), task.skill_name)).unwrap();
        value["evidence"] = json!([{
            "key": "position", "source_type": "frozen_input", "coverage_scope": "position", "title": "Frozen position",
            "url": null, "publisher": null, "published_at": null,
            "observed_at": as_of.to_rfc3339(), "excerpt": null,
            "source_snapshot_id": null, "frozen_object_type": "position_snapshot_item",
            "frozen_object_id": position_item_id.to_string()
        }]);
        value["findings"] = json!([{
            "finding_type": "equity_position_diagnosis", "market": "us",
            "window_start": window.to_rfc3339(), "window_end": as_of.to_rfc3339(),
            "affected_object_type": "position_snapshot_item", "affected_object_id": position_item_id.to_string(),
            "fact_summary": "持仓事实已完成整理", "inference": "当前没有用户确认的持仓逻辑",
            "impact_direction": null, "impact_strength": null,
            "time_horizon": "medium", "confidence": 0.7,
            "contrary_evidence": [], "confirmation_conditions": [],
            "invalidation_conditions": [], "conditional_action": null,
            "data_gaps": ["缺少用户确认的持仓逻辑", "未找到公司级证据，已检索发行人和监管披露"], "evidence_keys": ["position"], "related_event_keys": [],
            "related_narrative_keys": [], "related_transmission_path_keys": [],
            "analysis": {
                "intent_assessment": "missing",
                "intent_draft": {
                    "thesis": "公司执行若持续兑现，当前估值可能仍有修复空间",
                    "time_horizon": "medium",
                    "catalysts": ["公司上调经营指引"],
                    "invalidation_conditions": ["核心需求连续两个季度低于预期"]
                },
                "macro_context": "利率环境暂时稳定", "industry_context": "行业需求保持平稳",
                "company_context": "公司执行符合当前计划", "market_pricing": "市场估值处于中性区间",
                "future_expectation": "未来增长有望延续", "expectation_gap": "暂未发现显著预期差",
                "upside_catalysts": ["公司上调经营指引"], "downside_pressures": ["投入成本上升"],
                "core_risks": ["终端需求突然下降"]
            }
        }]);

        parse_and_validate_research(&value.to_string(), task, as_of, window, &known).unwrap();

        // Company coverage and the exact holding remain separate from generic
        // industry context, which legitimately has no per-stock binding.
        value["evidence"].as_array_mut().unwrap().push(json!({
            "key": "sector-sales", "source_type": "external", "coverage_scope": "industry",
            "related_instrument_id": null, "relationship_basis": null, "relationship_explanation": null,
            "title": "全球半导体销售增长", "url": "https://example.com/sector-sales", "publisher": "Industry association",
            "published_at": window.to_rfc3339(), "observed_at": as_of.to_rfc3339(), "excerpt": null,
            "source_snapshot_id": null, "frozen_object_type": null, "frozen_object_id": null
        }));
        value["findings"][0]["evidence_keys"] = json!(["position", "sector-sales"]);
        parse_and_validate_research(&value.to_string(), task, as_of, window, &known).unwrap();

        let sector_index = value["evidence"].as_array().unwrap().len() - 1;
        let mut invalid = value.clone();
        invalid["evidence"][sector_index]["coverage_scope"] = json!("company");
        assert!(
            parse_and_validate_research(&invalid.to_string(), task, as_of, window, &known)
                .unwrap_err()
                .contains("related_instrument_id")
        );
        invalid["evidence"][sector_index]["coverage_scope"] = json!("product");
        assert!(
            parse_and_validate_research(&invalid.to_string(), task, as_of, window, &known)
                .unwrap_err()
                .contains("related_instrument_id")
        );
        invalid = value.clone();
        invalid["evidence"][sector_index]["relationship_basis"] = json!("direct_transmission");
        invalid["evidence"][sector_index]["relationship_explanation"] =
            json!("不能在未绑定标的时宣称个股传导关系。");
        assert!(
            parse_and_validate_research(&invalid.to_string(), task, as_of, window, &known)
                .unwrap_err()
                .contains("unbound industry")
        );
        invalid = value.clone();
        invalid["evidence"][sector_index]["related_instrument_id"] = json!(Uuid::new_v4());
        assert!(
            parse_and_validate_research(&invalid.to_string(), task, as_of, window, &known).is_err()
        );
        invalid = value.clone();
        invalid["evidence"][sector_index]["published_at"] = json!("2026-08-14T00:00:01Z");
        assert!(
            parse_and_validate_research(&invalid.to_string(), task, as_of, window, &known)
                .unwrap_err()
                .contains("publication cutoff")
        );
        invalid = value.clone();
        invalid["findings"][0]["data_gaps"] = json!(["缺少用户确认的持仓逻辑"]);
        assert!(
            parse_and_validate_research(&invalid.to_string(), task, as_of, window, &known)
                .unwrap_err()
                .contains("company/product evidence")
        );

        let draft = value["findings"][0]["analysis"]["intent_draft"].clone();
        value["findings"][0]["analysis"]["intent_draft"] = Value::Null;
        assert!(
            parse_and_validate_research(&value.to_string(), task, as_of, window, &known)
                .unwrap_err()
                .contains("requires a proposed intent draft")
        );
        value["findings"][0]["analysis"]["intent_draft"] = draft;

        value["findings"][0]["analysis"]["expectation_gap"] = json!("");
        assert!(
            parse_and_validate_research(&value.to_string(), task, as_of, window, &known)
                .unwrap_err()
                .contains("diagnostic dimensions")
        );
        value["findings"][0]["analysis"]["expectation_gap"] = json!("暂未发现显著预期差");
        value["findings"][0]["impact_direction"] = json!("unchanged");
        assert!(
            parse_and_validate_research(&value.to_string(), task, as_of, window, &known)
                .unwrap_err()
                .contains("without Trade Intent")
        );
        value["findings"][0]["impact_direction"] = Value::Null;
        value["findings"][0]["affected_object_type"] = json!("market");
        value["findings"][0]["affected_object_id"] = Value::Null;
        assert!(
            parse_and_validate_research(&value.to_string(), task, as_of, window, &known)
                .unwrap_err()
                .contains("frozen position item")
        );

        let instrument_id = Uuid::new_v4();
        known.instruments.insert(instrument_id);
        value["findings"][0]["affected_object_type"] = json!("position_snapshot_item");
        value["findings"][0]["affected_object_id"] = json!(position_item_id.to_string());
        value["evidence"].as_array_mut().unwrap().push(json!({
            "key": "industry-policy", "source_type": "external", "coverage_scope": "industry",
            "related_instrument_id": instrument_id.to_string(), "title": "无人机进口政策调整",
            "relationship_basis": "direct_transmission", "relationship_explanation": "进口关税调整会直接改变该无人机厂商的相对成本和订单竞争力。",
            "url": "https://example.com/drone-policy", "publisher": "Example News",
            "published_at": "2026-08-13T23:00:00Z", "observed_at": as_of.to_rfc3339(),
            "excerpt": "政策可能影响本土无人机厂商", "source_snapshot_id": null,
            "frozen_object_type": null, "frozen_object_id": null
        }));
        value["findings"][0]["evidence_keys"] = json!(["position", "industry-policy"]);
        value["findings"][0]["related_transmission_path_keys"] = json!(["policy-to-company"]);
        value["transmission_paths"] = json!([{
            "path_key": "policy-to-company", "source_driver": "无人机进口关税调整",
            "steps": ["进口成本变化", "本土厂商相对价格竞争力变化"],
            "target_market": "us", "target_instrument_id": instrument_id.to_string(),
            "direction": "mixed", "confidence": 0.6, "evidence_keys": ["industry-policy"]
        }]);
        parse_and_validate_research(&value.to_string(), task, as_of, window, &known).unwrap();
    }

    #[test]
    fn equity_diagnosis_matches_the_exact_owner_scoped_intent() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = as_of - chrono::Duration::hours(12);
        let position_item_id = Uuid::new_v4();
        let instrument_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let intent_version_id = Uuid::new_v4();
        let task = ResearchTask {
            market: "us",
            run_type: "equity_position_diagnosis",
            skill_name: "diagnose-equity-positions",
            position_snapshot_item_id: None,
        };
        let mut known = KnownObjects::default();
        known.position_snapshot_items.insert(position_item_id);
        known
            .position_item_markets
            .insert(position_item_id, "us".into());
        known.position_item_scopes.insert(
            position_item_id,
            FrozenPositionScope {
                instrument_id,
                account_id: Some(account_id),
                wallet_id: None,
            },
        );
        known.trade_intent_versions.insert(intent_version_id);
        known.trade_intent_version_scopes.insert(
            intent_version_id,
            FrozenIntentScope {
                instrument_ids: HashSet::from([instrument_id]),
                account_id: Some(account_id),
                wallet_id: None,
            },
        );
        let mut value: Value =
            serde_json::from_str(&empty_output(&as_of.to_rfc3339(), task.skill_name)).unwrap();
        value["evidence"] = json!([{
            "key": "position", "source_type": "frozen_input", "coverage_scope": "position", "title": "Frozen position",
            "url": null, "publisher": null, "published_at": null,
            "observed_at": as_of.to_rfc3339(), "excerpt": null,
            "source_snapshot_id": null, "frozen_object_type": "position_snapshot_item",
            "frozen_object_id": position_item_id.to_string()
        }, {
            "key": "intent", "source_type": "frozen_input", "coverage_scope": "position", "title": "Frozen Intent",
            "url": null, "publisher": null, "published_at": null,
            "observed_at": as_of.to_rfc3339(), "excerpt": null,
            "source_snapshot_id": null, "frozen_object_type": "trade_intent_version",
            "frozen_object_id": intent_version_id.to_string()
        }]);
        value["findings"] = json!([{
            "finding_type": "equity_position_diagnosis", "market": "us",
            "window_start": window.to_rfc3339(), "window_end": as_of.to_rfc3339(),
            "affected_object_type": "trade_intent_version",
            "affected_object_id": intent_version_id.to_string(),
            "fact_summary": "现有证据没有改变持仓逻辑", "inference": "持仓逻辑维持不变",
            "impact_direction": "unchanged", "impact_strength": 1,
            "time_horizon": "medium", "confidence": 0.7,
            "contrary_evidence": [], "confirmation_conditions": [],
            "invalidation_conditions": [], "conditional_action": null,
            "data_gaps": ["未找到公司级证据，已检索发行人和监管披露"], "evidence_keys": ["position", "intent"],
            "related_event_keys": [], "related_narrative_keys": [],
            "related_transmission_path_keys": [],
            "analysis": {
                "intent_assessment": "matched",
                "intent_draft": null,
                "macro_context": "利率环境暂时稳定", "industry_context": "行业需求保持平稳",
                "company_context": "公司执行符合当前计划", "market_pricing": "市场估值处于中性区间",
                "future_expectation": "未来增长有望延续", "expectation_gap": "暂未发现显著预期差",
                "upside_catalysts": [], "downside_pressures": [], "core_risks": []
            }
        }]);
        parse_and_validate_research(&value.to_string(), task, as_of, window, &known).unwrap();

        let mut wrong_scope = known;
        wrong_scope
            .trade_intent_version_scopes
            .get_mut(&intent_version_id)
            .expect("intent scope exists")
            .account_id = Some(Uuid::new_v4());
        assert!(
            parse_and_validate_research(&value.to_string(), task, as_of, window, &wrong_scope)
                .unwrap_err()
                .contains("does not match the frozen position scope")
        );
    }

    #[test]
    fn account_position_does_not_match_an_unscoped_intent() {
        let position_item_id = Uuid::new_v4();
        let instrument_id = Uuid::new_v4();
        let intent_version_id = Uuid::new_v4();
        let mut known = KnownObjects::default();
        known.position_item_scopes.insert(
            position_item_id,
            FrozenPositionScope {
                instrument_id,
                account_id: Some(Uuid::new_v4()),
                wallet_id: None,
            },
        );
        known.trade_intent_version_scopes.insert(
            intent_version_id,
            FrozenIntentScope {
                instrument_ids: HashSet::from([instrument_id]),
                account_id: None,
                wallet_id: None,
            },
        );

        assert!(known.matching_intent_versions(position_item_id).is_empty());
    }

    #[test]
    fn frozen_input_builds_exact_position_to_intent_scope_links() {
        let position_item_id = Uuid::new_v4();
        let instrument_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let intent_version_id = Uuid::new_v4();
        let input = json!({
            "frozen_portfolio": {
                "positions": [{
                    "position_snapshot_item_id": position_item_id,
                    "snapshot_id": Uuid::new_v4(),
                    "instrument_id": instrument_id,
                    "account_id": account_id,
                    "wallet_id": null,
                    "market": "us"
                }],
                "trade_intent_versions": [{
                    "trade_intent_version_id": intent_version_id,
                    "trade_intent_id": intent_id,
                    "instrument_ids": [instrument_id],
                    "account_id": account_id,
                    "wallet_id": null
                }],
                "tracked_wallets": [], "prior_narratives": [],
                "source_snapshots": [], "market_events": []
            },
            "validated_upstream_findings": [],
            "validated_upstream_narratives": []
        });
        let known = collect_known_objects(&input).unwrap();
        assert_eq!(
            known.matching_intent_versions(position_item_id),
            HashSet::from([intent_version_id])
        );
    }

    #[test]
    fn rejects_evidence_after_the_cutoff() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = DateTime::parse_from_rfc3339("2026-08-13T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let task = MACRO_TASK;
        let mut value: Value =
            serde_json::from_str(&empty_output(&as_of.to_rfc3339(), task.skill_name)).unwrap();
        value["evidence"] = json!([{
            "key": "late", "source_type": "official", "coverage_scope": "macro", "title": "Late",
            "url": "https://example.com/late", "publisher": null,
            "published_at": "2026-08-14T00:00:01Z",
            "observed_at": "2026-08-14T00:00:00Z", "excerpt": null,
            "source_snapshot_id": null, "frozen_object_type": null,
            "frozen_object_id": null
        }]);
        let error = parse_and_validate_research(
            &value.to_string(),
            task,
            as_of,
            window,
            &KnownObjects::default(),
        )
        .unwrap_err();
        assert!(error.contains("publication cutoff"));
    }

    #[test]
    fn rejects_unknown_evidence_references() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = DateTime::parse_from_rfc3339("2026-08-13T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let task = MACRO_TASK;
        let mut value: Value =
            serde_json::from_str(&empty_output(&as_of.to_rfc3339(), task.skill_name)).unwrap();
        value["findings"] = json!([{
            "finding_type": "regime", "market": "macro",
            "window_start": window.to_rfc3339(), "window_end": as_of.to_rfc3339(),
            "affected_object_type": null, "affected_object_id": null,
            "fact_summary": "没有重大变化", "inference": null,
            "impact_direction": "unchanged", "impact_strength": 0,
            "time_horizon": "short", "confidence": 0.5,
            "contrary_evidence": [], "confirmation_conditions": [],
            "invalidation_conditions": [], "conditional_action": "继续等待",
            "data_gaps": [], "evidence_keys": ["missing"], "related_event_keys": [],
            "related_narrative_keys": [], "related_transmission_path_keys": [], "analysis": null
        }]);
        let error = parse_and_validate_research(
            &value.to_string(),
            task,
            as_of,
            window,
            &KnownObjects::default(),
        )
        .unwrap_err();
        assert!(error.contains("unknown evidence key"));
    }

    #[test]
    fn rejects_external_evidence_without_a_publication_time() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = as_of - chrono::Duration::hours(12);
        let task = MACRO_TASK;
        let mut value: Value =
            serde_json::from_str(&empty_output(&as_of.to_rfc3339(), task.skill_name)).unwrap();
        value["evidence"] = json!([{
            "key": "undated", "source_type": "news", "coverage_scope": "macro", "title": "Undated",
            "url": "https://example.com/undated", "publisher": "Example",
            "published_at": null, "observed_at": as_of.to_rfc3339(), "excerpt": null,
            "source_snapshot_id": null, "frozen_object_type": null,
            "frozen_object_id": null
        }]);
        let error = parse_and_validate_research(
            &value.to_string(),
            task,
            as_of,
            window,
            &KnownObjects::default(),
        )
        .unwrap_err();
        assert!(error.contains("no publication time"));
    }

    #[test]
    fn allows_future_events_only_when_marked_as_scheduled() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = as_of - chrono::Duration::hours(12);
        let task = MACRO_TASK;
        let mut value: Value =
            serde_json::from_str(&empty_output(&as_of.to_rfc3339(), task.skill_name)).unwrap();
        value["evidence"] = json!([{
            "key": "calendar", "source_type": "official", "coverage_scope": "macro", "title": "Calendar",
            "url": "https://example.com/calendar", "publisher": "Example",
            "published_at": window.to_rfc3339(), "observed_at": as_of.to_rfc3339(),
            "excerpt": null, "source_snapshot_id": null,
            "frozen_object_type": null, "frozen_object_id": null
        }]);
        value["events"] = json!([{
            "event_key": "calendar:future", "market": "macro",
            "event_type": "scheduled_event", "is_scheduled": true,
            "occurred_at": (as_of + chrono::Duration::hours(6)).to_rfc3339(),
            "headline": "即将公布的正式事件", "facts": ["该事件已经正式排期"],
            "evidence_keys": ["calendar"]
        }]);
        value["findings"] = json!([{
            "finding_type": "event_calendar", "market": "macro",
            "window_start": window.to_rfc3339(), "window_end": as_of.to_rfc3339(),
            "affected_object_type": "market", "affected_object_id": null,
            "fact_summary": "未来有已经正式排期的事件", "inference": null,
            "impact_direction": "unchanged", "impact_strength": 0,
            "time_horizon": "today", "confidence": 1.0,
            "contrary_evidence": [], "confirmation_conditions": [],
            "invalidation_conditions": [], "conditional_action": null,
            "data_gaps": [], "evidence_keys": ["calendar"],
            "related_event_keys": ["calendar:future"],
            "related_narrative_keys": [], "related_transmission_path_keys": [], "analysis": null
        }]);
        parse_and_validate_research(
            &value.to_string(),
            task,
            as_of,
            window,
            &KnownObjects::default(),
        )
        .unwrap();

        value["events"][0]["is_scheduled"] = json!(false);
        let error = parse_and_validate_research(
            &value.to_string(),
            task,
            as_of,
            window,
            &KnownObjects::default(),
        )
        .unwrap_err();
        assert!(error.contains("violates cutoff"));
    }

    #[test]
    fn validates_affected_objects_by_declared_type() {
        let instrument_id = Uuid::new_v4();
        let mut known = KnownObjects::default();
        known.instruments.insert(instrument_id);
        let error = validate_affected_reference(
            Some("trade_intent"),
            Some(&instrument_id.to_string()),
            &known,
        )
        .unwrap_err();
        assert!(error.contains("trade_intent object"));
        assert!(
            validate_affected_reference(None, None, &known)
                .unwrap_err()
                .contains("every finding")
        );
    }

    #[test]
    fn retired_wallets_are_not_stock_research_objects() {
        let wallet_id = Uuid::new_v4();
        let tracked_id = Uuid::new_v4();
        let known = collect_known_objects(&json!({
            "frozen_portfolio": {
                "positions": [],
                "tracked_wallets": [{"wallet_id": wallet_id, "tracked_wallet_id": tracked_id}]
            }
        }))
        .unwrap();
        assert!(known.validate("wallet", wallet_id).is_err());
        assert!(known.validate("tracked_wallet", tracked_id).is_err());
    }

    #[test]
    fn stock_research_rejects_retired_markets() {
        let as_of = Utc::now();
        let window = as_of - chrono::Duration::hours(12);
        for market in ["meme", "crypto"] {
            let mut value: Value =
                serde_json::from_str(&empty_output(&as_of.to_rfc3339(), MACRO_TASK.skill_name))
                    .unwrap();
            value["findings"] = json!([{
                "finding_type": "regime", "market": market,
                "window_start": window.to_rfc3339(), "window_end": as_of.to_rfc3339(),
                "affected_object_type": "portfolio", "affected_object_id": null,
                "fact_summary": "退休市场不应进入股票研究", "inference": null,
                "impact_direction": "unchanged", "impact_strength": 0,
                "time_horizon": "short", "confidence": 0.5,
                "contrary_evidence": [], "confirmation_conditions": [],
                "invalidation_conditions": [], "conditional_action": null,
                "data_gaps": [], "evidence_keys": [], "related_event_keys": [],
                "related_narrative_keys": [], "related_transmission_path_keys": [], "analysis": null
            }]);
            let error = parse_and_validate_research(
                &value.to_string(),
                MACRO_TASK,
                as_of,
                window,
                &KnownObjects::default(),
            )
            .unwrap_err();
            assert!(error.contains("invalid finding market"));
        }
    }

    #[test]
    fn rejects_conditional_actions_without_two_sided_conditions() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = as_of - chrono::Duration::hours(12);
        let task = MACRO_TASK;
        let mut value: Value =
            serde_json::from_str(&empty_output(&as_of.to_rfc3339(), task.skill_name)).unwrap();
        value["evidence"] = json!([{
            "key": "fact", "source_type": "official", "coverage_scope": "macro", "title": "Fact",
            "url": "https://example.com/fact", "publisher": "Example",
            "published_at": window.to_rfc3339(), "observed_at": as_of.to_rfc3339(),
            "excerpt": null, "source_snapshot_id": null,
            "frozen_object_type": null, "frozen_object_id": null
        }]);
        value["findings"] = json!([{
            "finding_type": "regime", "market": "macro",
            "window_start": window.to_rfc3339(), "window_end": as_of.to_rfc3339(),
            "affected_object_type": "market", "affected_object_id": null,
            "fact_summary": "事实已获得证据支持", "inference": null,
            "impact_direction": "unchanged", "impact_strength": 0,
            "time_horizon": "short", "confidence": 0.5,
            "contrary_evidence": [], "confirmation_conditions": [],
            "invalidation_conditions": ["判断失效"], "conditional_action": "继续等待",
            "data_gaps": [], "evidence_keys": ["fact"], "related_event_keys": [],
            "related_narrative_keys": [], "related_transmission_path_keys": [], "analysis": null
        }]);
        let error = parse_and_validate_research(
            &value.to_string(),
            task,
            as_of,
            window,
            &KnownObjects::default(),
        )
        .unwrap_err();
        assert!(error.contains("requires confirmation"));
    }

    #[test]
    fn briefing_market_items_require_validated_citations() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let raw = json!({
            "as_of": as_of.to_rfc3339(), "title": "持仓报告",
            "executive_summary": "组合摘要", "data_gaps": [],
            "items": [{
                "section": "portfolio", "title": "组合风险", "body": "风险判断",
                "severity": null, "intent_status": null, "actionability": null,
                "source_finding_ids": []
            }]
        })
        .to_string();
        let error = parse_and_validate_briefing(
            &raw,
            as_of,
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap_err();
        assert!(error.contains("no validated finding citation"));

        let system_raw = json!({
            "as_of": as_of.to_rfc3339(), "title": "持仓报告",
            "executive_summary": "组合摘要", "data_gaps": [],
            "items": [{
                "section": "system", "title": "系统", "body": "不可信内容",
                "severity": null, "intent_status": null, "actionability": null,
                "source_finding_ids": [Uuid::new_v4().to_string()]
            }]
        })
        .to_string();
        assert!(
            parse_and_validate_briefing(
                &system_raw,
                as_of,
                &HashSet::new(),
                &HashSet::new(),
                &HashMap::new(),
                &HashMap::new(),
            )
            .unwrap_err()
            .contains("reserved system item")
        );
    }

    #[test]
    fn briefing_requires_one_chinese_item_for_each_position_diagnosis() {
        let as_of = DateTime::parse_from_rfc3339("2026-08-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let position_id = Uuid::new_v4();
        let valid = HashSet::from([position_id]);
        let required = HashSet::from([position_id]);
        let assessments = HashMap::from([(position_id, "missing".to_string())]);
        let deterministic = HashMap::from([(
            position_id,
            DeterministicPositionItem {
                title: "示例公司持仓诊断".into(),
                body: "诊断结论：这是模型草稿，待用户确认。该草稿不可操作。".into(),
            },
        )]);
        let raw = json!({
            "as_of": as_of.to_rfc3339(), "title": "持仓报告",
            "executive_summary": "组合摘要", "data_gaps": [],
            "items": [{
                "section": "portfolio", "title": "组合判断", "body": "内容",
                "severity": null, "intent_status": "draft_unconfirmed",
                "actionability": "non_actionable", "source_finding_ids": [position_id.to_string()]
            }]
        })
        .to_string();
        assert!(
            parse_and_validate_briefing(
                &raw,
                as_of,
                &valid,
                &required,
                &assessments,
                &deterministic,
            )
            .unwrap_err()
            .contains("position_diagnosis item")
        );

        let valid_raw = json!({
            "as_of": as_of.to_rfc3339(), "title": "持仓报告",
            "executive_summary": "组合摘要", "data_gaps": [],
            "items": [{
                "section": "position_diagnosis", "title": "示例公司 ABC",
                "body": "诊断结论：这是模型草稿，待用户确认。\n\n相关新闻：本轮没有新消息。",
                "severity": null, "intent_status": "draft_unconfirmed",
                "actionability": "non_actionable", "source_finding_ids": [position_id.to_string()]
            }]
        })
        .to_string();
        assert!(
            parse_and_validate_briefing(
                &valid_raw,
                as_of,
                &valid,
                &required,
                &assessments,
                &deterministic,
            )
            .is_ok()
        );
    }

    #[test]
    fn rendered_briefing_includes_worker_failures_added_after_composition() {
        let output = BriefingOutput {
            as_of: "2026-08-14T00:00:00Z".into(),
            title: "晚报".into(),
            executive_summary: "摘要".into(),
            data_gaps: Vec::new(),
            items: Vec::new(),
        };
        let rendered = render_briefing_markdown(
            &output,
            &["diagnose-equity-positions: research task failed".into()],
        );
        assert!(rendered.contains("## 数据缺口"));
        assert!(rendered.contains("diagnose-equity-positions: research task failed"));
    }
}
