use crate::{db::ActivitySegmentRecord, domain::ActivityCategory};

/// Converts possibly overlapping collector output into one authoritative
/// segment stream for the requested range.
///
/// Active observations win over Idle observations. Ties are resolved
/// deterministically so every reporting surface produces the same result on
/// every supported platform, regardless of database row order.
pub fn canonicalize_activity_segments(
    segments: &[ActivitySegmentRecord],
    start_ms: i64,
    end_ms: i64,
) -> Vec<ActivitySegmentRecord> {
    if end_ms <= start_ms {
        return Vec::new();
    }

    let mut boundaries = vec![start_ms, end_ms];
    for segment in segments {
        let clipped_start = segment.started_at_ms.max(start_ms).min(end_ms);
        let clipped_end = segment.ended_at_ms.max(start_ms).min(end_ms);
        if clipped_end > clipped_start {
            boundaries.push(clipped_start);
            boundaries.push(clipped_end);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut canonical = Vec::<ActivitySegmentRecord>::new();
    for window in boundaries.windows(2) {
        let interval_start = window[0];
        let interval_end = window[1];
        let Some(winner) = segments
            .iter()
            .filter(|segment| {
                segment.started_at_ms < interval_end && segment.ended_at_ms > interval_start
            })
            .max_by(|left, right| {
                let left_active = left.category != ActivityCategory::Idle;
                let right_active = right.category != ActivityCategory::Idle;
                left_active
                    .cmp(&right_active)
                    .then_with(|| left.started_at_ms.cmp(&right.started_at_ms))
                    .then_with(|| left.ended_at_ms.cmp(&right.ended_at_ms))
                    .then_with(|| left.id.cmp(&right.id))
            })
        else {
            continue;
        };

        if let Some(previous) = canonical.last_mut()
            && previous.id == winner.id
            && previous.ended_at_ms == interval_start
        {
            previous.ended_at_ms = interval_end;
            continue;
        }

        let mut slice = winner.clone();
        slice.started_at_ms = interval_start;
        slice.ended_at_ms = interval_end;
        canonical.push(slice);
    }
    canonical
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ClassificationSource, VideoPurpose};

    fn segment(
        id: &str,
        category: ActivityCategory,
        started_at_ms: i64,
        ended_at_ms: i64,
    ) -> ActivitySegmentRecord {
        ActivitySegmentRecord {
            id: id.into(),
            started_at_ms,
            ended_at_ms,
            app: id.into(),
            app_path: String::new(),
            title: String::new(),
            category,
            video_purpose: VideoPurpose::Unknown,
            confidence: 1.0,
            source: ClassificationSource::Rule,
            reason: String::new(),
            model_version: String::new(),
            needs_review: false,
            inactivity_reason: None,
        }
    }

    #[test]
    fn active_segments_win_without_filling_unobserved_gaps() {
        let canonical = canonicalize_activity_segments(
            &[
                segment("idle-a", ActivityCategory::Idle, 0, 10_000),
                segment("idle-b", ActivityCategory::Idle, 0, 10_000),
                segment("active", ActivityCategory::Research, 2_000, 6_000),
                segment("after-gap", ActivityCategory::Research, 12_000, 14_000),
            ],
            0,
            20_000,
        );

        assert_eq!(
            canonical
                .iter()
                .map(|segment| (
                    segment.id.as_str(),
                    segment.started_at_ms,
                    segment.ended_at_ms
                ))
                .collect::<Vec<_>>(),
            [
                ("idle-b", 0, 2_000),
                ("active", 2_000, 6_000),
                ("idle-b", 6_000, 10_000),
                ("after-gap", 12_000, 14_000),
            ]
        );
        assert_eq!(
            canonical
                .iter()
                .map(|segment| segment.ended_at_ms - segment.started_at_ms)
                .sum::<i64>(),
            12_000
        );
    }
}
