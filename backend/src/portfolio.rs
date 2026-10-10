use std::collections::HashSet;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    AppState,
    domain::{
        CreateSnapshotInput, Market, SnapshotItemInput, normalize_chain_contract,
        validate_snapshot_input,
    },
    error::AppError,
    portfolio_csv::{ParsedPositionRow, parse_position_csv},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/dashboard", get(dashboard))
        .route("/api/accounts", get(list_accounts).post(create_account))
        .route(
            "/api/instruments",
            get(list_instruments).post(create_instrument),
        )
        .route("/api/trades", get(list_trades).post(record_trade))
        .route("/api/trades/{id}/void", post(void_trade))
        .route("/api/portfolio/positions", get(list_current_positions))
        .route(
            "/api/accounts/{account_id}/positions",
            post(add_account_position),
        )
        .route("/api/portfolio/changes", get(list_position_changes))
        .route("/api/portfolio/snapshots", post(create_snapshot))
        .route("/api/portfolio/imports/csv", post(import_position_csv))
        .route(
            "/api/trade-intents",
            get(list_trade_intents).post(create_trade_intent),
        )
        .route(
            "/api/trade-intents/{id}/versions",
            post(create_trade_intent_version),
        )
}

#[derive(Debug, Serialize, FromRow)]
struct Account {
    id: Uuid,
    name: String,
    market: String,
    currency: String,
    broker: Option<String>,
    is_active: bool,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateAccount {
    name: String,
    market: Market,
    broker: Option<String>,
}

fn default_account_currency(market: Market) -> Option<&'static str> {
    match market {
        Market::AShare => Some("RMB"),
        Market::Hk => Some("HKD"),
        Market::Us => Some("USD"),
        Market::Crypto | Market::Meme => None,
    }
}

async fn list_accounts(State(state): State<AppState>) -> Result<Json<Vec<Account>>, AppError> {
    let accounts = sqlx::query_as::<_, Account>(
        "SELECT id, name, market, currency, broker, is_active, created_at FROM accounts WHERE is_active ORDER BY created_at",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(accounts))
}

async fn create_account(
    State(state): State<AppState>,
    Json(input): Json<CreateAccount>,
) -> Result<Json<Account>, AppError> {
    if input.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let currency = default_account_currency(input.market).ok_or_else(|| {
        AppError::BadRequest("trading accounts support only a_share, hk and us".into())
    })?;
    let account = sqlx::query_as::<_, Account>(
        r#"INSERT INTO accounts (name, market, currency, broker)
           VALUES ($1, $2, upper($3), $4)
           RETURNING id, name, market, currency, broker, is_active, created_at"#,
    )
    .bind(input.name.trim())
    .bind(input.market.to_string())
    .bind(currency)
    .bind(input.broker)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(account))
}

#[derive(Debug, Serialize, FromRow)]
struct Instrument {
    id: Uuid,
    market: String,
    symbol: String,
    name: String,
    currency: Option<String>,
    exchange: Option<String>,
    chain: Option<String>,
    contract_address: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateInstrument {
    market: Market,
    symbol: String,
    name: String,
    currency: Option<String>,
    exchange: Option<String>,
    chain: Option<String>,
    contract_address: Option<String>,
}

async fn list_instruments(
    State(state): State<AppState>,
) -> Result<Json<Vec<Instrument>>, AppError> {
    let instruments = sqlx::query_as::<_, Instrument>(
        "SELECT id, market, symbol, name, currency, exchange, chain, contract_address FROM instruments ORDER BY market, symbol",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(instruments))
}

fn validate_new_instrument_market(market: Market) -> Result<(), AppError> {
    if market == Market::Meme {
        return Err(AppError::BadRequest(
            "Meme tracking has been removed".into(),
        ));
    }
    Ok(())
}

async fn create_instrument(
    State(state): State<AppState>,
    Json(input): Json<CreateInstrument>,
) -> Result<Json<Instrument>, AppError> {
    validate_new_instrument_market(input.market)?;
    if input.symbol.trim().is_empty() || input.name.trim().is_empty() {
        return Err(AppError::BadRequest("symbol and name are required".into()));
    }

    let currency = default_account_currency(input.market)
        .map(str::to_string)
        .or_else(|| {
            input
                .currency
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_ascii_uppercase)
        });
    let uses_chain_identity = input.market == Market::Crypto;
    let chain = uses_chain_identity
        .then(|| input.chain.map(|value| value.trim().to_ascii_lowercase()))
        .flatten()
        .filter(|value| !value.is_empty());
    let contract_address = uses_chain_identity
        .then(|| {
            input.contract_address.as_deref().map(|value| {
                chain
                    .as_deref()
                    .map(|chain| normalize_chain_contract(chain, value))
                    .unwrap_or_else(|| value.trim().to_string())
            })
        })
        .flatten()
        .filter(|value| !value.is_empty());
    let instrument = sqlx::query_as::<_, Instrument>(
        r#"INSERT INTO instruments (market, symbol, name, currency, exchange, chain, contract_address)
           VALUES ($1, upper($2), $3, upper($4), $5, $6, $7)
           RETURNING id, market, symbol, name, currency, exchange, chain, contract_address"#,
    )
    .bind(input.market.to_string())
    .bind(input.symbol.trim())
    .bind(input.name.trim())
    .bind(currency)
    .bind(input.exchange.map(|value| value.trim().to_ascii_uppercase()))
    .bind(chain)
    .bind(contract_address)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(instrument))
}

#[derive(Debug, Deserialize)]
struct RecordTradeInput {
    account_id: Uuid,
    instrument_id: Option<Uuid>,
    symbol: Option<String>,
    name: Option<String>,
    side: String,
    quantity: String,
    price: String,
    executed_at: Option<DateTime<Utc>>,
}

fn validate_record_trade(input: &RecordTradeInput) -> Result<(), AppError> {
    let has_symbol = input
        .symbol
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    if input.instrument_id.is_some() == has_symbol {
        return Err(AppError::BadRequest(
            "provide either instrument_id or symbol".into(),
        ));
    }
    if !matches!(input.side.as_str(), "buy" | "sell") {
        return Err(AppError::BadRequest("side must be buy or sell".into()));
    }
    for (label, value) in [("quantity", &input.quantity), ("price", &input.price)] {
        let number = value
            .parse::<f64>()
            .map_err(|_| AppError::BadRequest(format!("{label} must be numeric")))?;
        if !number.is_finite() || number <= 0.0 {
            return Err(AppError::BadRequest(format!(
                "{label} must be finite and positive"
            )));
        }
    }
    Ok(())
}

fn trade_follows_latest_snapshot(
    executed_at: DateTime<Utc>,
    latest_snapshot_at: Option<DateTime<Utc>>,
) -> bool {
    latest_snapshot_at.is_none_or(|latest| executed_at > latest)
}

fn reference_price_after_trade(
    current_market_price: Option<String>,
    _execution_price: &str,
) -> Option<String> {
    current_market_price
}

#[derive(Debug, Serialize, FromRow)]
struct TradeView {
    id: Uuid,
    owner_id: Uuid,
    owner_name: String,
    instrument_id: Uuid,
    market: String,
    currency: Option<String>,
    symbol: String,
    instrument_name: String,
    side: String,
    quantity: String,
    price: Option<String>,
    realized_pnl: Option<String>,
    position_quantity_after: Option<String>,
    average_cost_after: Option<String>,
    executed_at: DateTime<Utc>,
    voided_at: Option<DateTime<Utc>>,
    can_void: bool,
}

#[derive(Debug, Deserialize)]
struct ListTradesQuery {
    limit: Option<i64>,
}

async fn list_trades(
    State(state): State<AppState>,
    Query(query): Query<ListTradesQuery>,
) -> Result<Json<Vec<TradeView>>, AppError> {
    let limit = query.limit.unwrap_or(30).clamp(1, 100);
    let trades = sqlx::query_as::<_, TradeView>(
        r#"SELECT t.id, t.account_id AS owner_id, a.name AS owner_name,
                  t.instrument_id, i.market, COALESCE(t.currency, i.currency) AS currency,
                  i.symbol, i.name AS instrument_name, t.side,
                  t.quantity::text, t.price::text, t.realized_pnl::text,
                  t.position_quantity_after::text, t.average_cost_after::text,
                  t.executed_at, t.voided_at,
                  (t.voided_at IS NULL AND t.source = 'web_manual' AND NOT EXISTS (
                     SELECT 1 FROM trades newer
                     WHERE newer.account_id = t.account_id
                       AND newer.side IN ('buy', 'sell')
                       AND newer.voided_at IS NULL
                       AND (newer.executed_at, newer.created_at, newer.id) >
                           (t.executed_at, t.created_at, t.id)
                  )) AS can_void
           FROM trades t
           JOIN instruments i ON i.id = t.instrument_id
           JOIN accounts a ON a.id = t.account_id
           WHERE t.side IN ('buy', 'sell') AND i.market IN ('a_share', 'hk', 'us')
           ORDER BY t.executed_at DESC, t.created_at DESC
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(trades))
}

#[derive(Debug, Deserialize)]
struct VoidTradeInput {
    reason: Option<String>,
}

async fn void_trade(
    State(state): State<AppState>,
    Path(trade_id): Path<Uuid>,
    Json(input): Json<VoidTradeInput>,
) -> Result<Json<Value>, AppError> {
    let account_id: Option<Uuid> =
        sqlx::query_scalar("SELECT account_id FROM trades WHERE id = $1 AND source = 'web_manual'")
            .bind(trade_id)
            .fetch_optional(&state.pool)
            .await?
            .flatten();
    let account_id = account_id.ok_or(AppError::NotFound)?;

    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("position-snapshot:account:{account_id}"))
        .execute(&mut *transaction)
        .await?;
    let target: Option<(DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"SELECT executed_at, voided_at FROM trades
           WHERE id = $1 AND account_id = $2 AND source = 'web_manual'
             AND side IN ('buy', 'sell') FOR UPDATE"#,
    )
    .bind(trade_id)
    .bind(account_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let (executed_at, voided_at) = target.ok_or(AppError::NotFound)?;
    if voided_at.is_some() {
        return Err(AppError::BadRequest("trade is already voided".into()));
    }
    let latest_trade_id: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM trades
           WHERE account_id = $1 AND side IN ('buy', 'sell') AND voided_at IS NULL
           ORDER BY executed_at DESC, created_at DESC, id DESC LIMIT 1"#,
    )
    .bind(account_id)
    .fetch_optional(&mut *transaction)
    .await?;
    if latest_trade_id != Some(trade_id) {
        return Err(AppError::BadRequest(
            "only the latest active trade in an account can be voided".into(),
        ));
    }

    let latest_snapshot: Option<(Uuid, DateTime<Utc>)> = sqlx::query_as(
        r#"SELECT id, as_of FROM position_snapshots WHERE account_id = $1
           ORDER BY as_of DESC, created_at DESC, id DESC LIMIT 1"#,
    )
    .bind(account_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let (latest_snapshot_id, latest_snapshot_at) = latest_snapshot
        .ok_or_else(|| AppError::Internal("the trade has no derived position snapshot".into()))?;
    let previous_snapshot_id: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM position_snapshots
           WHERE account_id = $1 AND as_of < $2
           ORDER BY as_of DESC, created_at DESC, id DESC LIMIT 1"#,
    )
    .bind(account_id)
    .bind(executed_at)
    .fetch_optional(&mut *transaction)
    .await?;
    let now = Utc::now();
    let corrected_at = if now > latest_snapshot_at {
        now
    } else {
        latest_snapshot_at + chrono::Duration::microseconds(1)
    };
    let corrected_snapshot_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO position_snapshots (account_id, as_of, source)
           VALUES ($1, $2, 'trade_void') RETURNING id"#,
    )
    .bind(account_id)
    .bind(corrected_at)
    .fetch_one(&mut *transaction)
    .await?;
    if let Some(previous_snapshot_id) = previous_snapshot_id {
        sqlx::query(
            r#"INSERT INTO position_snapshot_items
               (snapshot_id, instrument_id, quantity, average_cost, market_price,
                market_value, unrealized_pnl, metadata)
               SELECT $1, previous.instrument_id, previous.quantity, previous.average_cost,
                      COALESCE(current.market_price, previous.market_price),
                      CASE WHEN COALESCE(current.market_price, previous.market_price) IS NULL
                           THEN NULL ELSE previous.quantity * COALESCE(current.market_price, previous.market_price) END,
                      CASE WHEN previous.average_cost IS NULL
                                OR COALESCE(current.market_price, previous.market_price) IS NULL
                           THEN NULL ELSE previous.quantity *
                                (COALESCE(current.market_price, previous.market_price) - previous.average_cost) END,
                      COALESCE(current.metadata, previous.metadata)
               FROM position_snapshot_items previous
               LEFT JOIN position_snapshot_items current
                 ON current.snapshot_id = $2
                AND current.instrument_id = previous.instrument_id
               WHERE previous.snapshot_id = $3"#,
        )
        .bind(corrected_snapshot_id)
        .bind(latest_snapshot_id)
        .bind(previous_snapshot_id)
        .execute(&mut *transaction)
        .await?;
    }
    let reason = input
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("用户撤销错误录入");
    sqlx::query("UPDATE trades SET voided_at = now(), void_reason = $2 WHERE id = $1")
        .bind(trade_id)
        .bind(reason)
        .execute(&mut *transaction)
        .await?;
    let item_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM position_snapshot_items WHERE snapshot_id = $1")
            .bind(corrected_snapshot_id)
            .fetch_one(&mut *transaction)
            .await?;
    transaction.commit().await?;
    Ok(Json(json!({
        "id": trade_id,
        "voided": true,
        "snapshot_id": corrected_snapshot_id,
        "item_count": item_count,
    })))
}

async fn record_trade(
    State(state): State<AppState>,
    Json(input): Json<RecordTradeInput>,
) -> Result<Json<Value>, AppError> {
    validate_record_trade(&input)?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("position-snapshot:account:{}", input.account_id))
        .execute(&mut *transaction)
        .await?;
    // For Web trades without an explicit execution time, take the timestamp
    // after acquiring the owner lock so a concurrent quote refresh cannot make
    // the just-entered trade older than the latest snapshot.
    let executed_at = input.executed_at.unwrap_or_else(Utc::now);
    if executed_at > Utc::now() + chrono::Duration::minutes(5) {
        return Err(AppError::BadRequest(
            "executed_at cannot be in the future".into(),
        ));
    }
    let account: Option<(String, String)> = sqlx::query_as(
        "SELECT market, currency FROM accounts WHERE id = $1 AND is_active FOR UPDATE",
    )
    .bind(input.account_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let (market, currency) = account.ok_or(AppError::NotFound)?;

    let latest_snapshot_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT max(as_of) FROM position_snapshots WHERE account_id = $1")
            .bind(input.account_id)
            .fetch_one(&mut *transaction)
            .await?;
    if !trade_follows_latest_snapshot(executed_at, latest_snapshot_at) {
        return Err(AppError::BadRequest(
            "a trade must be recorded after the latest position update".into(),
        ));
    }

    let (instrument_id, instrument_created) = if let Some(instrument_id) = input.instrument_id {
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM instruments WHERE id = $1 AND market = $2)",
        )
        .bind(instrument_id)
        .bind(&market)
        .fetch_one(&mut *transaction)
        .await?;
        if !valid {
            return Err(AppError::BadRequest(
                "instrument does not belong to the account market".into(),
            ));
        }
        (instrument_id, false)
    } else {
        let symbol = input
            .symbol
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_uppercase();
        if !supports_quick_instrument_creation(&market) {
            return Err(AppError::BadRequest(
                "new Crypto and Meme instruments require chain identity setup".into(),
            ));
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("listed-instrument:{market}:{symbol}"))
            .execute(&mut *transaction)
            .await?;
        let matches: Vec<Uuid> = sqlx::query_scalar(
            r#"SELECT id FROM instruments
               WHERE market = $1 AND symbol = $2 AND contract_address IS NULL
               ORDER BY created_at"#,
        )
        .bind(&market)
        .bind(&symbol)
        .fetch_all(&mut *transaction)
        .await?;
        match matches.as_slice() {
            [instrument_id] => (*instrument_id, false),
            [] => {
                let name = input
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&symbol);
                let instrument_id: Uuid = sqlx::query_scalar(
                    r#"INSERT INTO instruments (market, symbol, name, currency)
                       VALUES ($1, $2, $3, $4) RETURNING id"#,
                )
                .bind(&market)
                .bind(&symbol)
                .bind(name)
                .bind(&currency)
                .fetch_one(&mut *transaction)
                .await?;
                (instrument_id, true)
            }
            _ => {
                return Err(AppError::BadRequest(
                    "more than one instrument uses this symbol; choose the exact existing instrument"
                        .into(),
                ));
            }
        }
    };

    let current: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"WITH latest AS (
             SELECT id FROM position_snapshots
             WHERE account_id = $1
             ORDER BY as_of DESC, created_at DESC, id DESC LIMIT 1
           )
           SELECT psi.quantity::text, psi.average_cost::text, psi.market_price::text
           FROM latest
           JOIN position_snapshot_items psi ON psi.snapshot_id = latest.id
           WHERE psi.instrument_id = $2"#,
    )
    .bind(input.account_id)
    .bind(instrument_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let (current_quantity, current_average_cost, current_market_price) =
        current.unwrap_or_else(|| ("0".into(), None, None));

    let insufficient: bool = sqlx::query_scalar("SELECT $1 = 'sell' AND $2::numeric > $3::numeric")
        .bind(&input.side)
        .bind(&input.quantity)
        .bind(&current_quantity)
        .fetch_one(&mut *transaction)
        .await?;
    if insufficient {
        return Err(AppError::BadRequest(
            "sell quantity exceeds the current position".into(),
        ));
    }

    let (next_quantity, next_average_cost, realized_pnl): (String, Option<String>, Option<String>) =
        sqlx::query_as(
            r#"SELECT
             (CASE WHEN $1 = 'buy'
                THEN $2::numeric + $3::numeric
                ELSE $2::numeric - $3::numeric END)::text,
             (CASE WHEN $1 = 'buy' THEN
                CASE WHEN $2::numeric = 0 THEN $4::numeric
                     WHEN $5::numeric IS NULL THEN NULL
                     ELSE (($2::numeric * $5::numeric) + ($3::numeric * $4::numeric))
                          / ($2::numeric + $3::numeric) END
              ELSE CASE WHEN $2::numeric - $3::numeric = 0 THEN NULL ELSE $5::numeric END
              END)::text,
             (CASE WHEN $1 = 'sell' AND $5::numeric IS NOT NULL
                THEN $3::numeric * ($4::numeric - $5::numeric)
                ELSE NULL END)::text"#,
        )
        .bind(&input.side)
        .bind(&current_quantity)
        .bind(&input.quantity)
        .bind(&input.price)
        .bind(&current_average_cost)
        .fetch_one(&mut *transaction)
        .await?;

    let trade_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO trades
           (account_id, instrument_id, side, quantity, price, currency,
            executed_at, source, realized_pnl, position_quantity_after, average_cost_after)
           VALUES ($1, $2, $3, $4::numeric, $5::numeric, $6, $7, 'web_manual',
                   $8::numeric, $9::numeric, $10::numeric)
           RETURNING id"#,
    )
    .bind(input.account_id)
    .bind(instrument_id)
    .bind(&input.side)
    .bind(&input.quantity)
    .bind(&input.price)
    .bind(&currency)
    .bind(executed_at)
    .bind(&realized_pnl)
    .bind(&next_quantity)
    .bind(&next_average_cost)
    .fetch_one(&mut *transaction)
    .await?;

    let snapshot_input = CreateSnapshotInput {
        account_id: Some(input.account_id),
        wallet_id: None,
        as_of: executed_at,
        source: "web_trade".into(),
        merge_with_latest: true,
        items: vec![SnapshotItemInput {
            instrument_id,
            quantity: next_quantity.clone(),
            average_cost: next_average_cost.clone(),
            // A transaction price is a trade fact, not a current quote. Preserve a
            // previously observed quote; a newly opened position remains unpriced
            // until the market-data refresher succeeds.
            market_price: reference_price_after_trade(current_market_price, &input.price),
        }],
    };
    let (snapshot_id, _) = persist_merged_snapshot(&mut transaction, snapshot_input).await?;
    transaction.commit().await?;

    let quote_state = state.clone();
    tokio::spawn(async move {
        if let Err(error) = crate::market_data::refresh_market_data(&quote_state).await {
            tracing::warn!(%error, "post-trade market price refresh failed");
        }
    });

    Ok(Json(json!({
        "id": trade_id,
        "snapshot_id": snapshot_id,
        "instrument_id": instrument_id,
        "instrument_created": instrument_created,
        "position_quantity_after": next_quantity,
        "average_cost_after": next_average_cost,
        "realized_pnl": realized_pnl,
    })))
}

#[derive(Debug, Serialize, FromRow)]
struct CurrentPosition {
    snapshot_id: Uuid,
    position_snapshot_item_id: Uuid,
    as_of: DateTime<Utc>,
    owner_id: Uuid,
    owner_name: String,
    owner_type: String,
    instrument_id: Uuid,
    market: String,
    currency: Option<String>,
    symbol: String,
    instrument_name: String,
    quantity: String,
    average_cost: Option<String>,
    market_price: Option<String>,
    market_price_as_of: Option<DateTime<Utc>>,
    market_value: Option<String>,
    unrealized_pnl: Option<String>,
}

async fn list_current_positions(
    State(state): State<AppState>,
) -> Result<Json<Vec<CurrentPosition>>, AppError> {
    let positions = sqlx::query_as::<_, CurrentPosition>(
        r#"WITH ranked AS (
             SELECT ps.*, row_number() OVER (
               PARTITION BY ps.account_id, ps.wallet_id
               ORDER BY ps.as_of DESC, ps.created_at DESC, ps.id DESC
             ) AS rank
             FROM position_snapshots ps WHERE ps.account_id IS NOT NULL
           )
           SELECT r.id AS snapshot_id, psi.id AS position_snapshot_item_id, r.as_of,
                  r.account_id AS owner_id, a.name AS owner_name,
                  'account' AS owner_type,
                  i.id AS instrument_id, i.market, COALESCE(a.currency, i.currency) AS currency,
                  i.symbol, i.name AS instrument_name,
                  psi.quantity::text, psi.average_cost::text, psi.market_price::text,
                  NULLIF(psi.metadata->>'quote_observed_at', '')::timestamptz AS market_price_as_of,
                  psi.market_value::text, psi.unrealized_pnl::text
           FROM ranked r
           JOIN position_snapshot_items psi ON psi.snapshot_id = r.id
           JOIN instruments i ON i.id = psi.instrument_id
           JOIN accounts a ON a.id = r.account_id
           WHERE r.rank = 1 AND psi.quantity <> 0 AND i.market IN ('a_share', 'hk', 'us')
           ORDER BY i.market, i.symbol"#,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(positions))
}

#[derive(Debug, Serialize, FromRow)]
struct PositionChange {
    owner_id: Uuid,
    owner_name: String,
    owner_type: String,
    instrument_id: Uuid,
    market: String,
    symbol: String,
    instrument_name: String,
    previous_quantity: String,
    current_quantity: String,
    quantity_delta: String,
    change_type: String,
    current_as_of: DateTime<Utc>,
    previous_as_of: DateTime<Utc>,
}

async fn list_position_changes(
    State(state): State<AppState>,
) -> Result<Json<Vec<PositionChange>>, AppError> {
    let changes = sqlx::query_as::<_, PositionChange>(
        r#"WITH ranked AS (
             SELECT ps.*, row_number() OVER (
               PARTITION BY ps.account_id, ps.wallet_id
               ORDER BY ps.as_of DESC, ps.created_at DESC, ps.id DESC
             ) AS rank
             FROM position_snapshots ps WHERE ps.account_id IS NOT NULL
           ), owners AS (
             SELECT account_id, wallet_id,
                    (array_agg(id) FILTER (WHERE rank = 1))[1] AS current_id,
                    (array_agg(id) FILTER (WHERE rank = 2))[1] AS previous_id,
                    max(as_of) FILTER (WHERE rank = 1) AS current_as_of,
                    max(as_of) FILTER (WHERE rank = 2) AS previous_as_of
             FROM ranked WHERE rank <= 2
             GROUP BY account_id, wallet_id
             HAVING count(*) >= 2
           ), owner_instruments AS (
             SELECT o.*, instrument.instrument_id
             FROM owners o
             CROSS JOIN LATERAL (
               SELECT instrument_id FROM position_snapshot_items WHERE snapshot_id = o.current_id
               UNION
               SELECT instrument_id FROM position_snapshot_items WHERE snapshot_id = o.previous_id
             ) instrument
           ), paired AS (
             SELECT o.account_id, o.wallet_id, o.current_as_of, o.previous_as_of,
                    o.instrument_id,
                    COALESCE(previous_item.quantity, 0) AS previous_quantity,
                    COALESCE(current_item.quantity, 0) AS current_quantity
             FROM owner_instruments o
             LEFT JOIN position_snapshot_items current_item
               ON current_item.snapshot_id = o.current_id
              AND current_item.instrument_id = o.instrument_id
             LEFT JOIN position_snapshot_items previous_item
               ON previous_item.snapshot_id = o.previous_id
              AND previous_item.instrument_id = o.instrument_id
           )
           SELECT p.account_id AS owner_id, a.name AS owner_name,
                  'account' AS owner_type,
                  i.id AS instrument_id, i.market, i.symbol, i.name AS instrument_name,
                  p.previous_quantity::text, p.current_quantity::text,
                  (p.current_quantity - p.previous_quantity)::text AS quantity_delta,
                  CASE
                    WHEN sign(p.previous_quantity) <> sign(p.current_quantity)
                         AND p.previous_quantity <> 0 AND p.current_quantity <> 0 THEN 'reversed'
                    WHEN p.previous_quantity = 0 AND p.current_quantity <> 0 THEN 'opened'
                    WHEN p.previous_quantity <> 0 AND p.current_quantity = 0 THEN 'closed'
                    WHEN abs(p.current_quantity) > abs(p.previous_quantity) THEN 'increased'
                    WHEN abs(p.current_quantity) < abs(p.previous_quantity) THEN 'reduced'
                    ELSE 'unchanged'
                  END AS change_type,
                  p.current_as_of, p.previous_as_of
           FROM paired p
           JOIN instruments i ON i.id = p.instrument_id
           JOIN accounts a ON a.id = p.account_id
           WHERE p.current_quantity <> p.previous_quantity AND i.market IN ('a_share', 'hk', 'us')
           ORDER BY p.current_as_of DESC, i.market, i.symbol"#,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(changes))
}

async fn insert_snapshot_item(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot_id: Uuid,
    item: SnapshotItemInput,
) -> Result<(), AppError> {
    sqlx::query(
        r#"INSERT INTO position_snapshot_items
           (snapshot_id, instrument_id, quantity, average_cost, market_price, market_value, unrealized_pnl)
           VALUES ($1, $2, $3::numeric, $4::numeric, $5::numeric,
                   CASE WHEN $5::numeric IS NULL THEN NULL ELSE $3::numeric * $5::numeric END,
                   CASE WHEN $4::numeric IS NULL OR $5::numeric IS NULL
                        THEN NULL ELSE $3::numeric * ($5::numeric - $4::numeric) END)"#,
    )
    .bind(snapshot_id)
    .bind(item.instrument_id)
    .bind(item.quantity)
    .bind(item.average_cost)
    .bind(item.market_price)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn validate_stock_snapshot(input: &CreateSnapshotInput) -> Result<(), AppError> {
    if input.wallet_id.is_some() {
        return Err(AppError::BadRequest(
            "Wallet tracking has been removed".into(),
        ));
    }
    validate_snapshot_input(input).map_err(|message| AppError::BadRequest(message.into()))
}

async fn create_snapshot(
    State(state): State<AppState>,
    Json(input): Json<CreateSnapshotInput>,
) -> Result<Json<Value>, AppError> {
    validate_stock_snapshot(&input)?;
    let mut transaction = state.pool.begin().await?;
    let (snapshot_id, item_count) = persist_merged_snapshot(&mut transaction, input).await?;
    transaction.commit().await?;
    Ok(Json(json!({ "id": snapshot_id, "item_count": item_count })))
}

async fn persist_merged_snapshot(
    transaction: &mut Transaction<'_, Postgres>,
    input: CreateSnapshotInput,
) -> Result<(Uuid, i64), AppError> {
    let owner_lock = input
        .account_id
        .map(|id| format!("position-snapshot:account:{id}"))
        .or_else(|| {
            input
                .wallet_id
                .map(|id| format!("position-snapshot:wallet:{id}"))
        })
        .ok_or_else(|| AppError::BadRequest("a position owner is required".into()))?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(owner_lock)
        .execute(&mut **transaction)
        .await?;
    validate_snapshot_ownership(transaction, &input).await?;
    let previous_snapshot_id: Option<Uuid> = if input.merge_with_latest {
        sqlx::query_scalar(
            r#"SELECT id FROM position_snapshots
               WHERE account_id IS NOT DISTINCT FROM $1
                 AND wallet_id IS NOT DISTINCT FROM $2
                 AND as_of < $3
               ORDER BY as_of DESC, created_at DESC, id DESC
               LIMIT 1"#,
        )
        .bind(input.account_id)
        .bind(input.wallet_id)
        .bind(input.as_of)
        .fetch_optional(&mut **transaction)
        .await?
    } else {
        None
    };
    let snapshot_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO position_snapshots (account_id, wallet_id, as_of, source)
           VALUES ($1, $2, $3, $4) RETURNING id"#,
    )
    .bind(input.account_id)
    .bind(input.wallet_id)
    .bind(input.as_of)
    .bind(input.source.trim())
    .fetch_one(&mut **transaction)
    .await?;

    if let Some(previous_snapshot_id) = previous_snapshot_id {
        let replaced_instruments: Vec<Uuid> =
            input.items.iter().map(|item| item.instrument_id).collect();
        sqlx::query(
            r#"INSERT INTO position_snapshot_items
               (snapshot_id, instrument_id, quantity, average_cost, market_price,
                market_value, unrealized_pnl, metadata)
               SELECT $1, instrument_id, quantity, average_cost, market_price,
                      market_value, unrealized_pnl, metadata
               FROM position_snapshot_items
               WHERE snapshot_id = $2
                 AND NOT (instrument_id = ANY($3))"#,
        )
        .bind(snapshot_id)
        .bind(previous_snapshot_id)
        .bind(&replaced_instruments)
        .execute(&mut **transaction)
        .await?;
    }

    for item in input.items {
        insert_snapshot_item(transaction, snapshot_id, item).await?;
    }
    let item_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM position_snapshot_items WHERE snapshot_id = $1")
            .bind(snapshot_id)
            .fetch_one(&mut **transaction)
            .await?;
    Ok((snapshot_id, item_count))
}

#[derive(Debug, Deserialize)]
struct AddAccountPositionInput {
    instrument_id: Option<Uuid>,
    symbol: Option<String>,
    name: Option<String>,
    quantity: String,
    average_cost: Option<String>,
    market_price: Option<String>,
}

fn validate_quick_position(input: &AddAccountPositionInput) -> Result<(), AppError> {
    let has_symbol = input
        .symbol
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    if input.instrument_id.is_some() == has_symbol {
        return Err(AppError::BadRequest(
            "provide either instrument_id or symbol".into(),
        ));
    }
    let quantity = input
        .quantity
        .parse::<f64>()
        .map_err(|_| AppError::BadRequest("quantity must be numeric".into()))?;
    if !quantity.is_finite() || quantity == 0.0 {
        return Err(AppError::BadRequest(
            "a new position quantity must be finite and non-zero".into(),
        ));
    }
    Ok(())
}

fn supports_quick_instrument_creation(market: &str) -> bool {
    matches!(market, "a_share" | "hk" | "us")
}

async fn add_account_position(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Json(input): Json<AddAccountPositionInput>,
) -> Result<Json<Value>, AppError> {
    validate_quick_position(&input)?;
    let mut transaction = state.pool.begin().await?;
    let account: Option<(String, String)> = sqlx::query_as(
        "SELECT market, currency FROM accounts WHERE id = $1 AND is_active FOR UPDATE",
    )
    .bind(account_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let (market, currency) = account.ok_or(AppError::NotFound)?;

    let (instrument_id, instrument_created) = if let Some(instrument_id) = input.instrument_id {
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM instruments WHERE id = $1 AND market = $2)",
        )
        .bind(instrument_id)
        .bind(&market)
        .fetch_one(&mut *transaction)
        .await?;
        if !valid {
            return Err(AppError::BadRequest(
                "instrument does not belong to the account market".into(),
            ));
        }
        (instrument_id, false)
    } else {
        let symbol = input
            .symbol
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_uppercase();
        if !supports_quick_instrument_creation(&market) {
            return Err(AppError::BadRequest(
                "new Crypto and Meme instruments require chain identity setup".into(),
            ));
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("listed-instrument:{market}:{symbol}"))
            .execute(&mut *transaction)
            .await?;
        let matches: Vec<Uuid> = sqlx::query_scalar(
            r#"SELECT id FROM instruments
               WHERE market = $1 AND symbol = $2 AND contract_address IS NULL
               ORDER BY created_at"#,
        )
        .bind(&market)
        .bind(&symbol)
        .fetch_all(&mut *transaction)
        .await?;
        match matches.as_slice() {
            [instrument_id] => (*instrument_id, false),
            [] => {
                let name = input
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&symbol);
                let instrument_id: Uuid = sqlx::query_scalar(
                    r#"INSERT INTO instruments (market, symbol, name, currency)
                       VALUES ($1, $2, $3, $4) RETURNING id"#,
                )
                .bind(&market)
                .bind(&symbol)
                .bind(name)
                .bind(&currency)
                .fetch_one(&mut *transaction)
                .await?;
                (instrument_id, true)
            }
            _ => {
                return Err(AppError::BadRequest(
                    "more than one instrument uses this symbol; choose the exact existing instrument"
                        .into(),
                ));
            }
        }
    };

    let snapshot_input = CreateSnapshotInput {
        account_id: Some(account_id),
        wallet_id: None,
        as_of: Utc::now(),
        source: "web_manual".into(),
        merge_with_latest: true,
        items: vec![SnapshotItemInput {
            instrument_id,
            quantity: input.quantity,
            average_cost: input.average_cost,
            market_price: input.market_price,
        }],
    };
    validate_snapshot_input(&snapshot_input)
        .map_err(|message| AppError::BadRequest(message.into()))?;
    let (snapshot_id, item_count) =
        persist_merged_snapshot(&mut transaction, snapshot_input).await?;
    transaction.commit().await?;
    Ok(Json(json!({
        "snapshot_id": snapshot_id,
        "instrument_id": instrument_id,
        "instrument_created": instrument_created,
        "item_count": item_count,
    })))
}

async fn validate_snapshot_ownership(
    transaction: &mut Transaction<'_, Postgres>,
    input: &CreateSnapshotInput,
) -> Result<(), AppError> {
    let instrument_ids: Vec<Uuid> = input.items.iter().map(|item| item.instrument_id).collect();
    validate_owner_instruments(
        transaction,
        input.account_id,
        input.wallet_id,
        &instrument_ids,
    )
    .await
}

async fn validate_owner_instruments(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: Option<Uuid>,
    wallet_id: Option<Uuid>,
    instrument_ids: &[Uuid],
) -> Result<(), AppError> {
    if wallet_id.is_some() {
        return Err(AppError::BadRequest(
            "Wallet tracking has been removed".into(),
        ));
    }
    let instruments: Vec<(Uuid, String, Option<String>)> =
        sqlx::query_as("SELECT id, market, chain FROM instruments WHERE id = ANY($1)")
            .bind(instrument_ids)
            .fetch_all(&mut **transaction)
            .await?;
    if instruments.len() != instrument_ids.len() {
        return Err(AppError::BadRequest(
            "every snapshot instrument must exist".into(),
        ));
    }

    if instruments
        .iter()
        .any(|(_, market, _)| !supports_quick_instrument_creation(market))
    {
        return Err(AppError::BadRequest(
            "Only listed stock instruments are supported".into(),
        ));
    }

    if let Some(account_id) = account_id {
        let market: Option<String> =
            sqlx::query_scalar("SELECT market FROM accounts WHERE id = $1 AND is_active")
                .bind(account_id)
                .fetch_optional(&mut **transaction)
                .await?;
        let market = market.ok_or(AppError::NotFound)?;
        if instruments
            .iter()
            .any(|(_, instrument_market, _)| instrument_market != &market)
        {
            return Err(AppError::BadRequest(
                "account snapshots may contain only instruments from the account market".into(),
            ));
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct CsvImportInput {
    account_id: Uuid,
    as_of: Option<DateTime<Utc>>,
    csv: String,
}

async fn find_or_create_csv_instrument(
    transaction: &mut Transaction<'_, Postgres>,
    row: &ParsedPositionRow,
) -> Result<Uuid, AppError> {
    if row.market == Market::Meme {
        return Err(AppError::BadRequest(
            "Meme positions require chain and contract identity and cannot use the stock CSV importer"
                .into(),
        ));
    }
    let market = row.market.to_string();
    let existing: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM instruments
           WHERE market = $1 AND exchange IS NOT DISTINCT FROM $2
             AND symbol = $3 AND contract_address IS NULL
           LIMIT 1"#,
    )
    .bind(&market)
    .bind(&row.exchange)
    .bind(&row.symbol)
    .fetch_optional(&mut **transaction)
    .await?;

    if let Some(id) = existing {
        sqlx::query(
            r#"UPDATE instruments
               SET name = $2, currency = $3, updated_at = now()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(&row.name)
        .bind(&row.currency)
        .execute(&mut **transaction)
        .await?;
        return Ok(id);
    }

    let id = sqlx::query_scalar(
        r#"INSERT INTO instruments (market, symbol, name, currency, exchange)
           VALUES ($1, $2, $3, $4, $5) RETURNING id"#,
    )
    .bind(market)
    .bind(&row.symbol)
    .bind(&row.name)
    .bind(&row.currency)
    .bind(&row.exchange)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(id)
}

async fn import_position_csv(
    State(state): State<AppState>,
    Json(input): Json<CsvImportInput>,
) -> Result<Json<Value>, AppError> {
    let rows = parse_position_csv(&input.csv).map_err(AppError::BadRequest)?;
    let as_of = input.as_of.unwrap_or_else(Utc::now);
    let mut transaction = state.pool.begin().await?;
    let account_market: Option<String> =
        sqlx::query_scalar("SELECT market FROM accounts WHERE id = $1 AND is_active")
            .bind(input.account_id)
            .fetch_optional(&mut *transaction)
            .await?;
    let account_market = account_market.ok_or(AppError::NotFound)?;
    if rows
        .iter()
        .any(|row| row.market.to_string() != account_market)
    {
        return Err(AppError::BadRequest(
            "all CSV rows must match the selected account market".into(),
        ));
    }

    let snapshot_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO position_snapshots (account_id, as_of, source)
           VALUES ($1, $2, 'csv_import') RETURNING id"#,
    )
    .bind(input.account_id)
    .bind(as_of)
    .fetch_one(&mut *transaction)
    .await?;

    let mut imported_instruments = HashSet::new();
    for row in &rows {
        let instrument_id = find_or_create_csv_instrument(&mut transaction, row).await?;
        if !imported_instruments.insert(instrument_id) {
            return Err(AppError::BadRequest(format!(
                "CSV contains the same instrument more than once: {}",
                row.symbol
            )));
        }
        insert_snapshot_item(
            &mut transaction,
            snapshot_id,
            SnapshotItemInput {
                instrument_id,
                quantity: row.quantity.clone(),
                average_cost: row.average_cost.clone(),
                market_price: row.market_price.clone(),
            },
        )
        .await?;
    }
    transaction.commit().await?;
    Ok(Json(json!({
        "snapshot_id": snapshot_id,
        "account_id": input.account_id,
        "as_of": as_of,
        "imported_rows": rows.len()
    })))
}

#[derive(Debug, Serialize, FromRow)]
struct TradeIntentView {
    id: Uuid,
    title: String,
    strategy_type: String,
    time_horizon: String,
    status: String,
    version: i32,
    thesis: String,
    risk_budget: Option<String>,
    target_position: Option<String>,
    catalysts: Value,
    invalidation_conditions: Value,
    exit_principles: Value,
    notes: Option<String>,
    instrument_ids: Vec<Uuid>,
    account_id: Option<Uuid>,
    wallet_id: Option<Uuid>,
    owner_name: String,
    position_slice: Option<String>,
    created_at: DateTime<Utc>,
}

async fn list_trade_intents(
    State(state): State<AppState>,
) -> Result<Json<Vec<TradeIntentView>>, AppError> {
    let intents = sqlx::query_as::<_, TradeIntentView>(
        r#"SELECT ti.id, tiv.title, tiv.strategy_type, tiv.time_horizon, tiv.status,
                  tiv.version, tiv.thesis, tiv.risk_budget::text, tiv.target_position::text,
                  tiv.catalysts, tiv.invalidation_conditions, tiv.exit_principles,
                  tiv.notes, tiv.instrument_ids, tiv.account_id, tiv.wallet_id,
                  COALESCE(a.name, '全部相关仓位') AS owner_name,
                  tiv.position_slice::text, tiv.created_at
           FROM trade_intents ti
           JOIN LATERAL (
             SELECT * FROM trade_intent_versions
             WHERE trade_intent_id = ti.id ORDER BY version DESC LIMIT 1
           ) tiv ON true
           LEFT JOIN accounts a ON a.id = tiv.account_id
           WHERE tiv.wallet_id IS NULL
             AND NOT EXISTS (
               SELECT 1 FROM instruments i WHERE i.id = ANY(tiv.instrument_ids)
                 AND i.market NOT IN ('a_share', 'hk', 'us')
             )
           ORDER BY ti.updated_at DESC"#,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(intents))
}

#[derive(Debug, Deserialize)]
struct IntentVersionInput {
    title: Option<String>,
    strategy_type: Option<String>,
    time_horizon: Option<String>,
    status: Option<String>,
    thesis: String,
    risk_budget: Option<String>,
    target_position: Option<String>,
    #[serde(default)]
    catalysts: Vec<String>,
    #[serde(default)]
    invalidation_conditions: Vec<String>,
    #[serde(default)]
    exit_principles: Vec<String>,
    notes: Option<String>,
    scope: Option<IntentScopeInput>,
}

#[derive(Clone, Debug, Deserialize)]
struct IntentScopeInput {
    instrument_ids: Vec<Uuid>,
    account_id: Option<Uuid>,
    wallet_id: Option<Uuid>,
    position_slice: Option<String>,
}

#[derive(Debug, FromRow)]
struct LockedIntentState {
    id: Uuid,
    status: String,
    title: String,
    strategy_type: String,
    time_horizon: String,
    instrument_ids: Vec<Uuid>,
    account_id: Option<Uuid>,
    wallet_id: Option<Uuid>,
    position_slice: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateTradeIntent {
    title: String,
    strategy_type: String,
    time_horizon: String,
    #[serde(default = "active_status")]
    status: String,
    instrument_ids: Vec<Uuid>,
    account_id: Option<Uuid>,
    wallet_id: Option<Uuid>,
    position_slice: Option<String>,
    research_draft: Option<ResearchDraftConfirmation>,
    version: IntentVersionInput,
}

#[derive(Debug, Deserialize)]
struct ResearchDraftConfirmation {
    finding_id: Uuid,
    position_snapshot_item_id: Uuid,
    #[serde(default)]
    modified: bool,
}

impl CreateTradeIntent {
    fn scope(&self) -> IntentScopeInput {
        IntentScopeInput {
            instrument_ids: self.instrument_ids.clone(),
            account_id: self.account_id,
            wallet_id: self.wallet_id,
            position_slice: self.position_slice.clone(),
        }
    }
}

fn active_status() -> String {
    "active".into()
}

fn valid_intent_status(status: &str) -> bool {
    matches!(
        status,
        "draft" | "active" | "paused" | "invalidated" | "closed"
    )
}

#[allow(clippy::too_many_arguments)]
async fn insert_intent_version(
    transaction: &mut Transaction<'_, Postgres>,
    intent_id: Uuid,
    version: i32,
    title: &str,
    strategy_type: &str,
    time_horizon: &str,
    status: &str,
    scope: &IntentScopeInput,
    input: IntentVersionInput,
) -> Result<Uuid, AppError> {
    let id = sqlx::query_scalar(
        r#"INSERT INTO trade_intent_versions
           (trade_intent_id, version, title, strategy_type, time_horizon, status,
            thesis, risk_budget, target_position, catalysts,
            invalidation_conditions, exit_principles, notes, instrument_ids,
            account_id, wallet_id, position_slice)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8::numeric, $9::numeric, $10, $11, $12, $13,
                   $14, $15, $16, $17::numeric)
           RETURNING id"#,
    )
    .bind(intent_id)
    .bind(version)
    .bind(title)
    .bind(strategy_type)
    .bind(time_horizon)
    .bind(status)
    .bind(input.thesis)
    .bind(input.risk_budget)
    .bind(input.target_position)
    .bind(json!(input.catalysts))
    .bind(json!(input.invalidation_conditions))
    .bind(json!(input.exit_principles))
    .bind(input.notes)
    .bind(&scope.instrument_ids)
    .bind(scope.account_id)
    .bind(scope.wallet_id)
    .bind(&scope.position_slice)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(id)
}

fn validate_intent_scope_shape(scope: &IntentScopeInput) -> Result<(), AppError> {
    if scope.wallet_id.is_some() {
        return Err(AppError::BadRequest(
            "Wallet tracking has been removed".into(),
        ));
    }
    if scope.instrument_ids.is_empty() {
        return Err(AppError::BadRequest(
            "at least one scoped instrument is required".into(),
        ));
    }
    if scope.instrument_ids.iter().collect::<HashSet<_>>().len() != scope.instrument_ids.len() {
        return Err(AppError::BadRequest(
            "scoped instruments must be unique".into(),
        ));
    }
    Ok(())
}

async fn replace_intent_scope(
    transaction: &mut Transaction<'_, Postgres>,
    intent_id: Uuid,
    scope: &IntentScopeInput,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM position_intent_links WHERE trade_intent_id = $1")
        .bind(intent_id)
        .execute(&mut **transaction)
        .await?;
    for instrument_id in &scope.instrument_ids {
        sqlx::query(
            r#"INSERT INTO position_intent_links
               (trade_intent_id, instrument_id, account_id, wallet_id, position_slice)
               VALUES ($1, $2, $3, $4, $5::numeric)"#,
        )
        .bind(intent_id)
        .bind(instrument_id)
        .bind(scope.account_id)
        .bind(scope.wallet_id)
        .bind(&scope.position_slice)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn ensure_no_active_intent_for_scope(
    transaction: &mut Transaction<'_, Postgres>,
    scope: &IntentScopeInput,
    excluding_intent_id: Option<Uuid>,
) -> Result<(), AppError> {
    let already_active: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
             SELECT 1 FROM trade_intents ti
             JOIN position_intent_links link ON link.trade_intent_id = ti.id
             WHERE ti.status = 'active'
               AND ($4::uuid IS NULL OR ti.id <> $4)
               AND link.instrument_id = ANY($1)
               AND link.account_id IS NOT DISTINCT FROM $2
               AND link.wallet_id IS NOT DISTINCT FROM $3
           )"#,
    )
    .bind(&scope.instrument_ids)
    .bind(scope.account_id)
    .bind(scope.wallet_id)
    .bind(excluding_intent_id)
    .fetch_one(&mut **transaction)
    .await?;
    if already_active {
        return Err(AppError::BadRequest(
            "该账户与标的已经存在有效持仓逻辑，请编辑现有逻辑".into(),
        ));
    }
    Ok(())
}

async fn create_trade_intent(
    State(state): State<AppState>,
    Json(mut input): Json<CreateTradeIntent>,
) -> Result<Json<Value>, AppError> {
    if input.title.trim().is_empty()
        || input.version.thesis.trim().is_empty()
        || input.instrument_ids.is_empty()
    {
        return Err(AppError::BadRequest(
            "title, thesis and at least one instrument are required".into(),
        ));
    }
    if !valid_intent_status(&input.status) {
        return Err(AppError::BadRequest("invalid trade intent status".into()));
    }
    let scope = input.scope();
    validate_intent_scope_shape(&scope)?;
    let mut transaction = state.pool.begin().await?;
    let owner_lock = scope
        .account_id
        .map(|id| format!("position-snapshot:account:{id}"))
        .or_else(|| {
            scope
                .wallet_id
                .map(|id| format!("position-snapshot:wallet:{id}"))
        });
    if input.research_draft.is_some() || input.status == "active" {
        let owner_lock = owner_lock
            .ok_or_else(|| AppError::BadRequest("有效持仓逻辑必须绑定到一个明确的账户".into()))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(owner_lock)
            .execute(&mut *transaction)
            .await?;
    }
    validate_owner_instruments(
        &mut transaction,
        scope.account_id,
        scope.wallet_id,
        &scope.instrument_ids,
    )
    .await?;
    if input.status == "active" {
        ensure_no_active_intent_for_scope(&mut transaction, &scope, None).await?;
    }
    if let Some(draft) = &input.research_draft {
        let stored_draft: Option<Value> = sqlx::query_scalar(
            r#"SELECT rf.diagnostic_dimensions->'intent_draft'
                 FROM research_findings rf
                 JOIN research_runs rr ON rr.id = rf.research_run_id
                 JOIN position_snapshot_items source_item
                   ON source_item.id = rf.affected_object_id
                  AND source_item.id = $2
                 JOIN position_snapshots source_snapshot
                   ON source_snapshot.id = source_item.snapshot_id
                 JOIN LATERAL (
                   SELECT current_item.quantity, current_item.average_cost
                   FROM position_snapshots current_snapshot
                   JOIN position_snapshot_items current_item
                     ON current_item.snapshot_id = current_snapshot.id
                    AND current_item.instrument_id = source_item.instrument_id
                   WHERE (source_snapshot.account_id IS NOT NULL
                          AND current_snapshot.account_id = source_snapshot.account_id)
                      OR (source_snapshot.wallet_id IS NOT NULL
                          AND current_snapshot.wallet_id = source_snapshot.wallet_id)
                   ORDER BY current_snapshot.as_of DESC,
                            current_snapshot.created_at DESC,
                            current_snapshot.id DESC
                   LIMIT 1
                 ) current_position ON true
                 WHERE rf.id = $1
                   AND rr.status = 'complete'
                   AND rr.validation_status = 'valid'
                   AND rf.finding_type = 'equity_position_diagnosis'
                   AND rf.affected_object_type = 'position_snapshot_item'
                   AND rf.diagnostic_dimensions->>'intent_assessment' = 'missing'
                   AND rf.diagnostic_dimensions->'intent_draft' IS NOT NULL
                   AND cardinality($3::uuid[]) = 1
                   AND source_item.instrument_id = $3[1]
                   AND source_snapshot.account_id IS NOT DISTINCT FROM $4
                   AND source_snapshot.wallet_id IS NOT DISTINCT FROM $5
                   AND source_item.quantity = current_position.quantity
                   AND source_item.average_cost IS NOT DISTINCT FROM current_position.average_cost
                   AND NOT EXISTS (
                     SELECT 1 FROM trades t
                     WHERE t.instrument_id = source_item.instrument_id
                       AND t.account_id IS NOT DISTINCT FROM source_snapshot.account_id
                       AND t.wallet_id IS NOT DISTINCT FROM source_snapshot.wallet_id
                       AND t.side IN ('buy', 'sell')
                       AND t.created_at > source_snapshot.created_at
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM trade_intents existing
                     JOIN position_intent_links link ON link.trade_intent_id = existing.id
                     WHERE existing.status = 'active'
                       AND link.instrument_id = source_item.instrument_id
                       AND link.account_id IS NOT DISTINCT FROM source_snapshot.account_id
                       AND link.wallet_id IS NOT DISTINCT FROM source_snapshot.wallet_id
                   )
                 LIMIT 1"#,
        )
        .bind(draft.finding_id)
        .bind(draft.position_snapshot_item_id)
        .bind(&scope.instrument_ids)
        .bind(scope.account_id)
        .bind(scope.wallet_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(stored_draft) = stored_draft else {
            return Err(AppError::BadRequest(
                "研究草稿已过期、持仓已发生交易，或已有有效持仓逻辑，请重新检查".into(),
            ));
        };
        if !draft.modified {
            let stored_thesis = stored_draft
                .get("thesis")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let stored_horizon = stored_draft
                .get("time_horizon")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let stored_list = |key: &str| {
                stored_draft
                    .get(key)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            };
            if input.version.thesis.trim() != stored_thesis.trim()
                || input.time_horizon.trim() != stored_horizon.trim()
                || input.version.catalysts != stored_list("catalysts")
                || input.version.invalidation_conditions != stored_list("invalidation_conditions")
            {
                return Err(AppError::BadRequest(
                    "研究草稿内容已被修改，但请求没有声明用户修改".into(),
                ));
            }
        }
        let provenance = format!(
            "{}经验证研究草稿 Finding {}；冻结持仓项 {}。",
            if draft.modified {
                "基于用户修改的"
            } else {
                "采用"
            },
            draft.finding_id,
            draft.position_snapshot_item_id
        );
        input.version.notes = Some(match input.version.notes.take() {
            Some(notes) if !notes.trim().is_empty() => format!("{notes}\n{provenance}"),
            _ => provenance,
        });
    }
    let intent_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO trade_intents (title, strategy_type, time_horizon, status)
           VALUES ($1, $2, $3, $4) RETURNING id"#,
    )
    .bind(input.title.trim())
    .bind(input.strategy_type.trim())
    .bind(input.time_horizon.trim())
    .bind(&input.status)
    .fetch_one(&mut *transaction)
    .await?;
    let version_id = insert_intent_version(
        &mut transaction,
        intent_id,
        1,
        input.title.trim(),
        input.strategy_type.trim(),
        input.time_horizon.trim(),
        &input.status,
        &scope,
        input.version,
    )
    .await?;
    replace_intent_scope(&mut transaction, intent_id, &scope).await?;
    transaction.commit().await?;
    Ok(Json(
        json!({ "id": intent_id, "version_id": version_id, "version": 1 }),
    ))
}

async fn create_trade_intent_version(
    State(state): State<AppState>,
    axum::extract::Path(intent_id): axum::extract::Path<Uuid>,
    Json(input): Json<IntentVersionInput>,
) -> Result<Json<Value>, AppError> {
    if input.thesis.trim().is_empty() {
        return Err(AppError::BadRequest("thesis is required".into()));
    }
    let mut transaction = state.pool.begin().await?;
    let locked_intent: Option<LockedIntentState> = sqlx::query_as(
        r#"SELECT ti.id, ti.status, ti.title, ti.strategy_type, ti.time_horizon,
                  tiv.instrument_ids, tiv.account_id, tiv.wallet_id, tiv.position_slice::text
           FROM trade_intents ti
           JOIN LATERAL (
             SELECT * FROM trade_intent_versions
             WHERE trade_intent_id = ti.id ORDER BY version DESC LIMIT 1
           ) tiv ON true
           WHERE ti.id = $1 FOR UPDATE OF ti"#,
    )
    .bind(intent_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let locked_intent = locked_intent.ok_or(AppError::NotFound)?;
    debug_assert_eq!(locked_intent.id, intent_id);
    let next_scope = input.scope.clone().unwrap_or(IntentScopeInput {
        instrument_ids: locked_intent.instrument_ids,
        account_id: locked_intent.account_id,
        wallet_id: locked_intent.wallet_id,
        position_slice: locked_intent.position_slice,
    });
    let next_status = input.status.clone().unwrap_or(locked_intent.status);
    if next_status == "active" {
        let owner_lock = next_scope
            .account_id
            .map(|id| format!("position-snapshot:account:{id}"))
            .or_else(|| {
                next_scope
                    .wallet_id
                    .map(|id| format!("position-snapshot:wallet:{id}"))
            })
            .ok_or_else(|| AppError::BadRequest("有效持仓逻辑必须绑定到一个明确的账户".into()))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(owner_lock)
            .execute(&mut *transaction)
            .await?;
        ensure_no_active_intent_for_scope(&mut transaction, &next_scope, Some(intent_id)).await?;
    }
    validate_intent_scope_shape(&next_scope)?;
    validate_owner_instruments(
        &mut transaction,
        next_scope.account_id,
        next_scope.wallet_id,
        &next_scope.instrument_ids,
    )
    .await?;
    let next_title = input.title.clone().unwrap_or(locked_intent.title);
    let next_strategy = input
        .strategy_type
        .clone()
        .unwrap_or(locked_intent.strategy_type);
    let next_horizon = input
        .time_horizon
        .clone()
        .unwrap_or(locked_intent.time_horizon);
    if next_title.trim().is_empty()
        || next_strategy.trim().is_empty()
        || next_horizon.trim().is_empty()
    {
        return Err(AppError::BadRequest(
            "title, strategy_type and time_horizon are required".into(),
        ));
    }
    if !valid_intent_status(&next_status) {
        return Err(AppError::BadRequest("invalid trade intent status".into()));
    }
    let next_version: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(version), 0) + 1 FROM trade_intent_versions WHERE trade_intent_id = $1",
    )
    .bind(intent_id)
    .fetch_one(&mut *transaction)
    .await?;
    let version_id = insert_intent_version(
        &mut transaction,
        intent_id,
        next_version,
        next_title.trim(),
        next_strategy.trim(),
        next_horizon.trim(),
        &next_status,
        &next_scope,
        input,
    )
    .await?;
    replace_intent_scope(&mut transaction, intent_id, &next_scope).await?;
    sqlx::query(
        r#"UPDATE trade_intents
           SET status = $2, title = $3, strategy_type = $4, time_horizon = $5,
               updated_at = now()
           WHERE id = $1"#,
    )
    .bind(intent_id)
    .bind(&next_status)
    .bind(&next_title)
    .bind(&next_strategy)
    .bind(&next_horizon)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Json(json!({ "id": version_id, "version": next_version })))
}

async fn dashboard(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let positions: i64 = sqlx::query_scalar(
        r#"WITH ranked AS (
             SELECT ps.id, row_number() OVER (
               PARTITION BY ps.account_id, ps.wallet_id
               ORDER BY ps.as_of DESC, ps.created_at DESC, ps.id DESC
             ) AS rank FROM position_snapshots ps WHERE ps.account_id IS NOT NULL
           )
           SELECT count(*) FROM ranked r JOIN position_snapshot_items i ON i.snapshot_id = r.id
           JOIN instruments inst ON inst.id = i.instrument_id
           WHERE r.rank = 1 AND i.quantity <> 0 AND inst.market IN ('a_share', 'hk', 'us')"#,
    )
    .fetch_one(&state.pool)
    .await?;
    let active_intents: i64 =
        sqlx::query_scalar("SELECT count(*) FROM trade_intents WHERE status = 'active'")
            .fetch_one(&state.pool)
            .await?;
    let pending_briefings: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM briefing_runs WHERE status IN ('pending', 'running')",
    )
    .fetch_one(&state.pool)
    .await?;
    let last_snapshot: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT max(as_of) FROM position_snapshots WHERE account_id IS NOT NULL",
    )
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(json!({
        "positions": positions,
        "active_intents": active_intents,
        "pending_briefings": pending_briefings,
        "last_snapshot_at": last_snapshot,
        "research_runner": crate::research::runner_label(&state)
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual_trade(side: &str) -> RecordTradeInput {
        RecordTradeInput {
            account_id: Uuid::new_v4(),
            instrument_id: Some(Uuid::new_v4()),
            symbol: None,
            name: None,
            side: side.into(),
            quantity: "100".into(),
            price: "12.50".into(),
            executed_at: None,
        }
    }

    fn quick_position(
        instrument_id: Option<Uuid>,
        symbol: Option<&str>,
    ) -> AddAccountPositionInput {
        AddAccountPositionInput {
            instrument_id,
            symbol: symbol.map(str::to_string),
            name: None,
            quantity: "100".into(),
            average_cost: None,
            market_price: None,
        }
    }

    #[test]
    fn quick_position_accepts_an_existing_instrument_or_a_new_symbol() {
        assert!(validate_quick_position(&quick_position(Some(Uuid::new_v4()), None)).is_ok());
        assert!(validate_quick_position(&quick_position(None, Some("603893"))).is_ok());
    }

    #[test]
    fn quick_position_rejects_ambiguous_instrument_identity() {
        assert!(validate_quick_position(&quick_position(None, None)).is_err());
        assert!(
            validate_quick_position(&quick_position(Some(Uuid::new_v4()), Some("603893"))).is_err()
        );
    }

    #[test]
    fn quick_position_rejects_a_zero_quantity() {
        let mut input = quick_position(None, Some("603893"));
        input.quantity = "0".into();
        assert!(validate_quick_position(&input).is_err());
    }

    #[test]
    fn quick_instrument_creation_is_limited_to_listed_markets() {
        assert!(supports_quick_instrument_creation("a_share"));
        assert!(supports_quick_instrument_creation("hk"));
        assert!(supports_quick_instrument_creation("us"));
        assert!(!supports_quick_instrument_creation("crypto"));
        assert!(!supports_quick_instrument_creation("meme"));
    }

    #[test]
    fn manual_trade_accepts_only_buy_or_sell_with_positive_numbers() {
        assert!(validate_record_trade(&manual_trade("buy")).is_ok());
        assert!(validate_record_trade(&manual_trade("sell")).is_ok());

        let mut invalid_side = manual_trade("transfer_in");
        assert!(validate_record_trade(&invalid_side).is_err());
        invalid_side.side = "buy".into();
        invalid_side.quantity = "0".into();
        assert!(validate_record_trade(&invalid_side).is_err());
        invalid_side.quantity = "1".into();
        invalid_side.price = "-2".into();
        assert!(validate_record_trade(&invalid_side).is_err());
    }

    #[test]
    fn manual_trade_requires_an_existing_instrument_or_new_symbol() {
        let mut trade = manual_trade("buy");
        trade.instrument_id = None;
        trade.symbol = Some("AAPL".into());
        assert!(validate_record_trade(&trade).is_ok());

        trade.instrument_id = Some(Uuid::new_v4());
        assert!(validate_record_trade(&trade).is_err());
    }

    #[test]
    fn stock_account_currency_is_derived_from_its_market() {
        assert_eq!(default_account_currency(Market::AShare), Some("RMB"));
        assert_eq!(default_account_currency(Market::Hk), Some("HKD"));
        assert_eq!(default_account_currency(Market::Us), Some("USD"));
        assert_eq!(default_account_currency(Market::Crypto), None);
        assert_eq!(default_account_currency(Market::Meme), None);
    }

    #[test]
    fn retired_meme_instruments_cannot_be_created() {
        assert!(validate_new_instrument_market(Market::Meme).is_err());
        for market in [Market::AShare, Market::Hk, Market::Us] {
            assert!(validate_new_instrument_market(market).is_ok());
        }
        // Old database values can still be decoded without restoring the feature.
        assert_eq!("meme".parse::<Market>().unwrap(), Market::Meme);
    }

    #[test]
    fn retired_wallet_snapshot_and_intent_writes_are_rejected() {
        let instrument_id = Uuid::new_v4();
        let mut input = CreateSnapshotInput {
            account_id: None,
            wallet_id: Some(Uuid::new_v4()),
            as_of: Utc::now(),
            source: "web_manual".into(),
            merge_with_latest: false,
            items: vec![SnapshotItemInput {
                instrument_id,
                quantity: "1".into(),
                average_cost: None,
                market_price: None,
            }],
        };
        assert!(validate_stock_snapshot(&input).is_err());
        let mut scope = IntentScopeInput {
            instrument_ids: vec![instrument_id],
            account_id: None,
            wallet_id: input.wallet_id,
            position_slice: None,
        };
        assert!(validate_intent_scope_shape(&scope).is_err());
        input.wallet_id = None;
        input.account_id = Some(Uuid::new_v4());
        scope.wallet_id = None;
        scope.account_id = input.account_id;
        assert!(validate_stock_snapshot(&input).is_ok());
        assert!(validate_intent_scope_shape(&scope).is_ok());
    }

    #[test]
    fn a_trade_timestamp_must_be_strictly_after_the_latest_snapshot() {
        let latest = Utc::now();
        assert!(!trade_follows_latest_snapshot(latest, Some(latest)));
        assert!(!trade_follows_latest_snapshot(
            latest - chrono::Duration::microseconds(1),
            Some(latest)
        ));
        assert!(trade_follows_latest_snapshot(
            latest + chrono::Duration::microseconds(1),
            Some(latest)
        ));
        assert!(trade_follows_latest_snapshot(latest, None));
    }

    #[test]
    fn a_trade_never_promotes_its_execution_price_to_reference_price() {
        assert_eq!(reference_price_after_trade(None, "99.50"), None);
        assert_eq!(
            reference_price_after_trade(Some("101.25".into()), "99.50"),
            Some("101.25".into())
        );
    }
}
