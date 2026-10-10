use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReportLanguage {
    En,
    #[default]
    Zh,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_english_selection_and_legacy_chinese_are_distinct() {
        assert_eq!(ReportLanguage::from_input(&json!({})), ReportLanguage::Zh);
        assert_eq!(
            ReportLanguage::from_input(&json!({"frozen_portfolio": {"report_language": "en"}})),
            ReportLanguage::En
        );
        assert_eq!(
            serde_json::from_str::<ReportLanguage>("\"en\"").unwrap(),
            ReportLanguage::En
        );
        assert!(serde_json::from_str::<ReportLanguage>("\"fr\"").is_err());
    }
}

impl ReportLanguage {
    pub fn english() -> Self {
        Self::En
    }

    // Older immutable reports predate language selection and were Chinese.
    pub fn from_input(input: &Value) -> Self {
        let frozen = input.get("frozen_portfolio").unwrap_or(input);
        match frozen.get("report_language").and_then(Value::as_str) {
            Some("en") => Self::En,
            _ => Self::Zh,
        }
    }

    pub fn name(self) -> &'static str {
        self.text("Simplified Chinese", "English")
    }

    pub fn text<'a>(self, zh: &'a str, en: &'a str) -> &'a str {
        match self {
            Self::Zh => zh,
            Self::En => en,
        }
    }
}
