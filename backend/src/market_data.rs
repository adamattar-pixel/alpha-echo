use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{Json, Router, extract::State, routing::post};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::{America::New_York, Asia::Shanghai};
use reqwest::Client;
use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{AppState, error::AppError};

const TENCENT_QUOTE_URL: &str = "https://qt.gtimg.cn/q=";
const REFRESH_INTERVAL: Duration = Duration::from_secs(15 * 60);
static REFRESH_LOCK: Mutex<()> = Mutex::const_new(());

pub fn router() -> Router<AppState> {
    Router::new().route("/api/market-data/refresh", post(refresh_quotes))
}

#[derive(Debug, Clone, FromRow)]
struct QuoteInstrument {
    id: Uuid,
    market: String,
    symbol: String,
    currency: Option<String>,
    exchange: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FetchedQuote {
    instrument_id: Uuid,
    market: String,
    symbol: String,
    provider_symbol: String,
    price: String,
    currency: String,
    observed_at: DateTime<Utc>,
    raw: String,
}

#[derive(Debug, Serialize)]
pub struct QuoteFailure {
    instrument_id: Uuid,
    symbol: String,
    reason: String,
}

#[derive(Debug, Serialize)]
pub struct QuoteRefreshResult {
    provider: &'static str,
    requested: usize,
    fetched: usize,
    updated_accounts: usize,
    updated_positions: usize,
    source_snapshot_id: Option<Uuid>,
    captured_at: DateTime<Utc>,
    unchanged: bool,
    failures: Vec<QuoteFailure>,
}

#[derive(Debug, FromRow)]
struct SnapshotItem {
    instrument_id: Uuid,
    quantity: String,
    average_cost: Option<String>,
    market_price: Option<String>,
    metadata: Value,
}

async fn refresh_quotes(
    State(state): State<AppState>,
) -> Result<Json<QuoteRefreshResult>, AppError> {
    Ok(Json(refresh_market_data(&state).await?))
}

pub async fn run_quote_refresher(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(REFRESH_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match refresh_market_data(&state).await {
            Ok(result) if result.updated_positions > 0 => tracing::info!(
                provider = result.provider,
                fetched = result.fetched,
                updated_accounts = result.updated_accounts,
                updated_positions = result.updated_positions,
                "refreshed portfolio market prices"
            ),
            Ok(result) if !result.failures.is_empty() => tracing::warn!(
                requested = result.requested,
                fetched = result.fetched,
                failures = result.failures.len(),
                "market price refresh completed with gaps"
            ),
            Ok(_) => {}
            Err(error) => tracing::error!(%error, "market price refresh failed"),
        }
    }
}

pub async fn refresh_market_data(state: &AppState) -> Result<QuoteRefreshResult, AppError> {
    let _guard = REFRESH_LOCK.lock().await;
    let captured_at = Utc::now();
    let instruments = load_current_listed_instruments(state).await?;
    let requested = instruments.len();
    if instruments.is_empty() {
        return Ok(QuoteRefreshResult {
            provider: "tencent_quote",
            requested,
            fetched: 0,
            updated_accounts: 0,
            updated_positions: 0,
            source_snapshot_id: None,
            captured_at,
            unchanged: false,
            failures: Vec::new(),
        });
    }

    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(8))
        .user_agent("Alpha-Echo/0.1 local-portfolio")
        .build()
        .map_err(|error| AppError::Internal(format!("cannot build quote client: {error}")))?;
    let mut quotes = Vec::new();
    let mut failures = Vec::new();
    for instrument in instruments {
        match fetch_tencent_quote(&client, &instrument).await {
            Ok(quote) => quotes.push(quote),
            Err(reason) => failures.push(QuoteFailure {
                instrument_id: instrument.id,
                symbol: instrument.symbol,
                reason,
            }),
        }
    }

    if quotes.is_empty() {
        return Ok(QuoteRefreshResult {
            provider: "tencent_quote",
            requested,
            fetched: 0,
            updated_accounts: 0,
            updated_positions: 0,
            source_snapshot_id: None,
            captured_at,
            unchanged: false,
            failures,
        });
    }

    let payload = json!({ "quotes": quotes, "failures": failures });
    let checksum = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&payload)
                .map_err(|error| AppError::Internal(format!("cannot serialize quotes: {error}")))?
        )
    );
    let previous: Option<(Uuid, String)> = sqlx::query_as(
        r#"SELECT id, checksum FROM source_snapshots
           WHERE provider = 'tencent_quote' AND source_type = 'market_quote'
           ORDER BY captured_at DESC LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await?;
    let unchanged = previous
        .as_ref()
        .is_some_and(|(_, value)| value == &checksum);
    let source_snapshot_id = if unchanged {
        previous
            .as_ref()
            .map(|(id, _)| *id)
            .expect("an unchanged checksum requires a previous snapshot")
    } else {
        let source_as_of = quotes
            .iter()
            .map(|quote| quote.observed_at)
            .max()
            .unwrap_or(captured_at)
            .min(captured_at);
        let provider_symbols: Vec<&str> = quotes
            .iter()
            .map(|quote| quote.provider_symbol.as_str())
            .collect();
        sqlx::query_scalar(
            r#"INSERT INTO source_snapshots
               (provider, source_type, captured_at, as_of, request, payload, checksum)
               VALUES ('tencent_quote', 'market_quote', $1, $2, $3, $4, $5)
               RETURNING id"#,
        )
        .bind(captured_at)
        .bind(source_as_of)
        .bind(json!({ "symbols": provider_symbols }))
        .bind(&payload)
        .bind(&checksum)
        .fetch_one(&state.pool)
        .await?
    };

    let quote_map: HashMap<Uuid, &FetchedQuote> = quotes
        .iter()
        .map(|quote| (quote.instrument_id, quote))
        .collect();
    let account_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"WITH ranked AS (
             SELECT id, account_id, row_number() OVER (
               PARTITION BY account_id ORDER BY as_of DESC, created_at DESC, id DESC
             ) AS rank
             FROM position_snapshots WHERE account_id IS NOT NULL
           )
           SELECT DISTINCT r.account_id
           FROM ranked r
           JOIN position_snapshot_items psi ON psi.snapshot_id = r.id
           WHERE r.rank = 1 AND psi.quantity <> 0 AND psi.instrument_id = ANY($1)"#,
    )
    .bind(
        quotes
            .iter()
            .map(|quote| quote.instrument_id)
            .collect::<Vec<_>>(),
    )
    .fetch_all(&state.pool)
    .await?;

    let mut updated_accounts = 0;
    let mut updated_positions = 0;
    for account_id in account_ids {
        let mut transaction = state.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("position-snapshot:account:{account_id}"))
            .execute(&mut *transaction)
            .await?;
        let latest_snapshot: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
            r#"SELECT id, source_snapshot_id FROM position_snapshots WHERE account_id = $1
               ORDER BY as_of DESC, created_at DESC, id DESC LIMIT 1"#,
        )
        .bind(account_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some((latest_snapshot_id, latest_source_snapshot_id)) = latest_snapshot else {
            transaction.rollback().await?;
            continue;
        };
        if !source_needs_application(latest_source_snapshot_id, source_snapshot_id) {
            transaction.rollback().await?;
            continue;
        }
        let items = sqlx::query_as::<_, SnapshotItem>(
            r#"SELECT instrument_id, quantity::text, average_cost::text,
                      market_price::text, metadata
               FROM position_snapshot_items WHERE snapshot_id = $1
               ORDER BY instrument_id"#,
        )
        .bind(latest_snapshot_id)
        .fetch_all(&mut *transaction)
        .await?;
        if !items
            .iter()
            .any(|item| quote_map.contains_key(&item.instrument_id))
        {
            transaction.rollback().await?;
            continue;
        }
        let snapshot_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO position_snapshots
               (account_id, as_of, source, source_snapshot_id)
               VALUES ($1, $2, 'market_quote', $3) RETURNING id"#,
        )
        .bind(account_id)
        .bind(captured_at)
        .bind(source_snapshot_id)
        .fetch_one(&mut *transaction)
        .await?;

        for item in items {
            let quote = quote_map.get(&item.instrument_id).copied();
            let market_price = quote
                .map(|value| value.price.as_str())
                .or(item.market_price.as_deref());
            let mut metadata = item.metadata.as_object().cloned().unwrap_or_else(Map::new);
            if let Some(quote) = quote {
                metadata.insert("quote_provider".into(), json!("tencent_quote"));
                metadata.insert("quote_symbol".into(), json!(quote.provider_symbol));
                metadata.insert("quote_observed_at".into(), json!(quote.observed_at));
                updated_positions += 1;
            }
            sqlx::query(
                r#"INSERT INTO position_snapshot_items
                   (snapshot_id, instrument_id, quantity, average_cost, market_price,
                    market_value, unrealized_pnl, metadata)
                   VALUES ($1, $2, $3::numeric, $4::numeric, $5::numeric,
                           CASE WHEN $5::numeric IS NULL THEN NULL ELSE $3::numeric * $5::numeric END,
                           CASE WHEN $4::numeric IS NULL OR $5::numeric IS NULL THEN NULL
                                ELSE $3::numeric * ($5::numeric - $4::numeric) END,
                           $6)"#,
            )
            .bind(snapshot_id)
            .bind(item.instrument_id)
            .bind(item.quantity)
            .bind(item.average_cost)
            .bind(market_price)
            .bind(Value::Object(metadata))
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        updated_accounts += 1;
    }

    Ok(QuoteRefreshResult {
        provider: "tencent_quote",
        requested,
        fetched: quotes.len(),
        updated_accounts,
        updated_positions,
        source_snapshot_id: Some(source_snapshot_id),
        captured_at,
        unchanged,
        failures,
    })
}

async fn load_current_listed_instruments(
    state: &AppState,
) -> Result<Vec<QuoteInstrument>, AppError> {
    Ok(sqlx::query_as::<_, QuoteInstrument>(
        r#"WITH ranked AS (
             SELECT id, account_id, row_number() OVER (
               PARTITION BY account_id ORDER BY as_of DESC, created_at DESC, id DESC
             ) AS rank
             FROM position_snapshots WHERE account_id IS NOT NULL
           )
           SELECT DISTINCT i.id, i.market, i.symbol, i.currency, i.exchange
           FROM ranked r
           JOIN position_snapshot_items psi ON psi.snapshot_id = r.id
           JOIN instruments i ON i.id = psi.instrument_id
           WHERE r.rank = 1 AND psi.quantity <> 0
             AND i.market IN ('a_share', 'hk', 'us')
           ORDER BY i.market, i.symbol"#,
    )
    .fetch_all(&state.pool)
    .await?)
}

async fn fetch_tencent_quote(
    client: &Client,
    instrument: &QuoteInstrument,
) -> Result<FetchedQuote, String> {
    let provider_symbol = tencent_symbol(
        &instrument.market,
        &instrument.symbol,
        instrument.exchange.as_deref(),
    )?;
    let response = client
        .get(format!("{TENCENT_QUOTE_URL}{provider_symbol}"))
        .send()
        .await
        .map_err(|error| format!("quote request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("quote provider returned {}", response.status()));
    }
    let body = String::from_utf8_lossy(
        &response
            .bytes()
            .await
            .map_err(|error| format!("cannot read quote response: {error}"))?,
    )
    .into_owned();
    parse_tencent_quote(instrument, provider_symbol, body)
}

fn tencent_symbol(market: &str, symbol: &str, exchange: Option<&str>) -> Result<String, String> {
    let symbol = symbol.trim().to_ascii_uppercase();
    match market {
        "us" => Ok(format!("us{symbol}")),
        "hk" => {
            let number = symbol
                .parse::<u32>()
                .map_err(|_| "Hong Kong symbols must be numeric".to_string())?;
            Ok(format!("hk{number:05}"))
        }
        "a_share" => {
            let prefix = match exchange.map(|value| value.to_ascii_uppercase()) {
                Some(value) if matches!(value.as_str(), "SH" | "SSE" | "XSHG") => "sh",
                Some(value) if matches!(value.as_str(), "SZ" | "SZSE" | "XSHE") => "sz",
                Some(value) if matches!(value.as_str(), "BJ" | "BSE" | "XBEI") => "bj",
                _ if symbol.starts_with("920") => "bj",
                _ if symbol.starts_with(['5', '6', '9']) => "sh",
                _ if symbol.starts_with(['0', '1', '2', '3']) => "sz",
                _ if symbol.starts_with(['4', '8']) => "bj",
                _ => return Err("cannot infer A-share exchange from symbol".into()),
            };
            Ok(format!("{prefix}{symbol}"))
        }
        _ => Err("automatic quotes are only available for A-share, HK and US listings".into()),
    }
}

fn parse_tencent_quote(
    instrument: &QuoteInstrument,
    provider_symbol: String,
    raw: String,
) -> Result<FetchedQuote, String> {
    let start = raw.find('"').ok_or("invalid quote response")? + 1;
    let end = raw.rfind('"').ok_or("invalid quote response")?;
    if start >= end {
        return Err("empty quote response".into());
    }
    let fields: Vec<&str> = raw[start..end].split('~').collect();
    let price = fields.get(3).copied().unwrap_or_default().trim();
    let numeric_price = price
        .parse::<f64>()
        .map_err(|_| "quote has no numeric current price".to_string())?;
    if !numeric_price.is_finite() || numeric_price <= 0.0 {
        return Err("quote has no positive current price".into());
    }
    let observed_at = parse_quote_time(
        &instrument.market,
        fields.get(30).copied().unwrap_or_default(),
    )?;
    let currency =
        instrument
            .currency
            .clone()
            .unwrap_or_else(|| match instrument.market.as_str() {
                "a_share" => "RMB".into(),
                "hk" => "HKD".into(),
                _ => "USD".into(),
            });
    Ok(FetchedQuote {
        instrument_id: instrument.id,
        market: instrument.market.clone(),
        symbol: instrument.symbol.clone(),
        provider_symbol,
        price: price.into(),
        currency,
        observed_at,
        raw,
    })
}

fn parse_quote_time(market: &str, value: &str) -> Result<DateTime<Utc>, String> {
    let formats = ["%Y%m%d%H%M%S", "%Y-%m-%d %H:%M:%S", "%Y/%m/%d %H:%M:%S"];
    let naive = formats
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(value.trim(), format).ok())
        .ok_or_else(|| "quote has no valid observation time".to_string())?;
    let local = if market == "us" {
        New_York.from_local_datetime(&naive).single()
    } else {
        Shanghai.from_local_datetime(&naive).single()
    }
    .ok_or_else(|| "quote observation time is ambiguous".to_string())?;
    Ok(local.with_timezone(&Utc))
}

fn source_needs_application(
    latest_source_snapshot_id: Option<Uuid>,
    quote_source_id: Uuid,
) -> bool {
    latest_source_snapshot_id != Some(quote_source_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instrument(market: &str, symbol: &str, currency: &str) -> QuoteInstrument {
        QuoteInstrument {
            id: Uuid::nil(),
            market: market.into(),
            symbol: symbol.into(),
            currency: Some(currency.into()),
            exchange: None,
        }
    }

    #[test]
    fn maps_listed_symbols_to_provider_identity() {
        assert_eq!(tencent_symbol("us", "onds", None).unwrap(), "usONDS");
        assert_eq!(tencent_symbol("hk", "700", None).unwrap(), "hk00700");
        assert_eq!(
            tencent_symbol("a_share", "600519", None).unwrap(),
            "sh600519"
        );
        assert_eq!(
            tencent_symbol("a_share", "000001", None).unwrap(),
            "sz000001"
        );
        assert_eq!(
            tencent_symbol("a_share", "920001", None).unwrap(),
            "bj920001"
        );
        assert!(tencent_symbol("crypto", "BTC", None).is_err());
    }

    #[test]
    fn parses_a_real_us_quote_fixture_with_exchange_time() {
        let raw = "v_usONDS=\"200~Ondas Inc~ONDS.OQ~8.91~9.77~8.98~125170089~0~0~9.04~3000~0~0~0~0~0~0~0~0~9.05~600~0~0~0~0~0~0~0~0~~2026-08-13 16:00:01~-0.86~-8.80~9.55~8.79~USD\";".to_string();
        let quote =
            parse_tencent_quote(&instrument("us", "ONDS", "USD"), "usONDS".into(), raw).unwrap();
        assert_eq!(quote.price, "8.91");
        assert_eq!(quote.currency, "USD");
        assert_eq!(quote.observed_at.to_rfc3339(), "2026-08-13T20:00:01+00:00");
    }

    #[test]
    fn rejects_a_quote_without_a_price() {
        let raw = "v_usNONE=\"0~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~\";".to_string();
        assert!(
            parse_tencent_quote(&instrument("us", "NONE", "USD"), "usNONE".into(), raw).is_err()
        );
    }

    #[test]
    fn reapplies_an_unchanged_quote_after_a_trade_snapshot() {
        let quote_source_id = Uuid::new_v4();
        assert!(source_needs_application(None, quote_source_id));
        assert!(source_needs_application(
            Some(Uuid::new_v4()),
            quote_source_id
        ));
        assert!(!source_needs_application(
            Some(quote_source_id),
            quote_source_id
        ));
    }
}
