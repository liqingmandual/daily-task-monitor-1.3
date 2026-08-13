use daily_task_monitor_core::classifier::{ActivityEvidence, ManualRule, RuleClassifier};
use daily_task_monitor_core::domain::{ActivityCategory, ClassificationSource, VideoPurpose};

fn evidence(app: &str, title: &str) -> ActivityEvidence {
    ActivityEvidence {
        app: app.into(),
        title: title.into(),
        domain: String::new(),
        duration_seconds: 300,
        key_presses: 0,
        window_switches: 0,
        media_playing: false,
    }
}

#[test]
fn manual_rule_has_highest_priority() {
    let classifier = RuleClassifier::new(vec![ManualRule::app(
        "chrome",
        ActivityCategory::CreationDevelopment,
    )]);

    let result = classifier.classify(&evidence("chrome", "Google Search"));

    assert_eq!(result.category, ActivityCategory::CreationDevelopment);
    assert_eq!(result.source, ClassificationSource::Manual);
}

#[test]
fn classifies_game_social_file_and_video_rules() {
    let classifier = RuleClassifier::default();

    assert_eq!(
        classifier
            .classify(&evidence("LeagueClientUx", "League of Legends"))
            .category,
        ActivityCategory::Game
    );
    assert_eq!(
        classifier.classify(&evidence("WeChat", "微信")).category,
        ActivityCategory::Social
    );
    assert_eq!(
        classifier
            .classify(&evidence("explorer", "Documents"))
            .category,
        ActivityCategory::FileManagement
    );

    let mut video = evidence("chrome", "Lecture 4 - YouTube");
    video.domain = "youtube.com".into();
    video.media_playing = true;
    let result = classifier.classify(&video);
    assert_eq!(result.category, ActivityCategory::VideoInput);
    assert_eq!(result.video_purpose, VideoPurpose::Learning);
}

#[test]
fn distinguishes_research_from_sustained_text_reading() {
    let classifier = RuleClassifier::default();

    let mut research = evidence("chrome", "psychology bachelor - Google Search");
    research.domain = "google.com".into();
    research.key_presses = 25;
    research.window_switches = 8;
    assert_eq!(
        classifier.classify(&research).category,
        ActivityCategory::Research
    );

    let mut reading = evidence("Acrobat", "Research paper.pdf");
    reading.duration_seconds = 600;
    reading.key_presses = 3;
    reading.window_switches = 1;
    assert_eq!(
        classifier.classify(&reading).category,
        ActivityCategory::TextInput
    );
}

#[test]
fn editing_tools_with_input_are_creation_development() {
    let classifier = RuleClassifier::default();
    let mut item = evidence("Codex", "Daily Task Monitor - app.tsx");
    item.key_presses = 40;

    assert_eq!(
        classifier.classify(&item).category,
        ActivityCategory::CreationDevelopment
    );
}

#[test]
fn unknown_activity_is_pending_instead_of_forced() {
    let classifier = RuleClassifier::default();
    let result = classifier.classify(&evidence("mystery-app", "No title"));

    assert_eq!(result.category, ActivityCategory::Pending);
    assert!(result.needs_review);
}
