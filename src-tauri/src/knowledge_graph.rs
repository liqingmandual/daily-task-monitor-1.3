use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};

use crate::db::{ActivitySegmentRecord, BrowserVisitRecord, category_key};

const VISIT_MATCH_WINDOW_MS: i64 = 5 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphNodeKind {
    Category,
    App,
    Domain,
    Day,
    Activity,
    BrowserVisit,
}

impl GraphNodeKind {
    pub fn as_key(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::App => "app",
            Self::Domain => "domain",
            Self::Day => "day",
            Self::Activity => "activity",
            Self::BrowserVisit => "browser-visit",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphFilters {
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub node_types: Vec<GraphNodeKind>,
    #[serde(default)]
    pub timezone_offset_minutes: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphNode {
    pub id: String,
    pub kind: GraphNodeKind,
    pub label: String,
    pub duration_seconds: i64,
    pub category: String,
    pub confidence: f32,
    pub occurred_at_ms: Option<i64>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphLink {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub weight_seconds: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphPayload {
    pub nodes: Vec<KnowledgeGraphNode>,
    pub links: Vec<KnowledgeGraphLink>,
    pub counts: BTreeMap<String, usize>,
    pub total_seconds: i64,
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Default)]
struct HubAccumulator {
    duration_seconds: i64,
    weighted_confidence: f64,
    confidence_weight: i64,
}

pub fn build_knowledge_graph(
    mut segments: Vec<ActivitySegmentRecord>,
    visits: Vec<BrowserVisitRecord>,
    start_ms: i64,
    end_ms: i64,
    filters: &KnowledgeGraphFilters,
    excluded_apps: &[String],
    excluded_domains: &[String],
) -> KnowledgeGraphPayload {
    segments.retain(|segment| {
        !is_exact_excluded(&segment.app, excluded_apps)
            && matches_search(&filters.search, &[&segment.app, &segment.title])
    });
    segments.sort_by_key(|segment| segment.started_at_ms);

    let visits: Vec<_> = visits
        .into_iter()
        .filter(|visit| {
            !is_domain_excluded(&visit.domain, excluded_domains)
                && matches_search(
                    &filters.search,
                    &[&visit.browser, &visit.domain, &visit.title, &visit.url],
                )
        })
        .collect();

    let mut nodes = Vec::with_capacity(segments.len() + visits.len() + 64);
    let mut links = Vec::with_capacity(segments.len() * 3 + visits.len() * 2);
    let mut hubs: HashMap<(GraphNodeKind, String), HubAccumulator> = HashMap::new();
    let mut total_seconds = 0_i64;

    for segment in &segments {
        let clipped_start = segment.started_at_ms.max(start_ms);
        let clipped_end = segment.ended_at_ms.min(end_ms).max(clipped_start);
        let duration_seconds = (clipped_end - clipped_start) / 1_000;
        total_seconds += duration_seconds;
        let category = category_key(segment.category).to_string();
        let app_key = normalize_key(&segment.app);
        let day_label = local_day_label(segment.started_at_ms, filters.timezone_offset_minutes);
        let activity_id = format!("activity:{}", segment.id);
        let app_id = format!("app:{app_key}");
        let category_id = format!("category:{category}");
        let day_id = format!("day:{day_label}");

        let mut metadata = BTreeMap::new();
        metadata.insert("app".into(), segment.app.clone());
        metadata.insert(
            "source".into(),
            format!("{:?}", segment.source).to_lowercase(),
        );
        metadata.insert("reason".into(), segment.reason.clone());
        nodes.push(KnowledgeGraphNode {
            id: activity_id.clone(),
            kind: GraphNodeKind::Activity,
            label: fallback_label(&segment.title, &segment.app),
            duration_seconds,
            category: category.clone(),
            confidence: segment.confidence,
            occurred_at_ms: Some(segment.started_at_ms),
            metadata,
        });

        add_hub(
            &mut hubs,
            GraphNodeKind::App,
            &segment.app,
            duration_seconds,
            segment.confidence,
        );
        add_hub(
            &mut hubs,
            GraphNodeKind::Category,
            &category,
            duration_seconds,
            segment.confidence,
        );
        add_hub(
            &mut hubs,
            GraphNodeKind::Day,
            &day_label,
            duration_seconds,
            segment.confidence,
        );
        links.push(graph_link(
            &activity_id,
            &app_id,
            "used-app",
            duration_seconds,
        ));
        links.push(graph_link(
            &activity_id,
            &category_id,
            "classified-as",
            duration_seconds,
        ));
        links.push(graph_link(
            &activity_id,
            &day_id,
            "occurred-on",
            duration_seconds,
        ));
    }

    for visit in &visits {
        let visit_id = format!("visit:{}", visit.id);
        let domain_key = normalize_key(&visit.domain);
        let domain_id = format!("domain:{domain_key}");
        let mut metadata = BTreeMap::new();
        metadata.insert("browser".into(), visit.browser.clone());
        metadata.insert("domain".into(), visit.domain.clone());
        metadata.insert("url".into(), visit.url.clone());
        nodes.push(KnowledgeGraphNode {
            id: visit_id.clone(),
            kind: GraphNodeKind::BrowserVisit,
            label: fallback_label(&visit.title, &visit.domain),
            duration_seconds: 0,
            category: "research".into(),
            confidence: 0.72,
            occurred_at_ms: Some(visit.visited_at_ms),
            metadata,
        });
        add_hub(&mut hubs, GraphNodeKind::Domain, &visit.domain, 1, 0.72);
        links.push(graph_link(&visit_id, &domain_id, "visited-domain", 1));

        if let Some(segment) = nearest_segment(&segments, visit.visited_at_ms) {
            links.push(graph_link(
                &visit_id,
                &format!("activity:{}", segment.id),
                "browser-context",
                1,
            ));
        }
    }

    for ((kind, label), accumulator) in hubs {
        let key = normalize_key(&label);
        let id = format!("{}:{key}", kind.as_key());
        let confidence = if accumulator.confidence_weight > 0 {
            (accumulator.weighted_confidence / accumulator.confidence_weight as f64) as f32
        } else {
            1.0
        };
        nodes.push(KnowledgeGraphNode {
            id,
            kind,
            label: label.clone(),
            duration_seconds: accumulator.duration_seconds,
            category: if kind == GraphNodeKind::Category {
                label
            } else {
                String::new()
            },
            confidence,
            occurred_at_ms: None,
            metadata: BTreeMap::new(),
        });
    }

    if !filters.node_types.is_empty() {
        nodes.retain(|node| filters.node_types.contains(&node.kind));
        let retained: std::collections::HashSet<_> =
            nodes.iter().map(|node| node.id.as_str()).collect();
        links.retain(|link| {
            retained.contains(link.source.as_str()) && retained.contains(link.target.as_str())
        });
    }

    let mut counts = BTreeMap::new();
    for node in &nodes {
        *counts.entry(node.kind.as_key().to_string()).or_insert(0) += 1;
    }

    KnowledgeGraphPayload {
        nodes,
        links,
        counts,
        total_seconds,
        start_ms,
        end_ms,
    }
}

fn add_hub(
    hubs: &mut HashMap<(GraphNodeKind, String), HubAccumulator>,
    kind: GraphNodeKind,
    label: &str,
    duration_seconds: i64,
    confidence: f32,
) {
    let accumulator = hubs.entry((kind, label.to_string())).or_default();
    accumulator.duration_seconds += duration_seconds;
    accumulator.weighted_confidence += confidence as f64 * duration_seconds.max(1) as f64;
    accumulator.confidence_weight += duration_seconds.max(1);
}

fn graph_link(source: &str, target: &str, kind: &str, weight_seconds: i64) -> KnowledgeGraphLink {
    KnowledgeGraphLink {
        source: source.into(),
        target: target.into(),
        kind: kind.into(),
        weight_seconds,
    }
}

fn normalize_key(value: &str) -> String {
    let normalized: String = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();
    normalized.trim_matches('-').to_string()
}

fn fallback_label(primary: &str, fallback: &str) -> String {
    let primary = primary.trim();
    if primary.is_empty() {
        fallback.trim().to_string()
    } else {
        primary.to_string()
    }
}

fn matches_search(search: &str, values: &[&str]) -> bool {
    let search = search.trim().to_ascii_lowercase();
    search.is_empty()
        || values
            .iter()
            .any(|value| value.to_ascii_lowercase().contains(&search))
}

fn is_exact_excluded(value: &str, exclusions: &[String]) -> bool {
    let value = value.trim().to_ascii_lowercase();
    exclusions.iter().any(|excluded| value == *excluded)
}

fn is_domain_excluded(value: &str, exclusions: &[String]) -> bool {
    let value = value.trim().to_ascii_lowercase();
    exclusions
        .iter()
        .any(|excluded| value == *excluded || value.ends_with(&format!(".{excluded}")))
}

fn nearest_segment(
    segments: &[ActivitySegmentRecord],
    visited_at_ms: i64,
) -> Option<&ActivitySegmentRecord> {
    if segments.is_empty() {
        return None;
    }
    let insertion = segments.partition_point(|segment| segment.started_at_ms <= visited_at_ms);
    let mut candidates = Vec::with_capacity(2);
    if insertion > 0 {
        candidates.push(&segments[insertion - 1]);
    }
    if insertion < segments.len() {
        candidates.push(&segments[insertion]);
    }
    candidates
        .into_iter()
        .filter(|segment| {
            visited_at_ms >= segment.started_at_ms - VISIT_MATCH_WINDOW_MS
                && visited_at_ms <= segment.ended_at_ms + VISIT_MATCH_WINDOW_MS
        })
        .min_by_key(|segment| {
            if visited_at_ms < segment.started_at_ms {
                segment.started_at_ms - visited_at_ms
            } else if visited_at_ms > segment.ended_at_ms {
                visited_at_ms - segment.ended_at_ms
            } else {
                0
            }
        })
}

fn local_day_label(timestamp_ms: i64, timezone_offset_minutes: i32) -> String {
    let offset_seconds = timezone_offset_minutes
        .saturating_mul(-60)
        .clamp(-86_399, 86_399);
    let offset =
        FixedOffset::east_opt(offset_seconds).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    DateTime::<Utc>::from_timestamp_millis(timestamp_ms)
        .map(|value| value.with_timezone(&offset).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown-date".into())
}
