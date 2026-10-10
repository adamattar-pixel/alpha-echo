pub mod briefings;
pub mod config;
pub mod domain;
pub mod error;
pub mod http;
pub mod intelligence;
pub mod market_data;
pub mod news;
pub mod portfolio;
pub mod portfolio_csv;
pub mod report_language;
pub mod research;

use config::ResearchRunnerConfig;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub research: ResearchRunnerConfig,
}
