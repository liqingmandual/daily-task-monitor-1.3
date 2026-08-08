use std::error::Error;
use std::fs;
use std::path::Path;

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::classifier::{ActivityEvidence, RuleClassifier};
use crate::db::{ActivitySegmentRecord, Database};
use crate::domain::{ActivityCategory, ClassificationSource, InactivityReason, VideoPurpose};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub segment_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyActivity {
    start: String,
    end: String,
    #[serde(default)]
    duration_seconds: i64,
    #[serde(default)]
    app: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    is_idle: bool,
}

pub fn import_activity_jsonl(
    database: &Database,
    source: &Path,
) -> Result<LegacyImportResult, Box<dyn Error + Send + Sync>> {
    let content = fs::read_to_string(source)?;
    let mut result = LegacyImportResult::default();

    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let legacy: LegacyActivity = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => {
                result.skipped += 1;
                continue;
            }
        };
        let started_at_ms = DateTime::parse_from_rfc3339(&legacy.start)?.timestamp_millis();
        let parsed_end_ms = DateTime::parse_from_rfc3339(&legacy.end)?.timestamp_millis();
        let ended_at_ms =
            parsed_end_ms.max(started_at_ms.saturating_add(legacy.duration_seconds.max(0) * 1_000));
        let id_hash = format!("{:x}", Sha256::digest(line.as_bytes()));
        let id = format!("legacy-{}", &id_hash[..24]);
        let (category, video_purpose, confidence) = map_legacy_category(&legacy);
        let segment = ActivitySegmentRecord {
            id: id.clone(),
            started_at_ms,
            ended_at_ms,
            app: legacy.app,
            app_path: String::new(),
            title: legacy.title,
            category,
            video_purpose,
            confidence,
            source: ClassificationSource::Rule,
            reason: "Imported from the legacy JSONL log".into(),
            model_version: "legacy-import-v1".into(),
            needs_review: category == ActivityCategory::Pending,
            inactivity_reason: (category == ActivityCategory::Idle)
                .then_some(InactivityReason::InputIdle),
        };

        if database.insert_segment_with_origin(&segment, "legacy")? {
            result.imported += 1;
            result.segment_ids.push(id);
        } else {
            result.skipped += 1;
        }
    }

    Ok(result)
}

fn map_legacy_category(legacy: &LegacyActivity) -> (ActivityCategory, VideoPurpose, f32) {
    if legacy.is_idle || legacy.category.contains("空闲") {
        return (ActivityCategory::Idle, VideoPurpose::Unknown, 1.0);
    }
    if legacy.category.contains("搜索") || legacy.category.contains("调研") {
        return (ActivityCategory::Research, VideoPurpose::Unknown, 0.95);
    }
    if legacy.category.contains("沟通") || legacy.category.contains("社交") {
        return (ActivityCategory::Social, VideoPurpose::Unknown, 0.95);
    }
    if legacy.category.contains("文件") {
        return (
            ActivityCategory::FileManagement,
            VideoPurpose::Unknown,
            0.95,
        );
    }
    if legacy.category.contains("创作")
        || legacy.category.contains("开发")
        || legacy.category.contains("文档")
        || legacy.category.contains("输出")
    {
        return (
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            0.9,
        );
    }

    let inferred = RuleClassifier::default().classify(&ActivityEvidence {
        app: legacy.app.clone(),
        title: legacy.title.clone(),
        domain: String::new(),
        duration_seconds: legacy.duration_seconds,
        key_presses: 0,
        window_switches: 0,
        media_playing: false,
    });
    (
        inferred.category,
        inferred.video_purpose,
        inferred.confidence,
    )
}
