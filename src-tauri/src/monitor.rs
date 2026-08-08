use sha2::{Digest, Sha256};
use std::collections::VecDeque;

use crate::classifier::{ActivityEvidence, ManualRule, RuleClassifier};
use crate::db::ActivitySegmentRecord;
use crate::domain::{ActivityCategory, ClassificationSource, InactivityReason, VideoPurpose};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorSample {
    pub observed_at_ms: i64,
    pub last_input_at_ms: i64,
    pub app: String,
    pub app_path: String,
    pub title: String,
    pub domain: String,
    pub key_presses: u32,
    pub mouse_events: u32,
    pub media_playing: bool,
}

#[derive(Debug, Clone)]
pub struct MonitorOutput {
    pub completed: Vec<ActivitySegmentRecord>,
    pub current: ActivitySegmentRecord,
}

pub struct MonitorEngine {
    idle_threshold_ms: i64,
    current: Option<ActivitySegmentRecord>,
    classifier: RuleClassifier,
    last_media_observed_at_ms: Option<i64>,
    media_was_playing: bool,
    segment_key_presses: u32,
    recent_window_switches: VecDeque<i64>,
}

impl MonitorEngine {
    pub fn new(idle_threshold_ms: i64) -> Self {
        Self {
            idle_threshold_ms: idle_threshold_ms.max(60_000),
            current: None,
            classifier: RuleClassifier::default(),
            last_media_observed_at_ms: None,
            media_was_playing: false,
            segment_key_presses: 0,
            recent_window_switches: VecDeque::new(),
        }
    }

    pub fn set_idle_threshold_ms(&mut self, value: i64) {
        self.idle_threshold_ms = value.max(60_000);
    }

    pub fn set_manual_rules(&mut self, rules: Vec<ManualRule>) {
        self.classifier = RuleClassifier::new(rules);
    }

    pub fn take_current(&mut self) -> Option<ActivitySegmentRecord> {
        self.segment_key_presses = 0;
        self.recent_window_switches.clear();
        self.last_media_observed_at_ms = None;
        self.media_was_playing = false;
        self.current.take()
    }

    pub fn ingest(&mut self, sample: MonitorSample) -> MonitorOutput {
        if sample.media_playing || self.media_was_playing {
            self.last_media_observed_at_ms = Some(sample.observed_at_ms);
        }
        self.media_was_playing = sample.media_playing;
        let idle_start_ms = sample
            .last_input_at_ms
            .max(self.last_media_observed_at_ms.unwrap_or_default())
            .max(0)
            .min(sample.observed_at_ms);
        let is_idle = !sample.media_playing
            && sample.observed_at_ms.saturating_sub(idle_start_ms) >= self.idle_threshold_ms;
        let continues_subject = self.current.as_ref().is_some_and(|current| {
            current.category != ActivityCategory::Idle
                && current.app == sample.app
                && current.title == sample.title
        });
        if !continues_subject && self.current.is_some() {
            self.recent_window_switches.push_back(sample.observed_at_ms);
            self.segment_key_presses = 0;
        }
        while self
            .recent_window_switches
            .front()
            .is_some_and(|at_ms| sample.observed_at_ms.saturating_sub(*at_ms) > 60_000)
        {
            self.recent_window_switches.pop_front();
        }
        self.segment_key_presses = self.segment_key_presses.saturating_add(sample.key_presses);

        let next = if is_idle {
            self.make_segment(
                idle_start_ms,
                sample.observed_at_ms,
                "Idle",
                "",
                "Away from computer",
                ActivityCategory::Idle,
                VideoPurpose::Unknown,
                1.0,
                ClassificationSource::Idle,
                "No keyboard or mouse input beyond the configured threshold",
                "idle-v1",
                false,
            )
        } else {
            let started_at_ms = if continues_subject {
                self.current
                    .as_ref()
                    .map(|current| current.started_at_ms)
                    .unwrap_or(sample.observed_at_ms)
            } else {
                sample.observed_at_ms
            };
            let classification = self.classifier.classify(&ActivityEvidence {
                app: sample.app.clone(),
                title: sample.title.clone(),
                domain: sample.domain.clone(),
                duration_seconds: sample.observed_at_ms.saturating_sub(started_at_ms) / 1_000,
                key_presses: self.segment_key_presses,
                window_switches: self.recent_window_switches.len() as u32,
                media_playing: sample.media_playing,
            });
            self.make_segment(
                started_at_ms,
                sample.observed_at_ms,
                &sample.app,
                &sample.app_path,
                &sample.title,
                classification.category,
                classification.video_purpose,
                classification.confidence,
                classification.source,
                &classification.reason,
                &classification.model_version,
                classification.needs_review,
            )
        };

        let mut completed = Vec::new();
        match self.current.take() {
            None => self.current = Some(next),
            Some(mut current) if same_subject(&current, &next) => {
                current.ended_at_ms = sample.observed_at_ms.max(current.started_at_ms);
                current.category = next.category;
                current.video_purpose = next.video_purpose;
                current.confidence = next.confidence;
                current.source = next.source;
                current.reason = next.reason;
                current.model_version = next.model_version;
                current.needs_review = next.needs_review;
                self.current = Some(current);
            }
            Some(mut current) => {
                let boundary = if next.category == ActivityCategory::Idle {
                    next.started_at_ms
                } else {
                    sample.observed_at_ms
                };
                current.ended_at_ms = boundary.max(current.started_at_ms);
                completed.push(current);
                self.current = Some(next);
            }
        }

        let current = self.current.as_mut().expect("current segment exists");
        current.ended_at_ms = current.ended_at_ms.max(sample.observed_at_ms);
        MonitorOutput {
            completed,
            current: current.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn make_segment(
        &self,
        started_at_ms: i64,
        ended_at_ms: i64,
        app: &str,
        app_path: &str,
        title: &str,
        category: ActivityCategory,
        video_purpose: VideoPurpose,
        confidence: f32,
        source: ClassificationSource,
        reason: &str,
        model_version: &str,
        needs_review: bool,
    ) -> ActivitySegmentRecord {
        let identity = format!("{started_at_ms}\n{app}\n{app_path}\n{title}");
        let hash = format!("{:x}", Sha256::digest(identity.as_bytes()));
        ActivitySegmentRecord {
            id: format!("native-{}", &hash[..24]),
            started_at_ms,
            ended_at_ms,
            app: app.into(),
            app_path: app_path.into(),
            title: title.into(),
            category,
            video_purpose,
            confidence,
            source,
            reason: reason.into(),
            model_version: model_version.into(),
            needs_review,
            inactivity_reason: (category == ActivityCategory::Idle)
                .then_some(InactivityReason::InputIdle),
        }
    }
}

fn same_subject(left: &ActivitySegmentRecord, right: &ActivitySegmentRecord) -> bool {
    if left.category == ActivityCategory::Idle || right.category == ActivityCategory::Idle {
        return left.category == right.category;
    }
    left.app == right.app && left.app_path == right.app_path && left.title == right.title
}
