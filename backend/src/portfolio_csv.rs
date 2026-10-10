use std::str::FromStr;

use serde::Deserialize;

use crate::domain::Market;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPositionRow {
    pub market: Market,
    pub symbol: String,
    pub name: String,
    pub currency: Option<String>,
    pub exchange: Option<String>,
    pub quantity: String,
    pub average_cost: Option<String>,
    pub market_price: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CsvPositionRow {
    market: String,
    symbol: String,
    name: String,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    exchange: String,
    quantity: String,
    #[serde(default)]
    average_cost: String,
    #[serde(default)]
    market_price: String,
}

fn optional_text(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

pub fn parse_position_csv(input: &str) -> Result<Vec<ParsedPositionRow>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(input.as_bytes());
    let mut rows = Vec::new();

    for (index, result) in reader.deserialize::<CsvPositionRow>().enumerate() {
        let row = result.map_err(|error| format!("CSV row {} is invalid: {error}", index + 2))?;
        let quantity = row.quantity.parse::<f64>().ok();
        if row.symbol.is_empty()
            || row.name.is_empty()
            || quantity.is_none_or(|value| !value.is_finite())
        {
            return Err(format!(
                "CSV row {} requires symbol, name and numeric quantity",
                index + 2
            ));
        }
        for (label, value) in [
            ("average_cost", row.average_cost.as_str()),
            ("market_price", row.market_price.as_str()),
        ] {
            if !value.is_empty() {
                let number = value
                    .parse::<f64>()
                    .map_err(|_| format!("CSV row {} has invalid {label}", index + 2))?;
                if !number.is_finite() || number < 0.0 {
                    return Err(format!("CSV row {} has invalid {label}", index + 2));
                }
            }
        }

        rows.push(ParsedPositionRow {
            market: Market::from_str(&row.market)
                .map_err(|message| format!("CSV row {}: {message}", index + 2))?,
            symbol: row.symbol.to_ascii_uppercase(),
            name: row.name,
            currency: optional_text(row.currency).map(|value| value.to_ascii_uppercase()),
            exchange: optional_text(row.exchange).map(|value| value.to_ascii_uppercase()),
            quantity: row.quantity,
            average_cost: optional_text(row.average_cost),
            market_price: optional_text(row.market_price),
        });
    }

    if rows.is_empty() {
        return Err("CSV must contain at least one position row".into());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::parse_position_csv;
    use crate::domain::Market;

    #[test]
    fn parses_position_rows_and_normalizes_identity() {
        let rows = parse_position_csv(
            "market,symbol,name,currency,exchange,quantity,average_cost,market_price\n\
             us,aapl,Apple,usd,NASDAQ,2,180.5,190\n",
        )
        .expect("valid CSV");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].market, Market::Us);
        assert_eq!(rows[0].symbol, "AAPL");
        assert_eq!(rows[0].currency.as_deref(), Some("USD"));
        assert_eq!(rows[0].quantity, "2");
    }

    #[test]
    fn rejects_unknown_markets_with_row_number() {
        let error = parse_position_csv(
            "market,symbol,name,currency,exchange,quantity,average_cost,market_price\n\
             futures,ES,S&P futures,USD,CME,1,,\n",
        )
        .expect_err("unsupported market");

        assert!(error.contains("CSV row 2"));
        assert!(error.contains("market must be one of"));
    }
}
