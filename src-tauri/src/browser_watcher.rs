use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::browser::{is_public_web_url, redact_url_for_storage};

pub const BROWSER_WATCHER_PROTOCOL_VERSION: u16 = 1;
pub const BROWSER_WATCHER_MAX_GAP_MS: i64 = 45_000;
pub const BROWSER_WATCHER_FRESHNESS_MS: i64 = 45_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHeartbeat {
    pub protocol_version: u16,
    pub source_id: String,
    pub browser: String,
    #[serde(default)]
    pub profile: String,
    pub tab_id: String,
    pub captured_at_ms: i64,
    pub url: String,
    #[serde(default)]
    pub title: String,
    pub active: bool,
    #[serde(default)]
    pub private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserActivitySlice {
    pub id: String,
    pub source_id: String,
    pub browser: String,
    pub profile: String,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub url: String,
    pub domain: String,
    pub title: String,
    pub provenance: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserContext {
    pub browser: String,
    pub domain: String,
    pub title: String,
    pub captured_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidHeartbeat {
    source_id: String,
    browser: String,
    profile: String,
    tab_id: String,
    captured_at_ms: i64,
    url: String,
    domain: String,
    title: String,
}

#[derive(Debug, Default)]
pub struct BrowserWatcherEngine {
    previous_by_source: HashMap<String, ValidHeartbeat>,
}

impl BrowserWatcherEngine {
    pub fn ingest(
        &mut self,
        heartbeat: BrowserHeartbeat,
        received_at_ms: i64,
        excluded_domains: &[String],
    ) -> Result<Option<BrowserActivitySlice>, String> {
        validate_identifier("sourceId", &heartbeat.source_id, 128)?;
        validate_identifier("browser", &heartbeat.browser, 64)?;
        validate_identifier("tabId", &heartbeat.tab_id, 128)?;
        if heartbeat.protocol_version != BROWSER_WATCHER_PROTOCOL_VERSION {
            return Err(format!(
                "unsupported protocol version {}",
                heartbeat.protocol_version
            ));
        }
        if heartbeat.captured_at_ms <= 0
            || heartbeat.captured_at_ms.abs_diff(received_at_ms) > 5 * 60 * 1_000
        {
            return Err("heartbeat timestamp is outside the accepted clock window".into());
        }
        if heartbeat.private {
            self.previous_by_source.remove(&heartbeat.source_id);
            return Ok(None);
        }
        if !heartbeat.active {
            self.previous_by_source.remove(&heartbeat.source_id);
            return Ok(None);
        }
        if !is_public_web_url(&heartbeat.url) {
            self.previous_by_source.remove(&heartbeat.source_id);
            return Err("heartbeat URL must be a public HTTP(S) URL".into());
        }
        let url = redact_url_for_storage(&heartbeat.url);
        let domain = domain_from_url(&url).ok_or("heartbeat URL does not contain a domain")?;
        if matches_domain_exclusion(&domain, excluded_domains) {
            self.previous_by_source.remove(&heartbeat.source_id);
            return Ok(None);
        }
        let current = ValidHeartbeat {
            source_id: heartbeat.source_id.clone(),
            browser: truncate_text(&heartbeat.browser, 64),
            profile: truncate_text(&heartbeat.profile, 128),
            tab_id: heartbeat.tab_id,
            captured_at_ms: heartbeat.captured_at_ms,
            url,
            domain,
            title: truncate_text(&heartbeat.title, 240),
        };
        let previous = self
            .previous_by_source
            .insert(heartbeat.source_id, current.clone());
        let Some(previous) = previous else {
            return Ok(None);
        };
        let elapsed_ms = current
            .captured_at_ms
            .saturating_sub(previous.captured_at_ms);
        if elapsed_ms <= 0
            || elapsed_ms > BROWSER_WATCHER_MAX_GAP_MS
            || current.tab_id != previous.tab_id
            || current.url != previous.url
        {
            return Ok(None);
        }
        let identity = format!(
            "{}\n{}\n{}\n{}",
            current.source_id, previous.captured_at_ms, current.captured_at_ms, current.url
        );
        let hash = format!("{:x}", Sha256::digest(identity.as_bytes()));
        Ok(Some(BrowserActivitySlice {
            id: format!("browser-heartbeat-{}", &hash[..24]),
            source_id: current.source_id,
            browser: current.browser,
            profile: current.profile,
            started_at_ms: previous.captured_at_ms,
            ended_at_ms: current.captured_at_ms,
            url: current.url,
            domain: current.domain,
            title: current.title,
            provenance: "watcher-heartbeat-v1".into(),
        }))
    }

    pub fn current_context(&self, now_ms: i64) -> Option<BrowserContext> {
        self.previous_by_source
            .values()
            .filter(|heartbeat| {
                now_ms.saturating_sub(heartbeat.captured_at_ms) <= BROWSER_WATCHER_FRESHNESS_MS
            })
            .max_by_key(|heartbeat| heartbeat.captured_at_ms)
            .map(|heartbeat| BrowserContext {
                browser: heartbeat.browser.clone(),
                domain: heartbeat.domain.clone(),
                title: heartbeat.title.clone(),
                captured_at_ms: heartbeat.captured_at_ms,
            })
    }
}

pub fn is_browser_application(app: &str) -> bool {
    let normalized = app.to_ascii_lowercase();
    ["chrome", "edge", "brave", "arc", "firefox", "safari"]
        .iter()
        .any(|candidate| normalized.contains(candidate))
}

fn validate_identifier(label: &str, value: &str, max_len: usize) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > max_len {
        return Err(format!("{label} must contain 1 to {max_len} characters"));
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(format!("{label} contains unsupported characters"));
    }
    Ok(())
}

fn domain_from_url(value: &str) -> Option<String> {
    url::Url::parse(value)
        .ok()?
        .host_str()
        .map(|domain| domain.trim_start_matches("www.").to_ascii_lowercase())
}

fn matches_domain_exclusion(domain: &str, exclusions: &[String]) -> bool {
    let normalized = domain
        .trim()
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    exclusions.iter().any(|entry| {
        let entry = entry.trim().trim_start_matches("www.").to_ascii_lowercase();
        !entry.is_empty() && (normalized == entry || normalized.ends_with(&format!(".{entry}")))
    })
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        BROWSER_WATCHER_PROTOCOL_VERSION, BrowserHeartbeat, BrowserWatcherEngine,
        is_browser_application,
    };

    fn heartbeat(at_ms: i64) -> BrowserHeartbeat {
        BrowserHeartbeat {
            protocol_version: BROWSER_WATCHER_PROTOCOL_VERSION,
            source_id: "firefox-profile-1".into(),
            browser: "Firefox".into(),
            profile: "default".into(),
            tab_id: "42".into(),
            captured_at_ms: at_ms,
            url: "https://example.com/docs?token=secret&section=one#private".into(),
            title: "Documentation".into(),
            active: true,
            private: false,
        }
    }

    #[test]
    fn creates_measured_slices_only_between_matching_fresh_heartbeats() {
        let mut engine = BrowserWatcherEngine::default();
        assert_eq!(engine.ingest(heartbeat(10_000), 10_000, &[]), Ok(None));

        let slice = engine
            .ingest(heartbeat(40_000), 40_000, &[])
            .expect("heartbeat should be valid")
            .expect("second heartbeat should create a slice");

        assert_eq!(slice.started_at_ms, 10_000);
        assert_eq!(slice.ended_at_ms, 40_000);
        assert_eq!(slice.domain, "example.com");
        assert_eq!(slice.provenance, "watcher-heartbeat-v1");
        assert!(slice.url.contains("token=%5BREDACTED%"));
        assert!(!slice.url.contains("#private"));
    }

    #[test]
    fn rejects_gaps_private_tabs_and_excluded_domains() {
        let mut engine = BrowserWatcherEngine::default();
        engine.ingest(heartbeat(10_000), 10_000, &[]).unwrap();
        assert_eq!(engine.ingest(heartbeat(60_001), 60_001, &[]), Ok(None));

        let mut private = heartbeat(70_000);
        private.private = true;
        assert_eq!(engine.ingest(private, 70_000, &[]), Ok(None));
        assert_eq!(
            engine.ingest(heartbeat(80_000), 80_000, &["example.com".into()]),
            Ok(None)
        );
        assert!(engine.current_context(80_000).is_none());
    }

    #[test]
    fn exposes_only_fresh_browser_context() {
        let mut engine = BrowserWatcherEngine::default();
        engine.ingest(heartbeat(10_000), 10_000, &[]).unwrap();
        assert_eq!(
            engine.current_context(20_000).map(|context| context.domain),
            Some("example.com".into())
        );
        assert!(engine.current_context(60_000).is_none());
        assert!(is_browser_application("Google Chrome"));
        assert!(!is_browser_application("Visual Studio Code"));
    }
}
