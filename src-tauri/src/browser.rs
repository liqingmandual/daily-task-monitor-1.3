use std::error::Error;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use html5gum::{Token, Tokenizer};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use url::{Host, Url};

const CHROMIUM_EPOCH_OFFSET_MS: i64 = 11_644_473_600_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVisit {
    pub url: String,
    pub title: String,
    pub visited_at_ms: i64,
}

pub trait BrowserAdapter {
    fn browser_name(&self) -> &str;
    fn profile_name(&self) -> &str;
    fn scan(
        &self,
        start_ms: i64,
        end_ms: i64,
        limit: usize,
    ) -> Result<Vec<BrowserVisit>, Box<dyn Error + Send + Sync>>;
}

pub struct ChromiumHistoryAdapter<'a> {
    pub browser: &'a str,
    pub profile: &'a str,
    pub history_path: &'a Path,
}

impl BrowserAdapter for ChromiumHistoryAdapter<'_> {
    fn browser_name(&self) -> &str {
        self.browser
    }

    fn profile_name(&self) -> &str {
        self.profile
    }

    fn scan(
        &self,
        start_ms: i64,
        end_ms: i64,
        limit: usize,
    ) -> Result<Vec<BrowserVisit>, Box<dyn Error + Send + Sync>> {
        scan_chromium_history(self.history_path, start_ms, end_ms, limit)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HtmlSummary {
    pub title: String,
    pub description: String,
    pub text: String,
}

pub fn chromium_time_to_unix_ms(chromium_microseconds: i64) -> i64 {
    chromium_microseconds / 1_000 - CHROMIUM_EPOCH_OFFSET_MS
}

pub fn scan_chromium_history(
    history_path: &Path,
    start_ms: i64,
    end_ms: i64,
    limit: usize,
) -> Result<Vec<BrowserVisit>, Box<dyn Error + Send + Sync>> {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let copy_path = std::env::temp_dir().join(format!(
        "daily-task-monitor-history-{}-{suffix}.sqlite",
        std::process::id()
    ));
    fs::copy(history_path, &copy_path)?;

    let source_name = history_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("History");
    let copy_name = copy_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("History");
    let mut sidecars = Vec::new();
    for suffix in ["-wal", "-shm"] {
        let source = history_path.with_file_name(format!("{source_name}{suffix}"));
        let copied = copy_path.with_file_name(format!("{copy_name}{suffix}"));
        if source.exists() {
            fs::copy(&source, &copied)?;
            sidecars.push(copied);
        }
    }

    let result = query_copied_history(&copy_path, start_ms, end_ms, limit);
    let _ = fs::remove_file(&copy_path);
    for sidecar in sidecars {
        let _ = fs::remove_file(sidecar);
    }
    result.map_err(Into::into)
}

fn query_copied_history(
    copied_path: &Path,
    start_ms: i64,
    end_ms: i64,
    limit: usize,
) -> rusqlite::Result<Vec<BrowserVisit>> {
    let connection = Connection::open_with_flags(copied_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let start_chromium = (start_ms + CHROMIUM_EPOCH_OFFSET_MS) * 1_000;
    let end_chromium = (end_ms + CHROMIUM_EPOCH_OFFSET_MS) * 1_000;
    let mut statement = connection.prepare(
        "SELECT urls.url, urls.title, visits.visit_time
         FROM visits
         JOIN urls ON urls.id = visits.url
         WHERE visits.visit_time BETWEEN ?1 AND ?2
         ORDER BY visits.visit_time ASC
         LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![start_chromium, end_chromium, limit.clamp(1, 2_000) as i64],
        |row| {
            Ok(BrowserVisit {
                url: row.get(0)?,
                title: row.get(1)?,
                visited_at_ms: chromium_time_to_unix_ms(row.get(2)?),
            })
        },
    )?;
    rows.collect()
}

pub fn is_public_web_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    match url.host() {
        Some(Host::Domain(domain)) => {
            let domain = domain.to_ascii_lowercase();
            domain.contains('.')
                && domain != "localhost"
                && !domain.ends_with(".localhost")
                && !domain.ends_with(".local")
        }
        Some(Host::Ipv4(ip)) => is_public_ip(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => is_public_ip(IpAddr::V6(ip)),
        None => false,
    }
}

pub fn redact_url_for_storage(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return String::new();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let normalized = key.to_ascii_lowercase();
            let sensitive = [
                "token",
                "access_token",
                "refresh_token",
                "auth",
                "authorization",
                "api_key",
                "apikey",
                "key",
                "code",
                "password",
                "passwd",
                "session",
                "signature",
                "sig",
            ]
            .iter()
            .any(|candidate| {
                normalized == *candidate || normalized.ends_with(&format!("_{candidate}"))
            });
            (
                key.into_owned(),
                if sensitive {
                    "[REDACTED]".into()
                } else {
                    value.into_owned()
                },
            )
        })
        .collect();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.to_string()
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_unspecified())
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(mapped));
            }
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast())
        }
    }
}

#[cfg(feature = "desktop")]
pub async fn fetch_public_html_summary(
    value: &str,
    max_chars: usize,
) -> Result<HtmlSummary, Box<dyn Error + Send + Sync>> {
    const MAX_RESPONSE_BYTES: usize = 1_048_576;
    let mut current = Url::parse(value)?;

    for _ in 0..=3 {
        let address = ensure_public_destination(&current).await?;
        let host = current.host_str().ok_or("URL has no host")?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Orbit/1.3 (+local public-page summary)")
            .resolve(host, address)
            .build()?;
        let mut response = client.get(current.clone()).send().await?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or("Redirect did not contain a location")?
                .to_str()?;
            current = current.join(location)?;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("Page returned HTTP {}", response.status()).into());
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !content_type.contains("text/html") && !content_type.contains("application/xhtml") {
            return Err("Page is not HTML".into());
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
        {
            return Err("Page exceeds the response size limit".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err("Page exceeds the response size limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok(extract_html_summary(
            &String::from_utf8_lossy(&bytes),
            max_chars.min(2_000),
        ));
    }
    Err("Page redirected too many times".into())
}

#[cfg(feature = "desktop")]
async fn ensure_public_destination(
    url: &Url,
) -> Result<std::net::SocketAddr, Box<dyn Error + Send + Sync>> {
    if !is_public_web_url(url.as_str()) {
        return Err("Only public HTTP(S) pages may be fetched".into());
    }
    let host = url.host_str().ok_or("URL has no host")?;
    let port = url.port_or_known_default().ok_or("URL has no known port")?;
    let addresses: Vec<_> = tokio::net::lookup_host((host, port)).await?.collect();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err("Destination resolved to a private or unavailable address".into());
    }
    Ok(addresses[0])
}

pub fn extract_html_summary(html: &str, max_chars: usize) -> HtmlSummary {
    let mut summary = HtmlSummary::default();
    let mut title_depth = 0_u32;
    let mut ignored_depth = 0_u32;
    let mut text_parts = Vec::new();

    for token in Tokenizer::new(html).flatten() {
        match token {
            Token::StartTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name).to_ascii_lowercase();
                if name == "title" {
                    title_depth += 1;
                }
                if matches!(name.as_str(), "script" | "style" | "noscript" | "svg") {
                    ignored_depth += 1;
                }
                if name == "meta" {
                    let attribute = |key: &[u8]| {
                        tag.attributes
                            .get(key)
                            .map(|value| String::from_utf8_lossy(value).into_owned())
                    };
                    let meta_name = attribute(b"name").or_else(|| attribute(b"property"));
                    if meta_name.as_deref().is_some_and(|value| {
                        matches!(
                            value.to_ascii_lowercase().as_str(),
                            "description" | "og:description"
                        )
                    }) {
                        summary.description =
                            normalize_text(&attribute(b"content").unwrap_or_default());
                    }
                }
            }
            Token::EndTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name).to_ascii_lowercase();
                if name == "title" {
                    title_depth = title_depth.saturating_sub(1);
                }
                if matches!(name.as_str(), "script" | "style" | "noscript" | "svg") {
                    ignored_depth = ignored_depth.saturating_sub(1);
                }
            }
            Token::String(value) => {
                let value = normalize_text(&String::from_utf8_lossy(&value));
                if value.is_empty() {
                    continue;
                }
                if title_depth > 0 {
                    summary.title.push_str(&value);
                } else if ignored_depth == 0 {
                    text_parts.push(value);
                }
            }
            _ => {}
        }
    }

    summary.title = normalize_text(&summary.title);
    summary.text = text_parts.join(" ").chars().take(max_chars).collect();
    summary
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
