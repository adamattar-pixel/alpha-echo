use std::{collections::HashSet, fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum Market {
    AShare,
    Hk,
    Us,
    Crypto,
    Meme,
}

impl fmt::Display for Market {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::AShare => "a_share",
            Self::Hk => "hk",
            Self::Us => "us",
            Self::Crypto => "crypto",
            Self::Meme => "meme",
        };
        formatter.write_str(value)
    }
}

impl FromStr for Market {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "a_share" | "a-share" | "a股" => Ok(Self::AShare),
            "hk" | "港股" => Ok(Self::Hk),
            "us" | "美股" => Ok(Self::Us),
            "crypto" => Ok(Self::Crypto),
            "meme" => Ok(Self::Meme),
            _ => Err("market must be one of a_share, hk, us, crypto or meme"),
        }
    }
}

pub fn normalize_chain_contract(chain: &str, contract_address: &str) -> String {
    let contract_address = contract_address.trim();
    match chain.trim().to_ascii_lowercase().as_str() {
        "ethereum" | "eth" | "bsc" | "bnb" | "base" | "arbitrum" | "optimism" | "polygon"
        | "avalanche" | "fantom" | "linea" | "blast" => contract_address.to_ascii_lowercase(),
        _ => contract_address.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum BriefingKind {
    Report,
}

impl fmt::Display for BriefingKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Report => "report",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    Complete,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotItemInput {
    pub instrument_id: Uuid,
    pub quantity: String,
    pub average_cost: Option<String>,
    pub market_price: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSnapshotInput {
    pub account_id: Option<Uuid>,
    pub wallet_id: Option<Uuid>,
    pub as_of: DateTime<Utc>,
    pub source: String,
    #[serde(default)]
    pub merge_with_latest: bool,
    pub items: Vec<SnapshotItemInput>,
}

pub fn validate_snapshot_input(input: &CreateSnapshotInput) -> Result<(), &'static str> {
    if input.account_id.is_some() == input.wallet_id.is_some() {
        return Err("exactly one of account_id or wallet_id is required");
    }
    if input.source.trim().is_empty() {
        return Err("source is required");
    }
    if input.items.is_empty() {
        return Err("a snapshot must contain at least one item");
    }
    for item in &input.items {
        let quantity = item
            .quantity
            .parse::<f64>()
            .map_err(|_| "quantity must be numeric")?;
        if !quantity.is_finite() {
            return Err("quantity must be finite");
        }
        for value in [&item.average_cost, &item.market_price]
            .into_iter()
            .flatten()
        {
            let number = value
                .parse::<f64>()
                .map_err(|_| "cost and price must be numeric")?;
            if !number.is_finite() || number < 0.0 {
                return Err("cost and price must be finite and non-negative");
            }
        }
    }

    let mut instrument_ids = HashSet::new();
    if input
        .items
        .iter()
        .any(|item| !instrument_ids.insert(item.instrument_id))
    {
        return Err("a snapshot cannot contain duplicate instruments");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use super::{
        CreateSnapshotInput, SnapshotItemInput, normalize_chain_contract, validate_snapshot_input,
    };

    fn item(instrument_id: Uuid) -> SnapshotItemInput {
        SnapshotItemInput {
            instrument_id,
            quantity: "10".into(),
            average_cost: None,
            market_price: None,
        }
    }

    #[test]
    fn snapshot_requires_exactly_one_owner() {
        let input = CreateSnapshotInput {
            account_id: None,
            wallet_id: None,
            as_of: Utc::now(),
            source: "manual".into(),
            merge_with_latest: false,
            items: vec![item(Uuid::new_v4())],
        };

        assert_eq!(
            validate_snapshot_input(&input),
            Err("exactly one of account_id or wallet_id is required")
        );
    }

    #[test]
    fn snapshot_rejects_duplicate_instruments() {
        let instrument_id = Uuid::new_v4();
        let input = CreateSnapshotInput {
            account_id: Some(Uuid::new_v4()),
            wallet_id: None,
            as_of: Utc::now(),
            source: "manual".into(),
            merge_with_latest: false,
            items: vec![item(instrument_id), item(instrument_id)],
        };

        assert_eq!(
            validate_snapshot_input(&input),
            Err("a snapshot cannot contain duplicate instruments")
        );
    }

    #[test]
    fn snapshot_rejects_non_finite_numbers() {
        let input = CreateSnapshotInput {
            account_id: Some(Uuid::new_v4()),
            wallet_id: None,
            as_of: Utc::now(),
            source: "manual".into(),
            merge_with_latest: false,
            items: vec![SnapshotItemInput {
                instrument_id: Uuid::new_v4(),
                quantity: "NaN".into(),
                average_cost: None,
                market_price: None,
            }],
        };

        assert_eq!(
            validate_snapshot_input(&input),
            Err("quantity must be finite")
        );
    }

    #[test]
    fn contract_identity_is_chain_specific() {
        assert_eq!(normalize_chain_contract("solana", "AbC123"), "AbC123");
        assert_eq!(normalize_chain_contract("base", "0xAbC123"), "0xabc123");
    }
}
