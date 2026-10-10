use std::{sync::Arc, time::Duration};

use alpha_echo::{AppState, briefings, config::Config, http, market_data};
use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "alpha_echo=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    ensure_instrument_identity_can_migrate(&pool).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let state = AppState {
        pool,
        research: config.research.clone(),
    };
    let address = (config.host, config.port);
    let listener = TcpListener::bind(address).await?;

    let recovered = briefings::recover_interrupted_runs_on_startup(&state).await?;
    if recovered > 0 {
        tracing::warn!(
            recovered,
            "queued interrupted briefing runs for checkpoint resume"
        );
    }
    let scheduled = briefings::ensure_due_scheduled_runs(&state).await?;
    if scheduled > 0 {
        tracing::info!(scheduled, "created missed scheduled briefing runs");
    }
    tokio::spawn(briefings::run_worker(Arc::new(state.clone())));
    tokio::spawn(briefings::run_scheduler(Arc::new(state.clone())));
    tokio::spawn(market_data::run_quote_refresher(Arc::new(state.clone())));

    tracing::info!(?address, "alpha echo is listening");

    axum::serve(listener, http::router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn ensure_instrument_identity_can_migrate(pool: &sqlx::PgPool) -> Result<()> {
    let instruments_exist: bool =
        sqlx::query_scalar("SELECT to_regclass('public.instruments') IS NOT NULL")
            .fetch_one(pool)
            .await?;
    if !instruments_exist {
        return Ok(());
    }

    let migration_four_applied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version = 4 AND success)",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if migration_four_applied {
        return Ok(());
    }

    let ambiguous: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
             SELECT 1 FROM instruments
             WHERE contract_address IS NULL
             GROUP BY market, COALESCE(exchange, ''), symbol
             HAVING count(*) > 1
           )"#,
    )
    .fetch_one(pool)
    .await?;
    if ambiguous {
        anyhow::bail!(
            "cannot migrate listed instrument identity: duplicate market/symbol rows with a blank exchange exist; merge them before restarting Alpha Echo"
        );
    }
    Ok(())
}

async fn shutdown_signal() {
    let control_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = control_c => {},
        () = terminate => {},
    }
}
