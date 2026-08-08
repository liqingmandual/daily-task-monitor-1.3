use sha2::{Digest, Sha256};

use crate::db::{ActivitySegmentRecord, MonitoringContinuityCheckpoint};
use crate::domain::{ActivityCategory, ClassificationSource, InactivityReason, VideoPurpose};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitoringGap {
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub reason: InactivityReason,
}

pub fn monitoring_gap_from_checkpoint(
    checkpoint: &MonitoringContinuityCheckpoint,
    observed_at_ms: i64,
    boot_started_at_ms: i64,
    uptime_ms: i64,
    threshold_ms: i64,
) -> Option<MonitoringGap> {
    if !checkpoint.expected_tracking
        || observed_at_ms <= checkpoint.last_observed_at_ms
        || observed_at_ms.saturating_sub(checkpoint.last_observed_at_ms) <= threshold_ms.max(0)
    {
        return None;
    }

    let wall_delta_ms = observed_at_ms.saturating_sub(checkpoint.last_observed_at_ms);
    let same_boot = checkpoint.last_boot_started_at_ms > 0
        && boot_started_at_ms > 0
        && checkpoint
            .last_boot_started_at_ms
            .abs_diff(boot_started_at_ms)
            <= 120_000;
    let elapsed_ms = if same_boot && uptime_ms >= checkpoint.last_uptime_ms {
        let uptime_delta_ms = uptime_ms.saturating_sub(checkpoint.last_uptime_ms);
        if wall_delta_ms.abs_diff(uptime_delta_ms) > 120_000 {
            uptime_delta_ms
        } else {
            wall_delta_ms
        }
    } else {
        wall_delta_ms
    };
    if elapsed_ms <= threshold_ms.max(0) {
        return None;
    }
    let started_at_ms = if elapsed_ms == wall_delta_ms {
        checkpoint.last_observed_at_ms
    } else {
        observed_at_ms.saturating_sub(elapsed_ms)
    };
    if started_at_ms >= observed_at_ms {
        return None;
    }
    Some(MonitoringGap {
        started_at_ms,
        ended_at_ms: observed_at_ms,
        reason: InactivityReason::ContinuityGap,
    })
}

pub fn continuity_gap_segment(gap: &MonitoringGap) -> ActivitySegmentRecord {
    let identity = format!("continuity_gap\n{}\n{}", gap.started_at_ms, gap.ended_at_ms);
    let hash = format!("{:x}", Sha256::digest(identity.as_bytes()));
    ActivitySegmentRecord {
        id: format!("idle-gap-{}", &hash[..24]),
        started_at_ms: gap.started_at_ms,
        ended_at_ms: gap.ended_at_ms,
        app: "Idle".into(),
        app_path: String::new(),
        title: "Monitoring gap".into(),
        category: ActivityCategory::Idle,
        video_purpose: VideoPurpose::Unknown,
        confidence: 1.0,
        source: ClassificationSource::Idle,
        reason: "Monitoring continuity gap".into(),
        model_version: "continuity-gap-v1".into(),
        needs_review: false,
        inactivity_reason: Some(gap.reason),
    }
}
