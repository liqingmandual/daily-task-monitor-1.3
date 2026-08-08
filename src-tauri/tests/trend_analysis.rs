use chrono::NaiveDate;
use daily_task_monitor_core::ai::{
    AiExecutionErrorKind, AiExecutionMode, AiExecutionSnapshot, AiJobStatus,
};
use daily_task_monitor_core::app::{AppService, SettingsPatch};
use daily_task_monitor_core::db::{
    ActivitySegmentRecord, Database, TrendAnalysisRecord, TrendResearchAnalysisRecord,
};
use daily_task_monitor_core::domain::{
    ActivityCategory, ActivityScope, ClassificationSource, VideoPurpose,
};
use daily_task_monitor_core::trend_analysis::{
    EvidenceRelation, ResearchStatus, TrendResearchAnalysis, TrendResearchEvidence,
    build_trend_research_input, build_trend_research_job_payload, limitations_only_analysis,
    parse_trend_research_response, trend_research_protocol_prompt, trend_research_structural_rules,
};
use daily_task_monitor_core::trends::{
    TrendBaselineKind, TrendGranularity, TrendMetric, TrendWorkbenchPayload, TrendWorkbenchRequest,
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};

fn metric_values(active_seconds: f64) -> Value {
    json!({
        "monitoredSeconds": active_seconds as i64,
        "activeSeconds": active_seconds as i64,
        "learningSeconds": (active_seconds / 2.0) as i64,
        "idleSeconds": 0,
        "switchCount": 6,
        "longestFocusSeconds": (active_seconds / 3.0) as i64,
        "classificationCoverage": 0.8,
        "completedTaskCount": 2,
        "linkedTaskSeconds": (active_seconds / 4.0) as i64
    })
}

fn workbench(recorded_days: usize, coverage: f64) -> TrendWorkbenchPayload {
    let mut payload: TrendWorkbenchPayload = serde_json::from_value(json!({
        "range": {
            "startMs": 1,
            "endMs": 2,
            "startDate": "2026-07-06",
            "endDate": "2026-07-12",
            "dayCount": 7
        },
        "granularity": "day",
        "metric": "activeSeconds",
        "metricAvailability": [],
        "buckets": [{
            "id": "2026-07-06_2026-07-12",
            "startDate": "2026-07-06",
            "endDate": "2026-07-12",
            "values": metric_values(3600.0),
            "recordedDayCount": recorded_days,
            "missingDayCount": 7 - recorded_days,
            "evidenceIds": ["bucket.private"],
            "drilldown": {
                "bucketId": "2026-07-06_2026-07-12",
                "rawRows": [{
                    "rowId": "raw-private",
                    "bucketId": "2026-07-06_2026-07-12",
                    "evidenceKind": "activity",
                    "evidenceId": "activity-private",
                    "date": "2026-07-06",
                    "startTime": "09:00:00",
                    "endTime": "10:00:00",
                    "app": "Private App",
                    "titleSummary": "Secret raw activity title",
                    "category": "research",
                    "taskId": null,
                    "taskTitle": null,
                    "projectId": null,
                    "projectName": null,
                    "clippedDurationSeconds": 3600,
                    "confidence": 0.8,
                    "reviewState": "confirmed",
                    "shared": false
                }],
                "applicationDistribution": [],
                "categoryDistribution": [],
                "completedTasks": [],
                "linkedTaskRollups": [],
                "linkedProjectRollups": [],
                "workflowOwnership": [],
                "dataQuality": {
                    "recordedDayCount": recorded_days,
                    "missingDayCount": 7 - recorded_days,
                    "classifiedSeconds": 2880,
                    "classificationCoverage": coverage,
                    "lowConfidenceSeconds": 0,
                    "pendingSeconds": 720
                }
            }
        }],
        "summary": {
            "totals": metric_values(3600.0),
            "dailyAverage": {
                "monitoredSeconds": 3600.0 / recorded_days.max(1) as f64,
                "activeSeconds": 3600.0 / recorded_days.max(1) as f64,
                "idleSeconds": 0.0,
                "learningSeconds": 1800.0 / recorded_days.max(1) as f64
            },
            "averageSampleDayCount": recorded_days,
            "switchesPerActiveHour": 6.0,
            "meanPerBucket": metric_values(1200.0),
            "dailyMedian": metric_values(1100.0),
            "dailyMax": metric_values(1800.0),
            "dailySampleStddev": metric_values(200.0),
            "dailyCoefficientOfVariation": metric_values(0.2),
            "recordedDayCount": recorded_days,
            "effectiveActivityDayCount": recorded_days,
            "missingDayCount": 7 - recorded_days,
            "classifiedSeconds": 2880,
            "classificationCoverage": coverage,
            "lowConfidenceSeconds": 0,
            "pendingSeconds": 720,
            "evidenceIds": [
                "current.summary.activeSeconds",
                "current.summary.dailyMedian.activeSeconds",
                "current.summary.dailySampleStddev.activeSeconds",
                "current.summary.dailyCoefficientOfVariation.activeSeconds",
                "current.summary.dataQuality.recordedDayCount",
                "current.summary.dataQuality.classificationCoverage"
            ]
        },
        "baselines": [
            {
                "kind": "current",
                "range": {"startDate": "2026-07-06", "endDate": "2026-07-12"},
                "isValid": true,
                "value": 3600,
                "absoluteDelta": null,
                "percentDelta": null,
                "recordedDayCount": recorded_days,
                "missingDayCount": 7 - recorded_days,
                "evidenceIds": ["current.summary.activeSeconds"]
            },
            {
                "kind": "previousEqualLength",
                "range": {"startDate": "2026-06-29", "endDate": "2026-07-05"},
                "isValid": true,
                "value": 3000,
                "absoluteDelta": 600,
                "percentDelta": 20,
                "recordedDayCount": recorded_days,
                "missingDayCount": 7 - recorded_days,
                "evidenceIds": ["previousEqualLength.summary.activeSeconds"]
            },
            {
                "kind": "previousMonthSamePeriod",
                "range": {"startDate": "2026-06-06", "endDate": "2026-06-12"},
                "isValid": false,
                "value": null,
                "absoluteDelta": null,
                "percentDelta": null,
                "recordedDayCount": 0,
                "missingDayCount": 7,
                "evidenceIds": ["previousMonthSamePeriod.quality.valid"]
            }
        ],
        "evidence": [
            {"id": "current.summary.activeSeconds", "scope": "summary", "seriesKind": "current", "bucketId": null, "metric": "activeSeconds", "value": 3600},
            {"id": "current.summary.dailyMedian.activeSeconds", "scope": "summary", "seriesKind": "current", "bucketId": null, "metric": "activeSeconds", "value": 1100},
            {"id": "current.summary.dailySampleStddev.activeSeconds", "scope": "summary", "seriesKind": "current", "bucketId": null, "metric": "activeSeconds", "value": 200},
            {"id": "current.summary.dailyCoefficientOfVariation.activeSeconds", "scope": "summary", "seriesKind": "current", "bucketId": null, "metric": "activeSeconds", "value": 0.2},
            {"id": "current.summary.dataQuality.recordedDayCount", "scope": "summary", "seriesKind": "current", "bucketId": null, "metric": null, "value": recorded_days},
            {"id": "current.summary.dataQuality.classificationCoverage", "scope": "summary", "seriesKind": "current", "bucketId": null, "metric": null, "value": coverage},
            {"id": "previousEqualLength.summary.activeSeconds", "scope": "baseline", "seriesKind": "previousEqualLength", "bucketId": null, "metric": "activeSeconds", "value": 3000},
            {"id": "previousEqualLength.quality.recordedDayCount", "scope": "baseline", "seriesKind": "previousEqualLength", "bucketId": null, "metric": null, "value": recorded_days},
            {"id": "previousEqualLength.quality.missingDayCount", "scope": "baseline", "seriesKind": "previousEqualLength", "bucketId": null, "metric": null, "value": 7 - recorded_days},
            {"id": "previousMonthSamePeriod.quality.valid", "scope": "baseline", "seriesKind": "previousMonthSamePeriod", "bucketId": null, "metric": null, "value": 0},
            {"id": "previousMonthSamePeriod.summary.activeSeconds", "scope": "baseline", "seriesKind": "previousMonthSamePeriod", "bucketId": null, "metric": "activeSeconds", "value": 2800}
        ],
        "evidenceHash": "hash-current"
    }))
    .unwrap();
    payload.summary.classification_coverage = coverage;
    payload.summary.totals.classification_coverage = coverage;
    payload
}

fn request() -> TrendWorkbenchRequest {
    TrendWorkbenchRequest {
        start_date: "2026-07-06".into(),
        end_date: "2026-07-12".into(),
        selected_dates: None,
        timezone_offset_minutes: -480,
        granularity: Some(TrendGranularity::Day),
        metric: TrendMetric::ActiveSeconds,
        custom_baseline: None,
    }
}

fn utc_ms(date: &str) -> i64 {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis()
}

fn active_segment(id: &str, date: &str) -> ActivitySegmentRecord {
    ActivitySegmentRecord {
        id: id.into(),
        started_at_ms: utc_ms(date),
        ended_at_ms: utc_ms(date) + 60_000,
        app: "Private app".into(),
        app_path: String::new(),
        title: "Private title".into(),
        category: ActivityCategory::CreationDevelopment,
        video_purpose: VideoPurpose::Unknown,
        confidence: 0.9,
        source: ClassificationSource::Rule,
        reason: "test".into(),
        model_version: "rules-v1".into(),
        needs_review: false,
        inactivity_reason: None,
    }
}

fn snapshot(
    executor_id: &str,
    model: &str,
    evidence_hash: &str,
    created_at_ms: i64,
) -> AiExecutionSnapshot {
    AiExecutionSnapshot {
        execution_mode: AiExecutionMode::ApiKey,
        executor_id: executor_id.into(),
        model: model.into(),
        evidence_hash: evidence_hash.into(),
        created_at_ms,
    }
}

fn limitations_record(hash: &str, source: &str, model: &str) -> TrendResearchAnalysisRecord {
    TrendResearchAnalysisRecord {
        range_start: "2026-07-06".into(),
        range_end: "2026-07-12".into(),
        analysis: TrendResearchAnalysis {
            activity_scope: ActivityScope::All,
            status: ResearchStatus::LimitationsOnly,
            findings: Vec::new(),
            limitations: vec!["仅保存通过校验的限制说明".into()],
            source: source.into(),
            model: model.into(),
            evidence_hash: hash.into(),
        },
    }
}

fn finding(evidence_id: &str, relation: &str) -> Value {
    json!({
        "observation": "相较前序周期，当前指标上升",
        "possibleExplanation": "可能与工作安排有关",
        "validationMethod": "后续周期复核同类聚合证据",
        "evidenceIds": [evidence_id],
        "claims": [{"evidenceId": evidence_id, "relation": relation}],
        "confidence": 0.82,
        "limitations": ["这只是待验证的相关性假设"]
    })
}

fn response(hash: &str, findings: Vec<Value>) -> String {
    json!({
        "evidenceHash": hash,
        "findings": findings,
        "limitations": ["分析只覆盖已提供的聚合证据"]
    })
    .to_string()
}

fn response_with_raw_findings(hash: &str, findings: &[String]) -> String {
    format!(
        "{{\"evidenceHash\":{},\"findings\":[{}],\"limitations\":[]}}",
        serde_json::to_string(hash).unwrap(),
        findings.join(",")
    )
}

#[test]
fn protocol_documents_every_enforced_response_constraint() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    let mut confidence_mismatch = finding("previousEqualLength.summary.activeSeconds", "increased");
    confidence_mismatch["confidence"] = json!(80);
    let error = parse_trend_research_response(
        &response("hash-current", vec![confidence_mismatch]),
        &input,
        "openai",
        "gpt-test",
    )
    .expect_err("confidence expressed as 80 must not be interpreted as 0.8");
    assert!(error.contains("finding confidence must be finite and in [0, 1]"));

    let mut invalid_confidence = finding("previousEqualLength.summary.activeSeconds", "increased");
    invalid_confidence["confidence"] = json!("high");
    let error = parse_trend_research_response(
        &response("hash-current", vec![invalid_confidence]),
        &input,
        "openai",
        "gpt-test",
    )
    .expect_err("nonnumeric confidence must be rejected specifically");
    assert!(error.contains("finding confidence must be finite and in [0, 1]"));

    let overflow_confidence = response(
        "hash-current",
        vec![finding(
            "previousEqualLength.summary.activeSeconds",
            "increased",
        )],
    )
    .replacen("\"confidence\":0.82", "\"confidence\":1e400", 1);
    let error = parse_trend_research_response(&overflow_confidence, &input, "openai", "gpt-test")
        .expect_err("nonfinite confidence must be rejected specifically");
    assert!(error.contains("finding confidence must be finite and in [0, 1]"));

    let missing_top_level_limitations = json!({
        "evidenceHash": "hash-current",
        "findings": [finding("previousEqualLength.summary.activeSeconds", "increased")]
    })
    .to_string();
    assert!(
        parse_trend_research_response(
            &missing_top_level_limitations,
            &input,
            "openai",
            "gpt-test",
        )
        .is_err(),
        "an exact top-level schema must require limitations"
    );

    let prompt = trend_research_protocol_prompt();
    let structural_rules = trend_research_structural_rules();
    for required in [
        "The top-level object must contain exactly these fields: evidenceHash, findings, limitations.",
        "evidenceHash must exactly equal the supplied input evidenceHash.",
        "Each finding must contain exactly these fields: observation, possibleExplanation, validationMethod, evidenceIds, claims, confidence, limitations.",
        "Each claim must contain exactly these fields: evidenceId, relation.",
        "findings must be nonempty.",
        "findings must contain at most 8 provider items before per-item parsing.",
        "response JSON must not exceed 65536 bytes before top-level parsing.",
        "confidence must be a finite JSON number in the inclusive range [0, 1]; values such as 80 are invalid.",
        "evidenceIds must be nonempty, unique, and contain at most 8 items.",
        "claims must be nonempty and contain at most 8 items with unique evidenceId values.",
        "Every evidenceIds value and claim evidenceId must exactly match an ID supplied in input evidence.",
        "Free-text fields observation, possibleExplanation, and validationMethod must be nonempty, at most 500 characters, and contain no numeric literals.",
        "Every provider free-text field (observation, possibleExplanation, and validationMethod) must avoid personal or psychological state or trait inference terms; below 0.5 classificationCoverage, possibleExplanation is deterministically replaced before this inference check.",
        "Top-level limitations and each finding limitations must be empty arrays; provider prose is discarded and deterministic local policy constructs limitations.",
        "Submit findings in priority order; only the first 6 validated findings are retained.",
    ] {
        assert!(
            structural_rules.iter().any(|rule| rule == required),
            "parser-owned rule set omitted enforced rule: {required}"
        );
        assert!(
            prompt.contains(required),
            "protocol omitted enforced parser rule: {required}"
        );
    }
}

#[test]
fn evidence_and_claim_cardinality_errors_are_specific() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    let cases = [
        (
            "evidenceIds",
            json!([]),
            "finding evidenceIds cardinality must be between 1 and 8",
        ),
        (
            "evidenceIds",
            json!(["e0", "e1", "e2", "e3", "e4", "e5", "e6", "e7", "e8"]),
            "finding evidenceIds cardinality must be between 1 and 8",
        ),
        (
            "claims",
            json!([]),
            "finding claims cardinality must be between 1 and 8",
        ),
        (
            "claims",
            json!([
                {"evidenceId": "e0", "relation": "supports"},
                {"evidenceId": "e1", "relation": "supports"},
                {"evidenceId": "e2", "relation": "supports"},
                {"evidenceId": "e3", "relation": "supports"},
                {"evidenceId": "e4", "relation": "supports"},
                {"evidenceId": "e5", "relation": "supports"},
                {"evidenceId": "e6", "relation": "supports"},
                {"evidenceId": "e7", "relation": "supports"},
                {"evidenceId": "e8", "relation": "supports"}
            ]),
            "finding claims cardinality must be between 1 and 8",
        ),
    ];

    for (field, value, expected) in cases {
        let mut item = finding("previousEqualLength.summary.activeSeconds", "increased");
        item[field] = value;
        let error = parse_trend_research_response(
            &response("hash-current", vec![item]),
            &input,
            "openai",
            "gpt-test",
        )
        .expect_err("invalid cardinality must be rejected");
        assert!(
            error.contains(expected),
            "wrong {field} cardinality diagnostic: {error}"
        );
    }
}

#[test]
fn personal_state_inference_filter_covers_every_provider_free_text_field() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    for field in ["observation", "possibleExplanation", "validationMethod"] {
        let mut item = finding("previousEqualLength.summary.activeSeconds", "increased");
        item[field] = json!("压力");
        let error = parse_trend_research_response(
            &response("hash-current", vec![item]),
            &input,
            "openai",
            "gpt-test",
        )
        .expect_err("personal-state inference must be rejected");
        assert!(
            error.contains("personal or psychological state or trait inference"),
            "wrong {field} personal-state diagnostic: {error}"
        );
    }
}

#[test]
fn accepts_known_evidence_and_matching_direction() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    let analysis = parse_trend_research_response(
        &response(
            "hash-current",
            vec![finding(
                "previousEqualLength.summary.activeSeconds",
                "increased",
            )],
        ),
        &input,
        "openai",
        "gpt-test",
    )
    .unwrap();

    assert_eq!(analysis.status, ResearchStatus::Ready);
    assert_eq!(analysis.findings.len(), 1);
    assert_eq!(
        analysis.findings[0].claims[0].relation,
        EvidenceRelation::Increased
    );
    assert_eq!(analysis.source, "openai");
    assert_eq!(analysis.model, "gpt-test");
}

#[test]
fn rejects_stale_hash_and_all_invalid_findings() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    assert!(
        parse_trend_research_response(
            &response(
                "stale-hash",
                vec![finding(
                    "previousEqualLength.summary.activeSeconds",
                    "increased"
                )],
            ),
            &input,
            "openai",
            "gpt-test",
        )
        .is_err()
    );

    for invalid in [
        finding("unknown.evidence", "supports"),
        finding("previousEqualLength.summary.activeSeconds", "decreased"),
        finding("previousMonthSamePeriod.summary.activeSeconds", "increased"),
    ] {
        assert!(
            parse_trend_research_response(
                &response("hash-current", vec![invalid]),
                &input,
                "openai",
                "gpt-test",
            )
            .is_err()
        );
    }
}

#[test]
fn drops_only_invalid_findings_when_at_least_one_is_valid() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    let analysis = parse_trend_research_response(
        &response(
            "hash-current",
            vec![
                finding("unknown.evidence", "supports"),
                finding("previousEqualLength.summary.activeSeconds", "increased"),
            ],
        ),
        &input,
        "codex",
        "codex-model",
    )
    .unwrap();

    assert_eq!(analysis.findings.len(), 1);
    assert_eq!(
        analysis.findings[0].evidence_ids,
        ["previousEqualLength.summary.activeSeconds"]
    );
    assert!(
        analysis
            .limitations
            .iter()
            .any(|item| item.contains("未通过本地证据校验"))
    );
}

#[test]
fn structurally_invalid_siblings_are_filtered_independently() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    let valid = finding("previousEqualLength.summary.activeSeconds", "increased");

    let mut confidence_string = valid.clone();
    confidence_string["confidence"] = json!("high");
    let mut unknown_field = valid.clone();
    unknown_field["unexpected"] = json!(true);
    let mut missing_field = valid.clone();
    missing_field
        .as_object_mut()
        .unwrap()
        .remove("validationMethod");
    let mut invalid_relation = valid.clone();
    invalid_relation["claims"][0]["relation"] = json!("caused");
    let mut wrong_field_type = valid.clone();
    wrong_field_type["evidenceIds"] = json!("previousEqualLength.summary.activeSeconds");

    let invalid_siblings = [
        ("confidence string", confidence_string),
        ("unknown field", unknown_field),
        ("missing field", missing_field),
        ("invalid relation enum", invalid_relation),
        ("wrong field type", wrong_field_type),
    ];
    for (case, invalid) in &invalid_siblings {
        let analysis = parse_trend_research_response(
            &response("hash-current", vec![invalid.clone(), valid.clone()]),
            &input,
            "openai",
            "gpt-test",
        )
        .unwrap_or_else(|error| panic!("{case} sibling discarded valid finding: {error}"));
        assert_eq!(analysis.findings.len(), 1, "wrong result for {case}");
        assert_eq!(
            analysis.findings[0].evidence_ids,
            ["previousEqualLength.summary.activeSeconds"]
        );
        assert!(
            analysis
                .limitations
                .iter()
                .any(|item| item.contains("未通过本地证据校验")),
            "filtered sibling limitation missing for {case}"
        );
    }

    let error = parse_trend_research_response(
        &response(
            "hash-current",
            invalid_siblings
                .into_iter()
                .map(|(_, finding)| finding)
                .collect(),
        ),
        &input,
        "openai",
        "gpt-test",
    )
    .expect_err("all structurally invalid findings must be InvalidResponse");
    assert!(
        error.contains("no valid findings"),
        "all-invalid response did not use finding validation semantics: {error}"
    );

    let valid_raw = valid.to_string();
    let overflow_raw = valid_raw.replacen("\"confidence\":0.82", "\"confidence\":1e400", 1);
    let analysis = parse_trend_research_response(
        &response_with_raw_findings("hash-current", &[overflow_raw.clone(), valid_raw]),
        &input,
        "openai",
        "gpt-test",
    )
    .expect("overflow sibling must not discard a valid finding");
    assert_eq!(analysis.findings.len(), 1);
    assert!(
        analysis
            .limitations
            .iter()
            .any(|item| item.contains("未通过本地证据校验"))
    );

    let error = parse_trend_research_response(
        &response_with_raw_findings("hash-current", &[overflow_raw]),
        &input,
        "openai",
        "gpt-test",
    )
    .expect_err("all-overflow findings must remain InvalidResponse");
    assert!(error.contains("no valid findings"));
    assert!(error.contains("finding confidence must be finite and in [0, 1]"));
}

#[test]
fn top_level_shape_and_provider_finding_resource_bound_remain_strict() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    let valid = finding("previousEqualLength.summary.activeSeconds", "increased");
    let extra_status = json!({
        "evidenceHash": "hash-current",
        "findings": [valid.clone()],
        "limitations": [],
        "status": "ready"
    })
    .to_string();
    assert!(
        parse_trend_research_response(&extra_status, &input, "openai", "gpt-test").is_err(),
        "unknown top-level status must remain forbidden"
    );

    let wrong_findings_type = json!({
        "evidenceHash": "hash-current",
        "findings": {},
        "limitations": []
    })
    .to_string();
    assert!(
        parse_trend_research_response(&wrong_findings_type, &input, "openai", "gpt-test").is_err(),
        "top-level findings must remain an array"
    );

    let error = parse_trend_research_response(
        &response("hash-current", vec![valid; 9]),
        &input,
        "openai",
        "gpt-test",
    )
    .expect_err("provider finding count must be bounded before per-item parsing");
    assert!(error.contains("at most 8 provider items"));

    let oversized_response = json!({
        "evidenceHash": "hash-current",
        "findings": [{"oversized": "x".repeat(65_536)}],
        "limitations": []
    })
    .to_string();
    let error = parse_trend_research_response(&oversized_response, &input, "openai", "gpt-test")
        .expect_err("response resource bound must run before top-level parsing");
    assert!(error.contains("65536-byte resource limit"));
}

#[test]
fn findings_scanner_handles_strings_escapes_nesting_and_rejects_bad_envelopes() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    let valid = finding("previousEqualLength.summary.activeSeconds", "increased");
    let valid_raw = valid.to_string();

    let findings_inside_string = format!(
        "{{\"evidenceHash\":\"hash-current\",\"limitations\":[\"not a key: \\\"findings\\\": [ ]\"],\"findings\":[{valid_raw}]}}"
    );
    assert!(
        parse_trend_research_response(&findings_inside_string, &input, "openai", "gpt-test")
            .is_ok(),
        "findings text inside a string must not be treated as a key"
    );

    let mut escaped = valid.clone();
    escaped["limitations"] = json!(["escaped quote: \" and brackets: [ ] { }"]);
    let escaped_response = response_with_raw_findings("hash-current", &[escaped.to_string()]);
    assert!(
        parse_trend_research_response(&escaped_response, &input, "openai", "gpt-test").is_ok(),
        "escaped quotes and brackets inside strings must not split findings"
    );

    let mut nested_invalid = valid.clone();
    nested_invalid["unexpected"] = json!({
        "arrays": [[1, 2], {"findings": ["]", "}"]}],
        "object": {"deep": {"value": true}}
    });
    let nested_response = response_with_raw_findings(
        "hash-current",
        &[nested_invalid.to_string(), valid.to_string()],
    );
    let analysis = parse_trend_research_response(&nested_response, &input, "openai", "gpt-test")
        .expect("nested invalid sibling must be isolated");
    assert_eq!(analysis.findings.len(), 1);

    let malformed_array = format!(
        "{{\"evidenceHash\":\"hash-current\",\"findings\":[{valid_raw},],\"limitations\":[]}}"
    );
    assert!(parse_trend_research_response(&malformed_array, &input, "openai", "gpt-test").is_err());

    let duplicate_findings = format!(
        "{{\"evidenceHash\":\"hash-current\",\"findings\":[],\"findings\":[{valid_raw}],\"limitations\":[]}}"
    );
    assert!(
        parse_trend_research_response(&duplicate_findings, &input, "openai", "gpt-test").is_err()
    );

    for malformed in [
        r#"{"evidenceHash":"hash-current","limitations":[]}"#.to_string(),
        r#"{"evidenceHash":"hash-current","findings":{},"limitations":[]}"#.to_string(),
        format!(
            "{{\"evidenceHash\":\"hash-current\",\"findings\":[{valid_raw}],\"limitations\":[1e400]}}"
        ),
        format!(
            "{{\"evidenceHash\":\"hash-current\",\"findings\":[{valid_raw}],\"limitations\":[]}} {{}}"
        ),
    ] {
        assert!(
            parse_trend_research_response(&malformed, &input, "openai", "gpt-test").is_err(),
            "malformed top-level response was accepted: {malformed}"
        );
    }
}

#[test]
fn unified_personal_state_catalog_provides_dedicated_diagnostics() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    for term in ["聪明", "智力", "健康", "认知", "态度", "表现"] {
        let mut item = finding("previousEqualLength.summary.activeSeconds", "increased");
        item["possibleExplanation"] = json!(term);
        let error = parse_trend_research_response(
            &response("hash-current", vec![item]),
            &input,
            "openai",
            "gpt-test",
        )
        .expect_err("personal-state catalog term must be rejected");
        assert!(
            error.contains("personal or psychological state or trait inference"),
            "catalog term {term} used wrong diagnostic: {error}"
        );
        assert!(
            trend_research_protocol_prompt().contains(term),
            "protocol omitted catalog term: {term}"
        );
    }
}

#[test]
fn provider_limitations_are_discarded_and_replaced_with_local_policy() {
    let input = build_trend_research_input(&workbench(5, 0.49));
    let forbidden = [
        "当前指标下降",
        "这可能受学习活动影响",
        "这可能反映注意力不足",
        "样本只有 20%",
    ];
    let mut item = finding("previousEqualLength.summary.activeSeconds", "increased");
    item["possibleExplanation"] = json!("这可能受学习活动影响");
    item["limitations"] = json!(forbidden);
    let content = json!({
        "evidenceHash": "hash-current",
        "findings": [item],
        "limitations": forbidden
    })
    .to_string();

    let analysis = parse_trend_research_response(&content, &input, "openai", "gpt-test")
        .expect("provider limitation prose must be discarded rather than persisted");
    let serialized = serde_json::to_string(&analysis).unwrap();
    for text in forbidden {
        assert!(
            !serialized.contains(text),
            "provider limitation leaked: {text}"
        );
    }
    assert_eq!(
        analysis.findings[0].possible_explanation,
        "分类覆盖不足，暂不提供可能解释"
    );
    assert!(
        analysis
            .limitations
            .iter()
            .any(|item| item.contains("分类覆盖有限"))
    );
}

#[test]
fn numeric_literals_are_rejected_before_canonical_direction_validation() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    for text in [
        "相较前序周期，当前指标上升，活动投入约为 20%",
        "相较前序周期，当前指标上升，活动投入约为２０％",
        "相较前序周期，当前指标上升，活动投入约为𝟚倍",
        "相较前序周期，当前指标上升，活动投入约为百分之二十",
        "相较前序周期，当前指标上升，活动投入增加二十个百分点",
        "相较前序周期，当前指标上升，活动投入约为一半",
        "相较前序周期，当前指标上升，半数活动被记录",
        "相较前序周期，当前指标上升，多数活动被记录",
        "相较前序周期，当前指标上升，少数活动被记录",
        "相较前序周期，当前指标上升，活动投入成倍变化",
        "相较前序周期，当前指标上升，活动投入百分比变化",
        "相较前序周期，当前指标上升，记录持续一周",
        "相较前序周期，当前指标上升，包含一份记录",
        "相较前序周期，当前指标上升，记录归为一组",
        "相较前序周期，当前指标上升，记录归为一类",
    ] {
        let mut numeric = finding("previousEqualLength.summary.activeSeconds", "increased");
        numeric["observation"] = json!(text);
        let error = parse_trend_research_response(
            &response("hash-current", vec![numeric]),
            &input,
            "openai",
            "gpt-test",
        )
        .expect_err("numeric free text must be rejected");
        assert!(
            error.contains("numeric literal"),
            "wrong rejection for {text}: {error}"
        );
    }
}

#[test]
fn nonnumeric_uses_of_yi_are_allowed_in_operational_templates() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    let mut item = finding("previousEqualLength.summary.activeSeconds", "increased");
    item["possibleExplanation"] = json!("可能与前后周期一致的工作安排有关");
    item["validationMethod"] = json!("在上一周期与当前周期复核同类聚合证据");

    let analysis = parse_trend_research_response(
        &response("hash-current", vec![item]),
        &input,
        "openai",
        "gpt-test",
    )
    .expect("一致 and 上一周期 are nonnumeric context");
    assert_eq!(
        analysis.findings[0].possible_explanation,
        "可能与前后周期一致的工作安排有关"
    );
}

#[test]
fn explanations_and_validation_methods_require_operational_templates() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    for (text, expected) in [
        (
            "这可能反映注意力不足",
            "personal or psychological state or trait inference",
        ),
        (
            "这可能反映执行力不足",
            "personal or psychological state or trait inference",
        ),
        ("这可能受学习活动影响", "operational hypothesis"),
    ] {
        let mut item = finding("previousEqualLength.summary.activeSeconds", "increased");
        item["possibleExplanation"] = json!(text);
        let error = parse_trend_research_response(
            &response("hash-current", vec![item]),
            &input,
            "openai",
            "gpt-test",
        )
        .expect_err("non-operational explanation must be rejected");
        assert!(
            error.contains(expected),
            "wrong rejection for {text}: {error}"
        );
    }

    let mut inference_method = finding("previousEqualLength.summary.activeSeconds", "increased");
    inference_method["validationMethod"] = json!("后续周期检查用户注意力是否不足");
    let error = parse_trend_research_response(
        &response("hash-current", vec![inference_method]),
        &input,
        "openai",
        "gpt-test",
    )
    .expect_err("non-operational validation method must be rejected");
    assert!(error.contains("personal or psychological state or trait inference"));
}

#[test]
fn direction_language_must_match_authoritative_claims() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    for text in [
        "当前活动投入低于上一周期",
        "当前活动投入少于上一周期",
        "当前活动投入出现回落",
    ] {
        let mut item = finding("previousEqualLength.summary.activeSeconds", "increased");
        item["observation"] = json!(text);
        assert!(
            parse_trend_research_response(
                &response("hash-current", vec![item]),
                &input,
                "openai",
                "gpt-test",
            )
            .is_err(),
            "accepted opposing direction: {text}"
        );
    }
}

#[test]
fn mixed_direction_clauses_and_unbound_multi_evidence_comparisons_are_rejected() {
    let input = build_trend_research_input(&workbench(5, 0.8));
    let mut canonical = finding("previousEqualLength.summary.activeSeconds", "increased");
    canonical["evidenceIds"] = json!([
        "previousEqualLength.summary.activeSeconds",
        "current.summary.activeSeconds"
    ]);
    canonical["claims"] = json!([
        {
            "evidenceId": "previousEqualLength.summary.activeSeconds",
            "relation": "increased"
        },
        {"evidenceId": "current.summary.activeSeconds", "relation": "supports"}
    ]);
    assert!(
        parse_trend_research_response(
            &response("hash-current", vec![canonical.clone()]),
            &input,
            "openai",
            "gpt-test",
        )
        .is_ok(),
        "canonical direction should remain valid with factual supporting evidence"
    );

    for text in [
        "当前活动投入虽不及前序周期，但整体上升",
        "当前活动投入虽不及上一周期，但整体上升",
        "相较前序周期，当前指标上升，但部分时段下降",
        "相较前序周期，当前指标上升；附加证据显示活动投入不及基线",
    ] {
        let mut mixed = canonical.clone();
        mixed["observation"] = json!(text);
        let error = parse_trend_research_response(
            &response("hash-current", vec![mixed]),
            &input,
            "openai",
            "gpt-test",
        )
        .expect_err("mixed or extra comparative clauses must be rejected");
        assert!(
            error.contains("canonical evidence-bound direction"),
            "wrong rejection for {text}: {error}"
        );
    }
}

#[test]
fn canonical_observation_compatibility_matrix_is_explicit() {
    let base = build_trend_research_input(&workbench(5, 0.8));
    for (relation, observation) in [
        (EvidenceRelation::Decreased, "相较前序周期，当前指标下降"),
        (EvidenceRelation::Stable, "相较前序周期，当前指标保持稳定"),
    ] {
        let mut input = base.clone();
        input
            .evidence
            .iter_mut()
            .find(|item| item.id == "previousEqualLength.summary.activeSeconds")
            .unwrap()
            .allowed_relations = vec![EvidenceRelation::Supports, relation];
        let mut item = finding(
            "previousEqualLength.summary.activeSeconds",
            match relation {
                EvidenceRelation::Decreased => "decreased",
                EvidenceRelation::Stable => "stable",
                _ => unreachable!(),
            },
        );
        item["observation"] = json!(observation);
        assert!(
            parse_trend_research_response(
                &response("hash-current", vec![item]),
                &input,
                "openai",
                "gpt-test",
            )
            .is_ok(),
            "canonical observation rejected: {observation}"
        );
    }

    let mut support_only = finding("current.summary.activeSeconds", "supports");
    support_only["observation"] = json!("当前聚合证据可供复核");
    assert!(
        parse_trend_research_response(
            &response("hash-current", vec![support_only]),
            &base,
            "openai",
            "gpt-test",
        )
        .is_ok()
    );

    for (kind, evidence_id, relation, observation) in [
        (
            TrendBaselineKind::PreviousMonthSamePeriod,
            "previousMonthSamePeriod.summary.activeSeconds",
            EvidenceRelation::Increased,
            "相较上月同期，当前指标上升",
        ),
        (
            TrendBaselineKind::Custom,
            "custom.summary.activeSeconds",
            EvidenceRelation::Decreased,
            "相较自定义基线，当前指标下降",
        ),
    ] {
        let mut input = base.clone();
        if !input.enabled_baselines.contains(&kind) {
            input.enabled_baselines.push(kind);
        }
        input.evidence.retain(|item| item.id != evidence_id);
        input.evidence.push(TrendResearchEvidence {
            id: evidence_id.into(),
            value: 1.0,
            allowed_relations: vec![EvidenceRelation::Supports, relation],
        });
        let mut item = finding(
            evidence_id,
            match relation {
                EvidenceRelation::Increased => "increased",
                EvidenceRelation::Decreased => "decreased",
                _ => unreachable!(),
            },
        );
        item["observation"] = json!(observation);
        assert!(
            parse_trend_research_response(
                &response("hash-current", vec![item]),
                &input,
                "openai",
                "gpt-test",
            )
            .is_ok(),
            "canonical baseline observation rejected: {observation}"
        );
    }

    let mut two_direction_input = base.clone();
    two_direction_input
        .enabled_baselines
        .push(TrendBaselineKind::Custom);
    two_direction_input.evidence.push(TrendResearchEvidence {
        id: "custom.summary.activeSeconds".into(),
        value: 1.0,
        allowed_relations: vec![EvidenceRelation::Supports, EvidenceRelation::Decreased],
    });
    let mut two_direction = finding("previousEqualLength.summary.activeSeconds", "increased");
    two_direction["evidenceIds"] = json!([
        "previousEqualLength.summary.activeSeconds",
        "custom.summary.activeSeconds"
    ]);
    two_direction["claims"] = json!([
        {
            "evidenceId": "previousEqualLength.summary.activeSeconds",
            "relation": "increased"
        },
        {"evidenceId": "custom.summary.activeSeconds", "relation": "decreased"}
    ]);
    let error = parse_trend_research_response(
        &response("hash-current", vec![two_direction]),
        &two_direction_input,
        "openai",
        "gpt-test",
    )
    .expect_err("two directional claims must be rejected");
    assert!(error.contains("canonical evidence-bound direction"));
}

#[test]
fn low_classification_coverage_suppresses_provider_explanations() {
    let input = build_trend_research_input(&workbench(5, 0.49));
    for text in [
        "这可能因为学习类别增加而发生",
        "这可能由学习活动带来",
        "这可能与开发应用驱动有关",
        "这可能受学习活动影响",
        "压力",
    ] {
        let mut category_causal = finding("previousEqualLength.summary.activeSeconds", "increased");
        category_causal["possibleExplanation"] = json!(text);
        let analysis = parse_trend_research_response(
            &response("hash-current", vec![category_causal]),
            &input,
            "openai",
            "gpt-test",
        )
        .expect("low-coverage explanation should be suppressed, not exposed");
        assert_eq!(
            analysis.findings[0].possible_explanation, "分类覆盖不足，暂不提供可能解释",
            "provider explanation leaked at low coverage: {text}"
        );
    }
}

#[test]
fn preserves_baseline_quality_zero_facts_and_direction_tolerance() {
    let mut payload = workbench(5, 0.8);
    let previous = payload
        .baselines
        .iter_mut()
        .find(|baseline| baseline.kind == TrendBaselineKind::PreviousEqualLength)
        .unwrap();
    previous.value = Some(0.0);
    previous.absolute_delta = Some(3_600.0);
    previous.percent_delta = None;
    payload
        .evidence
        .iter_mut()
        .find(|item| item.id == "previousEqualLength.summary.activeSeconds")
        .unwrap()
        .value = 0.0;

    let input = build_trend_research_input(&payload);
    let zero = input
        .evidence
        .iter()
        .find(|item| item.id == "previousEqualLength.summary.activeSeconds")
        .expect("zero baseline remains factual evidence");
    assert_eq!(zero.value, 0.0);
    assert_eq!(zero.allowed_relations, [EvidenceRelation::Supports]);
    for id in [
        "previousEqualLength.quality.recordedDayCount",
        "previousEqualLength.quality.missingDayCount",
        "previousMonthSamePeriod.quality.valid",
    ] {
        assert!(
            input.evidence.iter().any(|item| item.id == id),
            "missing {id}"
        );
    }

    for (delta, expected) in [
        (7.99, EvidenceRelation::Stable),
        (8.0, EvidenceRelation::Increased),
        (-8.0, EvidenceRelation::Decreased),
    ] {
        let mut payload = workbench(5, 0.8);
        payload
            .baselines
            .iter_mut()
            .find(|baseline| baseline.kind == TrendBaselineKind::PreviousEqualLength)
            .unwrap()
            .percent_delta = Some(delta);
        let input = build_trend_research_input(&payload);
        let evidence = input
            .evidence
            .iter()
            .find(|item| item.id == "previousEqualLength.summary.activeSeconds")
            .unwrap();
        assert!(
            evidence.allowed_relations.contains(&expected),
            "delta {delta}"
        );
    }
}

#[test]
fn fewer_than_three_effective_days_returns_limitations_only() {
    let input = build_trend_research_input(&workbench(2, 0.8));
    let analysis = limitations_only_analysis(&input);

    assert_eq!(analysis.status, ResearchStatus::LimitationsOnly);
    assert!(analysis.findings.is_empty());
    assert!(!analysis.limitations.is_empty());
    assert_eq!(analysis.evidence_hash, "hash-current");
}

#[test]
fn queued_provider_payload_is_aggregate_only_and_excludes_raw_titles() {
    let payload = workbench(5, 0.8);
    let job = build_trend_research_job_payload(request(), &payload);
    let serialized = serde_json::to_string(&job).unwrap();

    for forbidden in [
        "timeline",
        "windowTitle",
        "browserVisits",
        "Secret raw activity title",
        "titleSummary",
        "rawRows",
        "Private App",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "provider payload leaked {forbidden}"
        );
    }
    assert!(serialized.contains("current.summary.activeSeconds"));
    assert!(serialized.contains("previousEqualLength"));
    assert!(!serialized.contains("previousMonthSamePeriod.summary.activeSeconds"));
}

#[test]
fn service_queue_freezes_concrete_provider_and_model_before_settings_change() {
    let database = Database::open_in_memory().unwrap();
    for (id, date) in [
        ("active-one", "2026-07-06"),
        ("active-two", "2026-07-07"),
        ("active-three", "2026-07-08"),
    ] {
        database.insert_segment(&active_segment(id, date)).unwrap();
    }
    let service = AppService::new(database);
    let workbench = service.get_trend_workbench(request()).unwrap();
    let queued_snapshot = snapshot(
        "queued-provider",
        "queued-model",
        &workbench.evidence_hash,
        1_000,
    );
    let id = service
        .queue_trend_research_analysis(request(), 1_000, Some(queued_snapshot.clone()), false)
        .unwrap()
        .unwrap();

    service
        .update_settings(SettingsPatch {
            ai_execution_mode: Some(AiExecutionMode::Codex),
            codex_executable: Some("changed-codex".into()),
            codex_model: Some("changed-model".into()),
            ..SettingsPatch::default()
        })
        .unwrap();

    let job = service.database().get_ai_job(&id).unwrap().unwrap();
    assert_eq!(job.execution, queued_snapshot);
    let claimed = service
        .database()
        .claim_next_due_ai_job(1_000)
        .unwrap()
        .unwrap();
    assert_eq!(claimed.execution.executor_id, "queued-provider");
    assert_eq!(claimed.execution.model, "queued-model");
    assert_eq!(claimed.execution.execution_mode, AiExecutionMode::ApiKey);
}

#[test]
fn research_storage_is_compatible_with_legacy_trend_rows_for_the_same_hash() {
    let database = Database::open_in_memory().unwrap();
    let hash = "shared-evidence-hash";
    database
        .save_trend_analysis(&TrendAnalysisRecord {
            range_start: "2026-07-06".into(),
            range_end: "2026-07-12".into(),
            evidence_hash: hash.into(),
            summary: "legacy summary".into(),
            observations: vec!["legacy observation".into()],
            suggestions: vec!["legacy suggestion".into()],
            source: "legacy-source".into(),
            model: "legacy-model".into(),
            confidence: 0.5,
            generated_at_ms: 1_000,
        })
        .unwrap();
    database
        .save_trend_research_analysis(&limitations_record(
            hash,
            "research-source",
            "research-model",
        ))
        .unwrap();

    let legacy = database
        .get_trend_analysis("2026-07-06", "2026-07-12", hash)
        .unwrap()
        .expect("legacy row remains readable");
    let research = database
        .get_trend_research_analysis("2026-07-06", "2026-07-12", hash)
        .unwrap()
        .expect("research row remains readable");
    assert_eq!(legacy.summary, "legacy summary");
    assert_eq!(research.analysis.source, "research-source");
}

#[test]
fn legacy_v1_research_prose_is_preserved_but_never_returned_to_the_backend_dto() {
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-task10-research-v2-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let database = Database::open(&path).unwrap();
    for (id, date) in [
        ("active-one", "2026-07-06"),
        ("active-two", "2026-07-07"),
        ("active-three", "2026-07-08"),
    ] {
        database.insert_segment(&active_segment(id, date)).unwrap();
    }
    let service = AppService::new(database);
    let workbench = service.get_trend_workbench(request()).unwrap();
    let hash = workbench.evidence_hash.clone();
    let unsafe_phrases = [
        "当前活动投入虽不及上一周期，但整体上升",
        "这可能受学习活动影响",
        "这可能反映注意力不足",
        "活动投入约为一半",
    ];
    let unsafe_finding = json!([{
        "observation": "当前聚合证据可供复核",
        "possibleExplanation": "可能与记录节奏有关",
        "validationMethod": "后续周期复核同类聚合证据",
        "evidenceIds": ["current.summary.activeSeconds"],
        "claims": [{
            "evidenceId": "current.summary.activeSeconds",
            "relation": "supports"
        }],
        "confidence": 0.8,
        "limitations": unsafe_phrases
    }]);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "INSERT INTO trend_analyses(
                range_start, range_end, evidence_hash, summary, observations_json,
                suggestions_json, source, model, confidence, generated_at_ms
             ) VALUES (?1, ?2, ?3, 'ready', ?4, ?5, 'legacy-provider',
                       'legacy-model', 0.8, 1000)",
            params![
                "2026-07-06",
                "2026-07-12",
                format!("research:v1:{hash}"),
                unsafe_finding.to_string(),
                json!(unsafe_phrases).to_string(),
            ],
        )
        .unwrap();
    drop(connection);

    assert!(
        service
            .database()
            .get_trend_research_analysis("2026-07-06", "2026-07-12", &hash)
            .unwrap()
            .is_none(),
        "legacy v1 research rows must not cross the current storage boundary"
    );
    let dto = service.get_trend_research_analysis(request()).unwrap();
    let serialized = serde_json::to_string(&dto).unwrap();
    assert_eq!(dto.status, ResearchStatus::LimitationsOnly);
    assert_eq!(dto.source, "local");
    for unsafe_phrase in unsafe_phrases {
        assert!(
            !serialized.contains(unsafe_phrase),
            "legacy unsafe prose reached the backend DTO: {unsafe_phrase}"
        );
    }

    service
        .database()
        .save_trend_research_analysis(&limitations_record(
            &hash,
            "current-provider",
            "current-model",
        ))
        .unwrap();
    let current = service.get_trend_research_analysis(request()).unwrap();
    assert_eq!(current.source, "current-provider");
    let connection = Connection::open(&path).unwrap();
    let preserved_v1_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM trend_analyses WHERE evidence_hash=?1",
            [format!("research:v1:{hash}")],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(preserved_v1_count, 1, "legacy v1 data must not be deleted");
    drop(connection);
    drop(service);
    let _ = std::fs::remove_file(path);
}

#[test]
fn invalid_response_audit_and_stale_research_generation_are_persisted_safely() {
    let database = Database::open_in_memory().unwrap();
    let invalid_id = database
        .enqueue_ai_job(
            "trend_research_analysis",
            "invalid-response-payload",
            1_000,
            &snapshot("queued-provider", "queued-model", "hash-invalid", 1_000),
        )
        .unwrap();
    let invalid_run = database.claim_next_due_ai_job(1_000).unwrap().unwrap();
    assert!(
        database
            .fail_ai_job_generation(
                &invalid_id,
                invalid_run.generation,
                1_100,
                "response contains no valid findings",
                AiExecutionErrorKind::InvalidResponse,
                Some("queued-provider"),
                Some("queued-model"),
                None,
            )
            .unwrap()
    );
    let failed = database.get_ai_job(&invalid_id).unwrap().unwrap();
    assert_eq!(failed.status, AiJobStatus::Pending);
    assert_eq!(
        failed.error_kind,
        Some(AiExecutionErrorKind::InvalidResponse)
    );
    assert_eq!(failed.executor_id.as_deref(), Some("queued-provider"));
    assert_eq!(failed.model.as_deref(), Some("queued-model"));

    let hash = "generation-hash";
    let subject = "2026-07-06:2026-07-12:generation-hash";
    let old_id = database
        .enqueue_ai_job_for_subject(
            "trend_research_analysis",
            subject,
            "payload",
            2_000,
            &snapshot("old-provider", "old-model", hash, 2_000),
        )
        .unwrap();
    let old_run = database.claim_next_due_ai_job(2_000).unwrap().unwrap();
    let new_id = database
        .force_enqueue_ai_job_for_subject(
            "trend_research_analysis",
            subject,
            "payload",
            3_000,
            &snapshot("new-provider", "new-model", hash, 3_000),
        )
        .unwrap();
    assert!(
        !database
            .complete_trend_research_analysis_job_generation(
                &old_id,
                old_run.generation,
                &limitations_record(hash, "old-provider", "old-model"),
            )
            .unwrap()
    );
    assert!(
        database
            .get_trend_research_analysis("2026-07-06", "2026-07-12", hash)
            .unwrap()
            .is_none()
    );

    let new_run = database.claim_next_due_ai_job(3_000).unwrap().unwrap();
    assert_eq!(new_run.id, new_id);
    assert!(
        database
            .complete_trend_research_analysis_job_generation(
                &new_id,
                new_run.generation,
                &limitations_record(hash, "new-provider", "new-model"),
            )
            .unwrap()
    );
    let saved = database
        .get_trend_research_analysis("2026-07-06", "2026-07-12", hash)
        .unwrap()
        .unwrap();
    assert_eq!(saved.analysis.source, "new-provider");
    assert_eq!(saved.analysis.model, "new-model");
}
