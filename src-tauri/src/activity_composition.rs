use std::collections::BTreeMap;

use crate::domain::{
    ActivityCategory, ActivityComposition, ActivityCompositionItem, ActivityCompositions,
    ActivityDisplayKey, MeaningfulReason, VideoPurpose,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityCompositionSlice {
    pub scope_key: String,
    pub category: ActivityCategory,
    pub video_purpose: VideoPurpose,
    pub seconds: i64,
    pub workflow_linked: bool,
}

#[derive(Debug, Clone)]
struct AggregateItem {
    category: ActivityCategory,
    video_purpose: Option<VideoPurpose>,
    seconds: i64,
    meaningful_reason: MeaningfulReason,
}

pub fn activity_display_key(
    category: ActivityCategory,
    video_purpose: VideoPurpose,
) -> ActivityDisplayKey {
    match (category, video_purpose) {
        (ActivityCategory::Idle, _) => ActivityDisplayKey::Idle,
        (ActivityCategory::Research, _) => ActivityDisplayKey::Research,
        (ActivityCategory::VideoInput, VideoPurpose::Learning) => ActivityDisplayKey::LearningVideo,
        (ActivityCategory::VideoInput, VideoPurpose::Leisure) => ActivityDisplayKey::LeisureVideo,
        // Unknown-purpose video is intentionally presented as unclassified.
        // Keep the legacy enum variant deserializable, but do not emit it from
        // new authoritative aggregates.
        (ActivityCategory::VideoInput, VideoPurpose::Unknown) => ActivityDisplayKey::Pending,
        (ActivityCategory::TextInput, _) => ActivityDisplayKey::TextInput,
        (ActivityCategory::Game, _) => ActivityDisplayKey::Game,
        (ActivityCategory::Social, _) => ActivityDisplayKey::Social,
        (ActivityCategory::CreationDevelopment, _) => ActivityDisplayKey::CreationDevelopment,
        (ActivityCategory::FileManagement, _) => ActivityDisplayKey::FileManagement,
        (ActivityCategory::Pending, _) => ActivityDisplayKey::Pending,
    }
}

pub fn meaningful_reason(
    category: ActivityCategory,
    video_purpose: VideoPurpose,
) -> MeaningfulReason {
    match (category, video_purpose) {
        (
            ActivityCategory::Research
            | ActivityCategory::TextInput
            | ActivityCategory::CreationDevelopment,
            _,
        )
        | (ActivityCategory::VideoInput, VideoPurpose::Learning) => MeaningfulReason::Core,
        _ => MeaningfulReason::Excluded,
    }
}

pub fn activity_is_meaningful(
    category: ActivityCategory,
    video_purpose: VideoPurpose,
    _workflow_linked: bool,
) -> bool {
    meaningful_reason(category, video_purpose) == MeaningfulReason::Core
}

pub fn effective_meaningful_reason(
    category: ActivityCategory,
    video_purpose: VideoPurpose,
    workflow_linked: bool,
) -> MeaningfulReason {
    if activity_is_meaningful(category, video_purpose, workflow_linked) {
        meaningful_reason(category, video_purpose)
    } else {
        MeaningfulReason::Excluded
    }
}

pub fn build_activity_compositions(
    slices: impl IntoIterator<Item = ActivityCompositionSlice>,
) -> ActivityCompositions {
    let mut deduplicated = BTreeMap::<String, ActivityCompositionSlice>::new();
    for mut slice in slices {
        slice.seconds = slice.seconds.max(0);
        if slice.seconds == 0 {
            continue;
        }
        deduplicated
            .entry(slice.scope_key.clone())
            .and_modify(|current| {
                current.seconds = current.seconds.max(slice.seconds);
                current.workflow_linked |= slice.workflow_linked;
            })
            .or_insert(slice);
    }

    let mut all = BTreeMap::<ActivityDisplayKey, AggregateItem>::new();
    let mut active = BTreeMap::<ActivityDisplayKey, AggregateItem>::new();
    let mut meaningful = BTreeMap::<ActivityDisplayKey, AggregateItem>::new();
    for slice in deduplicated.into_values() {
        let key = activity_display_key(slice.category, slice.video_purpose);
        let reason = meaningful_reason(slice.category, slice.video_purpose);
        let (display_category, display_video_purpose) = if key == ActivityDisplayKey::Pending {
            (ActivityCategory::Pending, VideoPurpose::Unknown)
        } else {
            (slice.category, slice.video_purpose)
        };
        add_seconds(
            &mut all,
            key,
            display_category,
            display_video_purpose,
            reason,
            slice.seconds,
        );
        if slice.category != ActivityCategory::Idle {
            add_seconds(
                &mut active,
                key,
                display_category,
                display_video_purpose,
                reason,
                slice.seconds,
            );
        }
        if activity_is_meaningful(slice.category, slice.video_purpose, slice.workflow_linked) {
            add_seconds(
                &mut meaningful,
                key,
                slice.category,
                slice.video_purpose,
                reason,
                slice.seconds,
            );
        }
    }

    ActivityCompositions {
        all: finish_composition(all),
        active: finish_composition(active),
        meaningful: finish_composition(meaningful),
    }
}

fn add_seconds(
    items: &mut BTreeMap<ActivityDisplayKey, AggregateItem>,
    key: ActivityDisplayKey,
    category: ActivityCategory,
    video_purpose: VideoPurpose,
    meaningful_reason: MeaningfulReason,
    seconds: i64,
) {
    let video_purpose = (category == ActivityCategory::VideoInput).then_some(video_purpose);
    let item = items.entry(key).or_insert(AggregateItem {
        category,
        video_purpose,
        seconds: 0,
        meaningful_reason,
    });
    item.seconds = item.seconds.saturating_add(seconds);
}

fn finish_composition(items: BTreeMap<ActivityDisplayKey, AggregateItem>) -> ActivityComposition {
    let total_seconds = items
        .values()
        .fold(0_i64, |total, item| total.saturating_add(item.seconds));
    let mut items = items
        .into_iter()
        .map(|(key, item)| ActivityCompositionItem {
            key,
            category: item.category,
            video_purpose: item.video_purpose,
            seconds: item.seconds,
            share: if total_seconds > 0 {
                item.seconds as f64 / total_seconds as f64
            } else {
                0.0
            },
            meaningful_reason: item.meaningful_reason,
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .seconds
            .cmp(&left.seconds)
            .then_with(|| left.key.cmp(&right.key))
    });
    ActivityComposition {
        total_seconds,
        items,
    }
}
