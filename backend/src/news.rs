use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use tokio::task::JoinSet;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct NewsItem {
    pub title: String,
    pub url: String,
    pub publisher: Option<String>,
    pub published_at: DateTime<Utc>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
struct EquityNewsTarget {
    instrument_id: Uuid,
    market: String,
    symbol: String,
    name: String,
}

#[derive(Debug, Serialize)]
struct FeedCapture {
    provider: &'static str,
    url: String,
    items: Vec<NewsItem>,
    error: Option<String>,
}

pub struct PreparedEquityNews {
    captures: Vec<(EquityNewsTarget, Vec<FeedCapture>)>,
    observed_at: DateTime<Utc>,
}

const EQUITY_NEWS_TARGETS: &str = r#"WITH latest AS (
         SELECT DISTINCT ON (ps.account_id) ps.id
         FROM position_snapshots ps
         WHERE ps.account_id IS NOT NULL AND ps.as_of <= $1
         ORDER BY ps.account_id, ps.as_of DESC, ps.created_at DESC, ps.id DESC
       )
       SELECT DISTINCT i.id AS instrument_id, i.market, i.symbol, i.name
       FROM latest l
       JOIN position_snapshot_items psi ON psi.snapshot_id = l.id
       JOIN instruments i ON i.id = psi.instrument_id
       WHERE psi.quantity <> 0 AND i.market IN ('a_share', 'hk', 'us')
       ORDER BY i.market, i.symbol"#;

pub async fn prepare_equity_news(
    pool: &PgPool,
    positions_as_of: DateTime<Utc>,
) -> Result<PreparedEquityNews, String> {
    let targets = sqlx::query_as::<_, EquityNewsTarget>(EQUITY_NEWS_TARGETS)
        .bind(positions_as_of)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    if targets.is_empty() {
        return Ok(PreparedEquityNews {
            captures: Vec::new(),
            observed_at: Utc::now(),
        });
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Alpha-Echo/0.1 local portfolio research")
        .build()
        .map_err(|error| error.to_string())?;
    let window_start = positions_as_of - chrono::Duration::days(180);
    let mut pending = JoinSet::new();
    for target in targets {
        let owned_client = client.clone();
        pending.spawn(async move {
            let feeds =
                collect_target_news(&owned_client, &target, positions_as_of, window_start).await;
            (target, feeds)
        });
    }

    let mut captures = Vec::new();
    while let Some(result) = pending.join_next().await {
        captures.push(result.map_err(|error| error.to_string())?);
    }
    captures.sort_by(|left, right| left.0.symbol.cmp(&right.0.symbol));
    Ok(PreparedEquityNews {
        captures,
        observed_at: Utc::now(),
    })
}

pub async fn persist_equity_news(
    transaction: &mut Transaction<'_, Postgres>,
    prepared: PreparedEquityNews,
    report_as_of: DateTime<Utc>,
) -> Result<(), String> {
    let current_targets = sqlx::query_as::<_, EquityNewsTarget>(EQUITY_NEWS_TARGETS)
        .bind(report_as_of)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| error.to_string())?;
    let prepared_ids = prepared
        .captures
        .iter()
        .map(|(target, _)| target.instrument_id)
        .collect::<std::collections::HashSet<_>>();
    let current_ids = current_targets
        .iter()
        .map(|target| target.instrument_id)
        .collect::<std::collections::HashSet<_>>();
    if prepared_ids != current_ids {
        return Err(
            "portfolio changed while equity news was being prepared; retry report creation".into(),
        );
    }
    for (target, feeds) in prepared.captures {
        let mut items = feeds
            .iter()
            .flat_map(|feed| feed.items.iter().cloned())
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.published_at.cmp(&left.published_at));
        items.dedup_by(|left, right| left.title.eq_ignore_ascii_case(&right.title));
        items.truncate(20);
        let payload = json!({
            "instrument_id": target.instrument_id,
            "market": target.market,
            "symbol": target.symbol,
            "name": target.name,
            "items": items,
            "feeds": feeds,
        });
        let checksum = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&payload).map_err(|error| error.to_string())?)
        );
        sqlx::query(
            r#"INSERT INTO source_snapshots
               (provider, source_type, captured_at, as_of, request, payload, checksum)
               VALUES ('equity_news_rss', 'equity_news', now(), $1, $2, $3, $4)"#,
        )
        .bind(prepared.observed_at)
        .bind(json!({
            "instrument_id": target.instrument_id,
            "symbol": target.symbol,
            "name": target.name,
            "lookback_days": 180,
        }))
        .bind(payload)
        .bind(checksum)
        .execute(&mut **transaction)
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn collect_target_news(
    client: &reqwest::Client,
    target: &EquityNewsTarget,
    cutoff: DateTime<Utc>,
    window_start: DateTime<Utc>,
) -> Vec<FeedCapture> {
    let query = format!("\"{}\" \"{}\"", target.symbol, target.name);
    let google_url = reqwest::Url::parse_with_params(
        "https://news.google.com/rss/search",
        &[
            ("q", format!("{query} when:180d")),
            ("hl", "zh-CN".to_string()),
            (
                "gl",
                if target.market == "us" { "US" } else { "CN" }.to_string(),
            ),
            (
                "ceid",
                if target.market == "us" {
                    "US:zh-Hans"
                } else {
                    "CN:zh-Hans"
                }
                .to_string(),
            ),
        ],
    )
    .expect("static Google News URL is valid")
    .to_string();
    let bing_url = reqwest::Url::parse_with_params(
        "https://www.bing.com/news/search",
        &[("q", query), ("format", "RSS".to_string())],
    )
    .expect("static Bing News URL is valid")
    .to_string();
    let (google, bing) = tokio::join!(
        fetch_feed(client, "google_news", google_url, cutoff, window_start),
        fetch_feed(client, "bing_news", bing_url, cutoff, window_start),
    );
    vec![google, bing]
}

async fn fetch_feed(
    client: &reqwest::Client,
    provider: &'static str,
    url: String,
    cutoff: DateTime<Utc>,
    window_start: DateTime<Utc>,
) -> FeedCapture {
    match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => match response.text().await {
            Ok(xml) => match parse_rss_news(&xml, cutoff, window_start) {
                Ok(items) => FeedCapture {
                    provider,
                    url,
                    items,
                    error: None,
                },
                Err(error) => FeedCapture {
                    provider,
                    url,
                    items: Vec::new(),
                    error: Some(error),
                },
            },
            Err(error) => FeedCapture {
                provider,
                url,
                items: Vec::new(),
                error: Some(format!("response body failed: {error}")),
            },
        },
        Ok(response) => FeedCapture {
            provider,
            url,
            items: Vec::new(),
            error: Some(format!("HTTP {}", response.status())),
        },
        Err(error) => FeedCapture {
            provider,
            url,
            items: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

pub fn parse_rss_news(
    xml: &str,
    cutoff: DateTime<Utc>,
    window_start: DateTime<Utc>,
) -> Result<Vec<NewsItem>, String> {
    if !xml.contains("<rss") && !xml.contains("<feed") {
        return Err("response is not a recognized RSS/Atom document".into());
    }
    Ok(xml
        .split("<item")
        .skip(1)
        .filter_map(|chunk| {
            let item = chunk.split_once("</item>")?.0;
            let title = xml_element(item, "title")?;
            let url = normalize_news_url(&decode_xml(&xml_element(item, "link")?));
            let published_at = DateTime::parse_from_rfc2822(&xml_element(item, "pubDate")?)
                .ok()?
                .with_timezone(&Utc);
            (published_at >= window_start && published_at <= cutoff).then(|| NewsItem {
                title: decode_xml(&title),
                url,
                publisher: xml_element(item, "source")
                    .or_else(|| xml_element(item, "News:Source"))
                    .map(|value| decode_xml(&value)),
                published_at,
            })
        })
        .collect())
}

fn normalize_news_url(value: &str) -> String {
    let Ok(url) = reqwest::Url::parse(value) else {
        return value.to_string();
    };
    if url
        .host_str()
        .is_some_and(|host| host.ends_with("bing.com"))
        && let Some((_, original)) = url.query_pairs().find(|(key, _)| key == "url")
    {
        return original.into_owned();
    }
    value.to_string()
}

fn xml_element(input: &str, tag: &str) -> Option<String> {
    let start_marker = format!("<{tag}");
    let start = input.find(&start_marker)?;
    let content_start = input[start..].find('>')? + start + 1;
    let end = input[content_start..].find(&format!("</{tag}>"))? + content_start;
    Some(
        input[content_start..end]
            .trim()
            .trim_start_matches("<![CDATA[")
            .trim_end_matches("]]>")
            .trim()
            .to_string(),
    )
}

fn decode_xml(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_news_is_frozen_only_when_published_by_the_report_cutoff() {
        let cutoff = DateTime::parse_from_rfc3339("2026-08-14T12:08:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let rss = r#"<rss><channel>
          <item><title>Drone tariffs lift ONDS &amp; peers</title><link>https://example.com/onds</link><pubDate>Fri, 14 Aug 2026 11:05:00 GMT</pubDate><source>Example News</source></item>
          <item><title>Future ONDS story</title><link>https://example.com/future</link><pubDate>Fri, 14 Aug 2026 13:05:00 GMT</pubDate><source>Example News</source></item>
        </channel></rss>"#;

        let items = parse_rss_news(rss, cutoff, cutoff - chrono::Duration::days(180)).unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Drone tariffs lift ONDS & peers");
        assert_eq!(items[0].publisher.as_deref(), Some("Example News"));
        assert_eq!(items[0].url, "https://example.com/onds");
        assert_eq!(
            items[0].published_at.to_rfc3339(),
            "2026-08-14T11:05:00+00:00"
        );
    }

    #[test]
    fn bing_news_keeps_the_publisher_and_original_article_url() {
        let cutoff = DateTime::parse_from_rfc3339("2026-08-14T12:08:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let rss = r#"<rss xmlns:News="https://www.bing.com/news"><channel><item>
          <title>ONDAS raises guidance</title>
          <link>http://www.bing.com/news/apiclick.aspx?url=https%3a%2f%2fexample.com%2fonds%3fa%3d1&amp;c=42</link>
          <pubDate>Thu, 13 Aug 2026 09:21:00 GMT</pubDate>
          <News:Source>Example News</News:Source>
        </item></channel></rss>"#;

        let items = parse_rss_news(rss, cutoff, cutoff - chrono::Duration::days(180)).unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].publisher.as_deref(), Some("Example News"));
        assert_eq!(items[0].url, "https://example.com/onds?a=1");
    }

    #[test]
    fn malformed_success_response_is_an_auditable_feed_error() {
        let cutoff = Utc::now();
        assert!(parse_rss_news("", cutoff, cutoff - chrono::Duration::days(180)).is_err());
    }
}
