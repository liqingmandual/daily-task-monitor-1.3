use daily_task_monitor_core::activity_composition::{
    ActivityCompositionSlice, build_activity_compositions,
};
use daily_task_monitor_core::domain::{
    ActivityCategory, ActivityDisplayKey, MeaningfulReason, VideoPurpose,
};

fn slice(
    scope_key: &str,
    category: ActivityCategory,
    video_purpose: VideoPurpose,
    workflow_linked: bool,
) -> ActivityCompositionSlice {
    ActivityCompositionSlice {
        scope_key: scope_key.into(),
        category,
        video_purpose,
        seconds: 10 * 60,
        workflow_linked,
    }
}

#[test]
fn authoritative_composition_merges_unknown_video_into_pending_and_learning_is_strict() {
    let composition = build_activity_compositions([
        slice(
            "creation",
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            false,
        ),
        slice(
            "research",
            ActivityCategory::Research,
            VideoPurpose::Unknown,
            false,
        ),
        slice(
            "text",
            ActivityCategory::TextInput,
            VideoPurpose::Unknown,
            false,
        ),
        slice(
            "learning-video",
            ActivityCategory::VideoInput,
            VideoPurpose::Learning,
            false,
        ),
        slice(
            "leisure-video",
            ActivityCategory::VideoInput,
            VideoPurpose::Leisure,
            false,
        ),
        slice(
            "unknown-video",
            ActivityCategory::VideoInput,
            VideoPurpose::Unknown,
            false,
        ),
        slice("game", ActivityCategory::Game, VideoPurpose::Unknown, false),
        slice("idle", ActivityCategory::Idle, VideoPurpose::Unknown, false),
        slice(
            "pending",
            ActivityCategory::Pending,
            VideoPurpose::Unknown,
            false,
        ),
        slice(
            "linked-file",
            ActivityCategory::FileManagement,
            VideoPurpose::Unknown,
            true,
        ),
        slice(
            "unlinked-file",
            ActivityCategory::FileManagement,
            VideoPurpose::Unknown,
            false,
        ),
        slice(
            "linked-social",
            ActivityCategory::Social,
            VideoPurpose::Unknown,
            true,
        ),
        slice(
            "unlinked-social",
            ActivityCategory::Social,
            VideoPurpose::Unknown,
            false,
        ),
    ]);

    assert_eq!(composition.all.total_seconds, 130 * 60);
    assert_eq!(composition.meaningful.total_seconds, 40 * 60);

    let all_file = composition
        .all
        .items
        .iter()
        .find(|item| item.key == ActivityDisplayKey::FileManagement)
        .unwrap();
    assert_eq!(all_file.seconds, 20 * 60);
    assert_eq!(all_file.meaningful_reason, MeaningfulReason::Excluded);
    assert!(
        !composition
            .meaningful
            .items
            .iter()
            .any(|item| item.key == ActivityDisplayKey::FileManagement)
    );
    assert!(
        !composition
            .meaningful
            .items
            .iter()
            .any(|item| item.key == ActivityDisplayKey::Social)
    );

    assert!(composition.all.items.iter().any(|item| {
        item.key == ActivityDisplayKey::LearningVideo
            && item.video_purpose == Some(VideoPurpose::Learning)
    }));
    assert!(composition.all.items.iter().any(|item| {
        item.key == ActivityDisplayKey::LeisureVideo
            && item.video_purpose == Some(VideoPurpose::Leisure)
    }));
    assert!(
        !composition
            .all
            .items
            .iter()
            .any(|item| item.key == ActivityDisplayKey::UnknownVideo)
    );
    let pending = composition
        .all
        .items
        .iter()
        .find(|item| item.key == ActivityDisplayKey::Pending)
        .unwrap();
    assert_eq!(pending.seconds, 20 * 60);
    assert_eq!(pending.category, ActivityCategory::Pending);
    assert_eq!(pending.video_purpose, None);
    assert!(!composition.meaningful.items.iter().any(|item| matches!(
        item.key,
        ActivityDisplayKey::Idle
            | ActivityDisplayKey::Game
            | ActivityDisplayKey::LeisureVideo
            | ActivityDisplayKey::UnknownVideo
            | ActivityDisplayKey::Pending
    )));
}

#[test]
fn duplicate_workflow_links_do_not_duplicate_the_same_activity_piece() {
    let mut first = slice(
        "activity:shared:0",
        ActivityCategory::Social,
        VideoPurpose::Unknown,
        true,
    );
    first.seconds = 300;
    let mut duplicate = first.clone();
    duplicate.seconds = 300;

    let composition = build_activity_compositions([first, duplicate]);

    assert_eq!(composition.all.total_seconds, 300);
    assert_eq!(composition.meaningful.total_seconds, 0);
    assert_eq!(composition.all.items[0].seconds, 300);
}
