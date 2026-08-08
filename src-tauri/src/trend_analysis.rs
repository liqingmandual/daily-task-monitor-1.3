use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use serde::{Deserialize, Deserializer, Serialize};

use crate::domain::ActivityScope;
use crate::trends::{
    TrendBaselineKind, TrendEvidenceScope, TrendMetric, TrendSelectionMode, TrendWorkbenchPayload,
    TrendWorkbenchRequest,
};

pub const DIRECTION_TOLERANCE_PERCENT: f64 = 8.0;
const MINIMUM_EFFECTIVE_ACTIVITY_DAYS: usize = 3;
pub const TREND_RESEARCH_MAX_EVIDENCE_IDS_PER_FINDING: usize = 8;
pub const TREND_RESEARCH_MAX_CLAIMS_PER_FINDING: usize = 8;
pub const TREND_RESEARCH_MAX_VALIDATED_FINDINGS: usize = 6;
pub const TREND_RESEARCH_MAX_PROVIDER_FINDINGS: usize = 8;
pub const TREND_RESEARCH_MAX_RESPONSE_BYTES: usize = 65_536;
pub const TREND_RESEARCH_MAX_FREE_TEXT_CHARS: usize = 500;
const TREND_RESEARCH_MAX_JSON_DEPTH: usize = 64;
pub const TREND_RESEARCH_CANONICAL_OBSERVATIONS: &[&str] = &[
    "当前聚合证据可供复核",
    "相较前序周期，当前指标上升",
    "相较前序周期，当前指标下降",
    "相较前序周期，当前指标保持稳定",
    "相较上月同期，当前指标上升",
    "相较上月同期，当前指标下降",
    "相较上月同期，当前指标保持稳定",
    "相较自定义基线，当前指标上升",
    "相较自定义基线，当前指标下降",
    "相较自定义基线，当前指标保持稳定",
];
pub const TREND_RESEARCH_OPERATIONAL_HYPOTHESES: &[&str] = &[
    "可能与任务安排有关",
    "可能与工作安排有关",
    "可能与日程安排有关",
    "可能与任务结构有关",
    "可能与日程结构有关",
    "可能与工作流程有关",
    "可能与活动切换有关",
    "可能与记录节奏有关",
    "可能与工作节奏有关",
    "可能与记录完整性有关",
    "可能与数据质量有关",
    "可能与分类覆盖有关",
    "可能与前后周期一致的工作安排有关",
];
pub const TREND_RESEARCH_OPERATIONAL_VALIDATION_METHODS: &[&str] = &[
    "后续周期复核同类聚合证据",
    "在后续周期复核同类聚合证据",
    "在上一周期与当前周期复核同类聚合证据",
    "复核任务安排与聚合指标的同步变化",
    "复核工作安排与聚合指标的同步变化",
    "复核日程结构与聚合指标的同步变化",
    "复核工作流程与聚合指标的同步变化",
    "复核活动切换与聚合指标的同步变化",
    "检查记录完整性后重新计算聚合指标",
    "检查数据质量后重新计算聚合指标",
    "检查分类覆盖后重新计算聚合指标",
];
pub const TREND_RESEARCH_RELATION_RULES: &[&str] = &[
    "Use relation values exactly: supports, increased, decreased, stable.",
    "Each claim evidenceId must appear exactly once in evidenceIds, and every evidenceId must have exactly one claim.",
    "A claim relation is allowed only when that exact evidence item lists it in allowedRelations.",
    "A finding may contain at most one directional claim (increased, decreased, or stable); supports claims may accompany it.",
    "The observation must be the exact canonical string for the directional claim's evidence baseline and relation.",
];
pub const TREND_RESEARCH_LOW_COVERAGE_EXPLANATION: &str = "分类覆盖不足，暂不提供可能解释";
const LOCAL_LIMITATION_LOW_COVERAGE: &str = "分类覆盖有限，可能解释已被本地策略抑制";
const LOCAL_LIMITATION_BASELINE: &str = "部分基线缺少可比较证据";
const LOCAL_LIMITATION_FILTERED: &str = "部分模型发现未通过本地证据校验";
const CONFIDENCE_VALIDATION_ERROR: &str = "finding confidence must be finite and in [0, 1]";
const PERSONAL_STATE_INFERENCE_ERROR: &str =
    "finding free text contains personal or psychological state or trait inference";
pub const TREND_RESEARCH_PERSONAL_STATE_INFERENCE_TERMS: &[&str] = &[
    "压力",
    "焦虑",
    "抑郁",
    "情绪",
    "心情",
    "心理",
    "精神",
    "注意力",
    "专注力",
    "执行力",
    "意志",
    "意志力",
    "自律",
    "懒惰",
    "疲劳",
    "疲惫",
    "倦怠",
    "能力",
    "聪明",
    "智力",
    "性格",
    "人格",
    "动机",
    "健康",
    "诊断",
    "认知",
    "态度",
    "表现",
];

pub fn trend_research_structural_rules() -> Vec<String> {
    vec![
        "The top-level object must contain exactly these fields: evidenceHash, findings, limitations."
            .into(),
        "evidenceHash must exactly equal the supplied input evidenceHash.".into(),
        "Each finding must contain exactly these fields: observation, possibleExplanation, validationMethod, evidenceIds, claims, confidence, limitations."
            .into(),
        "Each claim must contain exactly these fields: evidenceId, relation.".into(),
        "findings must be nonempty.".into(),
        format!(
            "findings must contain at most {} provider items before per-item parsing.",
            TREND_RESEARCH_MAX_PROVIDER_FINDINGS
        ),
        format!(
            "response JSON must not exceed {} bytes before top-level parsing.",
            TREND_RESEARCH_MAX_RESPONSE_BYTES
        ),
        "confidence must be a finite JSON number in the inclusive range [0, 1]; values such as 80 are invalid."
            .into(),
        format!(
            "evidenceIds must be nonempty, unique, and contain at most {} items.",
            TREND_RESEARCH_MAX_EVIDENCE_IDS_PER_FINDING
        ),
        format!(
            "claims must be nonempty and contain at most {} items with unique evidenceId values.",
            TREND_RESEARCH_MAX_CLAIMS_PER_FINDING
        ),
        "Every evidenceIds value and claim evidenceId must exactly match an ID supplied in input evidence."
            .into(),
        format!(
            "Free-text fields observation, possibleExplanation, and validationMethod must be nonempty, at most {} characters, and contain no numeric literals.",
            TREND_RESEARCH_MAX_FREE_TEXT_CHARS
        ),
        "Every provider free-text field (observation, possibleExplanation, and validationMethod) must avoid personal or psychological state or trait inference terms; below 0.5 classificationCoverage, possibleExplanation is deterministically replaced before this inference check."
            .into(),
        format!(
            "Personal or psychological state or trait inference term catalog: {}.",
            serde_json::to_string(TREND_RESEARCH_PERSONAL_STATE_INFERENCE_TERMS)
                .expect("personal-state inference terms serialize")
        ),
        "Top-level limitations and each finding limitations must be empty arrays; provider prose is discarded and deterministic local policy constructs limitations."
            .into(),
        format!(
            "Submit findings in priority order; only the first {} validated findings are retained.",
            TREND_RESEARCH_MAX_VALIDATED_FINDINGS
        ),
    ]
}

pub fn trend_research_protocol_prompt() -> String {
    let structural_rules = trend_research_structural_rules().join(" ");
    format!(
        "Analyze only the supplied aggregate trend evidence. Return one strict JSON object and no markdown or surrounding text. Structural parser rules: {} Exact accepted observation strings: {}. Exact accepted possibleExplanation strings when classificationCoverage is at least 0.5: {}. When classificationCoverage is below 0.5, possibleExplanation is locally replaced with exactly: {}. Exact accepted validationMethod strings: {}. Relation and evidence rules: {} Every provider limitations array must be empty; provider limitation prose is discarded and local policy constructs limitations.",
        structural_rules,
        serde_json::to_string(TREND_RESEARCH_CANONICAL_OBSERVATIONS)
            .expect("canonical observations serialize"),
        serde_json::to_string(TREND_RESEARCH_OPERATIONAL_HYPOTHESES)
            .expect("operational hypotheses serialize"),
        TREND_RESEARCH_LOW_COVERAGE_EXPLANATION,
        serde_json::to_string(TREND_RESEARCH_OPERATIONAL_VALIDATION_METHODS)
            .expect("operational validation methods serialize"),
        TREND_RESEARCH_RELATION_RULES.join(" "),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchStatus {
    Ready,
    LimitationsOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRelation {
    Supports,
    Increased,
    Decreased,
    Stable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrendEvidenceClaim {
    pub evidence_id: String,
    pub relation: EvidenceRelation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrendResearchFinding {
    pub observation: String,
    pub possible_explanation: String,
    pub validation_method: String,
    pub evidence_ids: Vec<String>,
    pub claims: Vec<TrendEvidenceClaim>,
    #[serde(deserialize_with = "deserialize_trend_research_confidence")]
    pub confidence: f64,
    pub limitations: Vec<String>,
}

fn deserialize_trend_research_confidence<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    f64::deserialize(deserializer)
        .map_err(|_| <D::Error as serde::de::Error>::custom(CONFIDENCE_VALIDATION_ERROR))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendResearchAnalysis {
    pub status: ResearchStatus,
    pub findings: Vec<TrendResearchFinding>,
    pub limitations: Vec<String>,
    pub source: String,
    pub model: String,
    pub evidence_hash: String,
    #[serde(default)]
    pub activity_scope: ActivityScope,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrendResearchEvidence {
    pub id: String,
    pub value: f64,
    pub allowed_relations: Vec<EvidenceRelation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrendResearchInput {
    pub evidence_hash: String,
    pub metric: TrendMetric,
    #[serde(default)]
    pub selection_mode: TrendSelectionMode,
    #[serde(default)]
    pub selected_dates: Vec<String>,
    #[serde(default)]
    pub selected_date_count: usize,
    #[serde(default)]
    pub envelope_day_count: usize,
    pub effective_activity_day_count: usize,
    pub classification_coverage: f64,
    pub direction_tolerance_percent: f64,
    pub enabled_baselines: Vec<TrendBaselineKind>,
    pub evidence: Vec<TrendResearchEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrendResearchJobPayload {
    pub request: TrendWorkbenchRequest,
    pub input: TrendResearchInput,
    #[serde(default)]
    pub activity_scope: ActivityScope,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TrendResearchCandidate {
    evidence_hash: String,
    #[serde(rename = "findings")]
    _findings: Vec<serde::de::IgnoredAny>,
    #[serde(rename = "limitations")]
    _limitations: Vec<String>,
}

struct IsolatedTrendFindings {
    envelope: String,
    item_ranges: Vec<Range<usize>>,
}

fn skip_json_whitespace(bytes: &[u8], mut cursor: usize) -> usize {
    while matches!(bytes.get(cursor), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        cursor += 1;
    }
    cursor
}

fn scan_json_string(bytes: &[u8], start: usize) -> Result<usize, String> {
    if bytes.get(start) != Some(&b'"') {
        return Err("JSON object keys and string values must be quoted".into());
    }
    let mut cursor = start + 1;
    while let Some(&byte) = bytes.get(cursor) {
        match byte {
            b'"' => return Ok(cursor + 1),
            b'\\' => {
                cursor += 1;
                match bytes.get(cursor) {
                    Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                        cursor += 1;
                    }
                    Some(b'u') => {
                        let hex_end = cursor + 5;
                        if hex_end > bytes.len()
                            || !bytes[cursor + 1..hex_end].iter().all(u8::is_ascii_hexdigit)
                        {
                            return Err("JSON string contains an invalid Unicode escape".into());
                        }
                        cursor = hex_end;
                    }
                    _ => return Err("JSON string contains an invalid escape".into()),
                }
            }
            0x00..=0x1f => return Err("JSON string contains an unescaped control byte".into()),
            _ => cursor += 1,
        }
    }
    Err("JSON string is unterminated".into())
}

fn scan_json_number(bytes: &[u8], start: usize) -> Result<usize, String> {
    let mut cursor = start;
    if bytes.get(cursor) == Some(&b'-') {
        cursor += 1;
    }
    match bytes.get(cursor) {
        Some(b'0') => {
            cursor += 1;
            if matches!(bytes.get(cursor), Some(b'0'..=b'9')) {
                return Err("JSON number contains a leading zero".into());
            }
        }
        Some(b'1'..=b'9') => {
            cursor += 1;
            while matches!(bytes.get(cursor), Some(b'0'..=b'9')) {
                cursor += 1;
            }
        }
        _ => return Err("JSON number is malformed".into()),
    }
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let fraction_start = cursor;
        while matches!(bytes.get(cursor), Some(b'0'..=b'9')) {
            cursor += 1;
        }
        if cursor == fraction_start {
            return Err("JSON number has an empty fraction".into());
        }
    }
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let exponent_start = cursor;
        while matches!(bytes.get(cursor), Some(b'0'..=b'9')) {
            cursor += 1;
        }
        if cursor == exponent_start {
            return Err("JSON number has an empty exponent".into());
        }
    }
    Ok(cursor)
}

fn scan_json_literal(bytes: &[u8], start: usize, literal: &[u8]) -> Result<usize, String> {
    let end = start + literal.len();
    if bytes.get(start..end) == Some(literal) {
        Ok(end)
    } else {
        Err("JSON literal is malformed".into())
    }
}

fn scan_json_value(bytes: &[u8], start: usize, depth: usize) -> Result<usize, String> {
    if depth > TREND_RESEARCH_MAX_JSON_DEPTH {
        return Err("Trend research response exceeds the JSON nesting limit".into());
    }
    let cursor = skip_json_whitespace(bytes, start);
    match bytes.get(cursor) {
        Some(b'"') => scan_json_string(bytes, cursor),
        Some(b'{') => scan_json_object(bytes, cursor, depth + 1),
        Some(b'[') => scan_json_array(bytes, cursor, depth + 1),
        Some(b't') => scan_json_literal(bytes, cursor, b"true"),
        Some(b'f') => scan_json_literal(bytes, cursor, b"false"),
        Some(b'n') => scan_json_literal(bytes, cursor, b"null"),
        Some(b'-' | b'0'..=b'9') => scan_json_number(bytes, cursor),
        _ => Err("JSON value is malformed".into()),
    }
}

fn scan_json_array(bytes: &[u8], start: usize, depth: usize) -> Result<usize, String> {
    let mut cursor = skip_json_whitespace(bytes, start + 1);
    if bytes.get(cursor) == Some(&b']') {
        return Ok(cursor + 1);
    }
    loop {
        cursor = skip_json_whitespace(bytes, scan_json_value(bytes, cursor, depth)?);
        match bytes.get(cursor) {
            Some(b',') => cursor = skip_json_whitespace(bytes, cursor + 1),
            Some(b']') => return Ok(cursor + 1),
            _ => return Err("JSON array must separate items with commas".into()),
        }
    }
}

fn scan_json_object(bytes: &[u8], start: usize, depth: usize) -> Result<usize, String> {
    let mut cursor = skip_json_whitespace(bytes, start + 1);
    if bytes.get(cursor) == Some(&b'}') {
        return Ok(cursor + 1);
    }
    loop {
        cursor = scan_json_string(bytes, cursor)?;
        cursor = skip_json_whitespace(bytes, cursor);
        if bytes.get(cursor) != Some(&b':') {
            return Err("JSON object key must be followed by a colon".into());
        }
        cursor = skip_json_whitespace(bytes, scan_json_value(bytes, cursor + 1, depth)?);
        match bytes.get(cursor) {
            Some(b',') => cursor = skip_json_whitespace(bytes, cursor + 1),
            Some(b'}') => return Ok(cursor + 1),
            _ => return Err("JSON object must separate fields with commas".into()),
        }
    }
}

fn scan_findings_array(bytes: &[u8], start: usize) -> Result<(usize, Vec<Range<usize>>), String> {
    if bytes.get(start) != Some(&b'[') {
        return Err("Trend research response findings must be an array".into());
    }
    let mut cursor = skip_json_whitespace(bytes, start + 1);
    let mut item_ranges = Vec::new();
    if bytes.get(cursor) == Some(&b']') {
        return Ok((cursor + 1, item_ranges));
    }
    loop {
        let item_start = cursor;
        let item_end = scan_json_value(bytes, item_start, 1)?;
        item_ranges.push(item_start..item_end);
        if item_ranges.len() > TREND_RESEARCH_MAX_PROVIDER_FINDINGS {
            return Err(format!(
                "Trend research response findings must contain at most {} provider items",
                TREND_RESEARCH_MAX_PROVIDER_FINDINGS
            ));
        }
        cursor = skip_json_whitespace(bytes, item_end);
        match bytes.get(cursor) {
            Some(b',') => cursor = skip_json_whitespace(bytes, cursor + 1),
            Some(b']') => return Ok((cursor + 1, item_ranges)),
            _ => return Err("JSON findings array must separate items with commas".into()),
        }
    }
}

fn isolate_trend_findings(input: &str) -> Result<IsolatedTrendFindings, String> {
    let bytes = input.as_bytes();
    let mut cursor = skip_json_whitespace(bytes, 0);
    if bytes.get(cursor) != Some(&b'{') {
        return Err("Trend research response must be one JSON object".into());
    }
    cursor = skip_json_whitespace(bytes, cursor + 1);
    let mut findings_range = None;
    let mut item_ranges = Vec::new();

    if bytes.get(cursor) != Some(&b'}') {
        loop {
            let key_start = cursor;
            let key_end = scan_json_string(bytes, key_start)?;
            let key: String = serde_json::from_str(&input[key_start..key_end])
                .map_err(|error| format!("Invalid top-level JSON key: {error}"))?;
            cursor = skip_json_whitespace(bytes, key_end);
            if bytes.get(cursor) != Some(&b':') {
                return Err("JSON object key must be followed by a colon".into());
            }
            let value_start = skip_json_whitespace(bytes, cursor + 1);
            if key == "findings" {
                if findings_range.is_some() {
                    return Err("Trend research response contains duplicate findings fields".into());
                }
                let (value_end, ranges) = scan_findings_array(bytes, value_start)?;
                findings_range = Some(value_start..value_end);
                item_ranges = ranges;
                cursor = value_end;
            } else {
                cursor = scan_json_value(bytes, value_start, 1)?;
            }
            cursor = skip_json_whitespace(bytes, cursor);
            match bytes.get(cursor) {
                Some(b',') => cursor = skip_json_whitespace(bytes, cursor + 1),
                Some(b'}') => break,
                _ => return Err("JSON object must separate fields with commas".into()),
            }
        }
    }

    cursor = skip_json_whitespace(bytes, cursor + 1);
    if cursor != bytes.len() {
        return Err("Trend research response must not contain trailing content".into());
    }
    let findings_range = findings_range
        .ok_or_else(|| "Trend research response must contain a findings field".to_string())?;
    let mut envelope = String::with_capacity(input.len() - findings_range.len() + 2);
    envelope.push_str(&input[..findings_range.start]);
    envelope.push_str("[]");
    envelope.push_str(&input[findings_range.end..]);
    Ok(IsolatedTrendFindings {
        envelope,
        item_ranges,
    })
}

pub fn build_trend_research_input(payload: &TrendWorkbenchPayload) -> TrendResearchInput {
    let enabled_baselines = payload
        .baselines
        .iter()
        .map(|baseline| baseline.kind)
        .collect::<Vec<_>>();

    let mut evidence = payload
        .evidence
        .iter()
        .filter_map(|item| {
            if !item.value.is_finite() || item.scope == TrendEvidenceScope::Bucket {
                return None;
            }
            let mut allowed_relations = vec![EvidenceRelation::Supports];
            if item.scope == TrendEvidenceScope::Baseline
                && item.series_kind != TrendBaselineKind::Current
            {
                let baseline = payload
                    .baselines
                    .iter()
                    .find(|baseline| baseline.kind == item.series_kind)?;
                if let Some(metric) = item.metric {
                    if metric != payload.metric || !baseline.is_valid {
                        return None;
                    }
                    if let Some(relation) = baseline.percent_delta.and_then(relation_for_delta) {
                        allowed_relations.push(relation);
                    }
                }
            }
            Some(TrendResearchEvidence {
                id: item.id.clone(),
                value: item.value,
                allowed_relations,
            })
        })
        .collect::<Vec<_>>();
    evidence.sort_by(|left, right| left.id.cmp(&right.id));
    evidence.dedup_by(|left, right| left.id == right.id);

    TrendResearchInput {
        evidence_hash: payload.evidence_hash.clone(),
        metric: payload.metric,
        selection_mode: payload.range.selection_mode,
        selected_dates: payload.range.selected_dates.clone(),
        selected_date_count: payload.range.selected_date_count,
        envelope_day_count: payload.range.envelope_day_count,
        effective_activity_day_count: payload.summary.effective_activity_day_count,
        classification_coverage: payload.summary.classification_coverage.clamp(0.0, 1.0),
        direction_tolerance_percent: DIRECTION_TOLERANCE_PERCENT,
        enabled_baselines,
        evidence,
    }
}

pub fn build_trend_research_job_payload(
    request: TrendWorkbenchRequest,
    payload: &TrendWorkbenchPayload,
) -> TrendResearchJobPayload {
    build_trend_research_job_payload_scoped(request, payload, ActivityScope::All)
}

pub fn build_trend_research_job_payload_scoped(
    request: TrendWorkbenchRequest,
    payload: &TrendWorkbenchPayload,
    activity_scope: ActivityScope,
) -> TrendResearchJobPayload {
    TrendResearchJobPayload {
        request,
        input: build_trend_research_input(payload),
        activity_scope,
    }
}

pub fn limitations_only_analysis(input: &TrendResearchInput) -> TrendResearchAnalysis {
    TrendResearchAnalysis {
        status: ResearchStatus::LimitationsOnly,
        findings: Vec::new(),
        limitations: vec!["有效活动日不足，暂不生成研究结论".into()],
        source: "local".into(),
        model: "policy-v1".into(),
        evidence_hash: input.evidence_hash.clone(),
        activity_scope: ActivityScope::All,
    }
}

pub fn unavailable_analysis(input: &TrendResearchInput) -> TrendResearchAnalysis {
    TrendResearchAnalysis {
        status: ResearchStatus::LimitationsOnly,
        findings: Vec::new(),
        limitations: vec!["尚无通过证据校验的研究分析".into()],
        source: "local".into(),
        model: "policy-v1".into(),
        evidence_hash: input.evidence_hash.clone(),
        activity_scope: ActivityScope::All,
    }
}

pub fn parse_trend_research_response(
    content: &str,
    input: &TrendResearchInput,
    source: &str,
    model: &str,
) -> Result<TrendResearchAnalysis, String> {
    if input.effective_activity_day_count < MINIMUM_EFFECTIVE_ACTIVITY_DAYS {
        return Ok(limitations_only_analysis(input));
    }
    let trimmed = content.trim();
    if trimmed.len() > TREND_RESEARCH_MAX_RESPONSE_BYTES {
        return Err(format!(
            "Trend research response exceeds the {}-byte resource limit",
            TREND_RESEARCH_MAX_RESPONSE_BYTES
        ));
    }
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err("Trend research response must be one JSON object".into());
    }
    let isolated = isolate_trend_findings(trimmed)?;
    let candidate: TrendResearchCandidate =
        serde_json::from_str(&isolated.envelope).map_err(|error| {
            format!("Trend research response failed strict top-level validation: {error}")
        })?;
    if candidate.evidence_hash != input.evidence_hash {
        return Err("Trend research response evidence hash is stale".into());
    }

    let evidence = input
        .evidence
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let candidate_finding_count = isolated.item_ranges.len();
    let mut first_finding_error: Option<String> = None;
    let mut findings = isolated
        .item_ranges
        .into_iter()
        .filter_map(|range| {
            let result = serde_json::from_str::<TrendResearchFinding>(&trimmed[range])
                .map_err(|error| format!("finding failed strict structural validation: {error}"))
                .and_then(|finding| {
                    validate_finding(finding, input, &evidence).map_err(str::to_string)
                });
            match result {
                Ok(finding) => Some(finding),
                Err(error) => {
                    first_finding_error.get_or_insert(error);
                    None
                }
            }
        })
        .collect::<Vec<_>>();
    if findings.is_empty() {
        return Err(format!(
            "Trend research response contains no valid findings: {}",
            first_finding_error.unwrap_or_else(|| "finding failed validation".into())
        ));
    }
    let filtered_finding_count = candidate_finding_count.saturating_sub(findings.len());
    findings.truncate(TREND_RESEARCH_MAX_VALIDATED_FINDINGS);
    for finding in &mut findings {
        finding.limitations = deterministic_finding_limitations(input);
    }
    let limitations = deterministic_analysis_limitations(input, filtered_finding_count);

    Ok(TrendResearchAnalysis {
        status: ResearchStatus::Ready,
        findings,
        limitations,
        source: source.to_string(),
        model: model.to_string(),
        evidence_hash: input.evidence_hash.clone(),
        activity_scope: ActivityScope::All,
    })
}

fn relation_for_delta(delta: f64) -> Option<EvidenceRelation> {
    if !delta.is_finite() {
        return None;
    }
    Some(if delta >= DIRECTION_TOLERANCE_PERCENT {
        EvidenceRelation::Increased
    } else if delta <= -DIRECTION_TOLERANCE_PERCENT {
        EvidenceRelation::Decreased
    } else {
        EvidenceRelation::Stable
    })
}

fn validate_finding(
    mut finding: TrendResearchFinding,
    input: &TrendResearchInput,
    evidence: &HashMap<&str, &TrendResearchEvidence>,
) -> Result<TrendResearchFinding, &'static str> {
    if finding_text_contains_numeric_literal(&finding) {
        return Err("finding free text contains a numeric literal");
    }
    if input.classification_coverage < 0.5 {
        finding.possible_explanation = TREND_RESEARCH_LOW_COVERAGE_EXPLANATION.into();
    }
    if finding_text_contains_personal_state_inference(&finding) {
        return Err(PERSONAL_STATE_INFERENCE_ERROR);
    }
    if !finding.confidence.is_finite() || !(0.0..=1.0).contains(&finding.confidence) {
        return Err(CONFIDENCE_VALIDATION_ERROR);
    }
    if finding.evidence_ids.is_empty()
        || finding.evidence_ids.len() > TREND_RESEARCH_MAX_EVIDENCE_IDS_PER_FINDING
    {
        return Err("finding evidenceIds cardinality must be between 1 and 8");
    }
    if finding.claims.is_empty() || finding.claims.len() > TREND_RESEARCH_MAX_CLAIMS_PER_FINDING {
        return Err("finding claims cardinality must be between 1 and 8");
    }
    if !validate_free_text(&finding.observation, TREND_RESEARCH_MAX_FREE_TEXT_CHARS)
        || !validate_free_text(
            &finding.possible_explanation,
            TREND_RESEARCH_MAX_FREE_TEXT_CHARS,
        )
        || !validate_free_text(
            &finding.validation_method,
            TREND_RESEARCH_MAX_FREE_TEXT_CHARS,
        )
    {
        return Err("finding fields failed bounded free-text validation");
    }

    let evidence_ids = finding
        .evidence_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if evidence_ids.len() != finding.evidence_ids.len() {
        return Err("finding evidence IDs must be unique");
    }
    let claim_ids = finding
        .claims
        .iter()
        .map(|claim| claim.evidence_id.as_str())
        .collect::<HashSet<_>>();
    if claim_ids != evidence_ids || claim_ids.len() != finding.claims.len() {
        return Err("finding claims must map one-to-one to evidence IDs");
    }
    if finding.claims.iter().any(|claim| {
        evidence
            .get(claim.evidence_id.as_str())
            .is_none_or(|item| !item.allowed_relations.contains(&claim.relation))
    }) {
        return Err("finding claim is not authorized by its evidence");
    }

    if !is_canonical_evidence_bound_observation(&finding.observation, &finding.claims) {
        return Err("observation must use canonical evidence-bound direction phrasing");
    }
    if input.classification_coverage >= 0.5
        && !is_operational_hypothesis(&finding.possible_explanation)
    {
        return Err("possibleExplanation must use an operational hypothesis");
    }
    if !is_operational_validation_method(&finding.validation_method) {
        return Err("validationMethod must use an operational validation method");
    }

    Ok(finding)
}

fn finding_text_contains_numeric_literal(finding: &TrendResearchFinding) -> bool {
    [
        finding.observation.as_str(),
        finding.possible_explanation.as_str(),
        finding.validation_method.as_str(),
    ]
    .into_iter()
    .any(contains_numeric_literal)
}

fn finding_text_contains_personal_state_inference(finding: &TrendResearchFinding) -> bool {
    [
        finding.observation.as_str(),
        finding.possible_explanation.as_str(),
        finding.validation_method.as_str(),
    ]
    .into_iter()
    .any(|text| {
        TREND_RESEARCH_PERSONAL_STATE_INFERENCE_TERMS
            .iter()
            .any(|term| text.contains(term))
    })
}

fn is_canonical_evidence_bound_observation(
    observation: &str,
    claims: &[TrendEvidenceClaim],
) -> bool {
    let mut directional_claims = claims
        .iter()
        .filter(|claim| claim.relation != EvidenceRelation::Supports);
    let Some(claim) = directional_claims.next() else {
        return observation == TREND_RESEARCH_CANONICAL_OBSERVATIONS[0];
    };
    if directional_claims.next().is_some() {
        return false;
    }
    canonical_direction_observation(&claim.evidence_id, claim.relation)
        .is_some_and(|canonical| observation == canonical)
}

fn is_operational_hypothesis(text: &str) -> bool {
    TREND_RESEARCH_OPERATIONAL_HYPOTHESES.contains(&text)
}

fn is_operational_validation_method(text: &str) -> bool {
    TREND_RESEARCH_OPERATIONAL_VALIDATION_METHODS.contains(&text)
}

fn canonical_direction_observation(
    evidence_id: &str,
    relation: EvidenceRelation,
) -> Option<&'static str> {
    let offset = if evidence_id.starts_with("previousEqualLength.") {
        1
    } else if evidence_id.starts_with("previousMonthSamePeriod.") {
        4
    } else if evidence_id.starts_with("custom.") {
        7
    } else {
        return None;
    };
    let relation_offset = match relation {
        EvidenceRelation::Increased => 0,
        EvidenceRelation::Decreased => 1,
        EvidenceRelation::Stable => 2,
        EvidenceRelation::Supports => return None,
    };
    TREND_RESEARCH_CANONICAL_OBSERVATIONS
        .get(offset + relation_offset)
        .copied()
}

fn deterministic_finding_limitations(input: &TrendResearchInput) -> Vec<String> {
    if input.classification_coverage < 0.5 {
        vec![LOCAL_LIMITATION_LOW_COVERAGE.into()]
    } else {
        Vec::new()
    }
}

fn deterministic_analysis_limitations(
    input: &TrendResearchInput,
    filtered_finding_count: usize,
) -> Vec<String> {
    let mut limitations = deterministic_finding_limitations(input);
    if input
        .enabled_baselines
        .iter()
        .copied()
        .filter(|kind| *kind != TrendBaselineKind::Current)
        .any(|kind| !baseline_has_comparative_evidence(input, kind))
    {
        limitations.push(LOCAL_LIMITATION_BASELINE.into());
    }
    if filtered_finding_count > 0 {
        limitations.push(LOCAL_LIMITATION_FILTERED.into());
    }
    limitations
}

fn baseline_has_comparative_evidence(input: &TrendResearchInput, kind: TrendBaselineKind) -> bool {
    let prefix = match kind {
        TrendBaselineKind::Current => return true,
        TrendBaselineKind::PreviousEqualLength => "previousEqualLength.",
        TrendBaselineKind::PreviousMonthSamePeriod => "previousMonthSamePeriod.",
        TrendBaselineKind::Custom => "custom.",
    };
    input.evidence.iter().any(|item| {
        item.id.starts_with(prefix)
            && item.allowed_relations.iter().any(|relation| {
                matches!(
                    relation,
                    EvidenceRelation::Increased
                        | EvidenceRelation::Decreased
                        | EvidenceRelation::Stable
                )
            })
    })
}

fn validate_free_text(text: &str, max_chars: usize) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= max_chars
        && !contains_numeric_literal(trimmed)
}

fn contains_numeric_literal(text: &str) -> bool {
    if text.chars().any(char::is_numeric) {
        return true;
    }
    let characters = text.chars().collect::<Vec<_>>();
    if characters
        .iter()
        .enumerate()
        .any(|(index, character)| *character == '一' && yi_has_numeric_context(&characters, index))
    {
        return true;
    }
    const EXPLICIT_NUMBER_PATTERNS: &[&str] = &[
        "百分之",
        "千分之",
        "万分之",
        "个百分点",
        "一倍",
        "一天",
        "一次",
        "一项",
        "一个",
        "一成",
        "一秒",
        "一分钟",
        "一小时",
        "一日",
        "一年",
        "一半",
        "半数",
        "多数",
        "少数",
        "成倍",
        "倍增",
        "翻倍",
        "百分比",
        "千分比",
        "万分比",
    ];
    if EXPLICIT_NUMBER_PATTERNS
        .iter()
        .any(|pattern| text.contains(pattern))
    {
        return true;
    }
    text.chars().any(|character| {
        matches!(
            character,
            '零' | '〇'
                | '二'
                | '三'
                | '四'
                | '五'
                | '六'
                | '七'
                | '八'
                | '九'
                | '十'
                | '百'
                | '千'
                | '万'
                | '亿'
                | '两'
                | '壹'
                | '贰'
                | '叁'
                | '肆'
                | '伍'
                | '陆'
                | '柒'
                | '捌'
                | '玖'
                | '拾'
                | '佰'
                | '仟'
                | '萬'
                | '億'
                | '半'
        )
    })
}

fn yi_has_numeric_context(characters: &[char], index: usize) -> bool {
    if index > 0
        && characters[index - 1] == '上'
        && characters.get(index + 1) == Some(&'周')
        && characters.get(index + 2) == Some(&'期')
    {
        return false;
    }
    characters.get(index + 1).is_some_and(|next| {
        matches!(
            *next,
            '半' | '倍'
                | '个'
                | '项'
                | '次'
                | '份'
                | '段'
                | '组'
                | '种'
                | '类'
                | '批'
                | '成'
                | '天'
                | '日'
                | '周'
                | '月'
                | '年'
                | '秒'
                | '分'
                | '时'
        )
    })
}
