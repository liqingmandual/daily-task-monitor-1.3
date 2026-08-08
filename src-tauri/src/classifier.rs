use crate::domain::{ActivityCategory, Classification, ClassificationSource, VideoPurpose};

pub const AI_AUTO_APPLY_THRESHOLD: f64 = 0.85;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiDisposition {
    AutoApply,
    Review,
    ManualLock,
}

pub fn decide_ai_disposition(confidence: f64, manually_owned: bool) -> AiDisposition {
    if manually_owned {
        AiDisposition::ManualLock
    } else if !confidence.is_finite() {
        AiDisposition::Review
    } else if confidence >= AI_AUTO_APPLY_THRESHOLD {
        AiDisposition::AutoApply
    } else {
        AiDisposition::Review
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityEvidence {
    pub app: String,
    pub title: String,
    pub domain: String,
    pub duration_seconds: i64,
    pub key_presses: u32,
    pub window_switches: u32,
    pub media_playing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualRule {
    app: Option<String>,
    title: Option<String>,
    domain: Option<String>,
    category: ActivityCategory,
    video_purpose: VideoPurpose,
}

impl ManualRule {
    pub fn app(app: impl Into<String>, category: ActivityCategory) -> Self {
        Self {
            app: Some(normalize(&app.into())),
            title: None,
            domain: None,
            category,
            video_purpose: VideoPurpose::Unknown,
        }
    }

    pub fn domain(domain: impl Into<String>, category: ActivityCategory) -> Self {
        Self {
            app: None,
            title: None,
            domain: Some(normalize(&domain.into())),
            category,
            video_purpose: VideoPurpose::Unknown,
        }
    }

    pub fn app_title(
        app: impl Into<String>,
        title: impl Into<String>,
        category: ActivityCategory,
    ) -> Self {
        Self::app_title_with_video_purpose(app, title, category, VideoPurpose::Unknown)
    }

    pub fn app_title_with_video_purpose(
        app: impl Into<String>,
        title: impl Into<String>,
        category: ActivityCategory,
        video_purpose: VideoPurpose,
    ) -> Self {
        Self {
            app: Some(normalize(&app.into())),
            title: Some(normalize(&title.into())),
            domain: None,
            category,
            video_purpose,
        }
    }

    fn matches(&self, evidence: &ActivityEvidence) -> bool {
        self.app.as_ref().is_some_and(|value| {
            normalize(&evidence.app).contains(value)
                && self
                    .title
                    .as_ref()
                    .is_none_or(|title| normalize(&evidence.title) == *title)
        }) || self
            .domain
            .as_ref()
            .is_some_and(|value| normalize(&evidence.domain) == *value)
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuleClassifier {
    manual_rules: Vec<ManualRule>,
}

impl RuleClassifier {
    pub fn new(manual_rules: Vec<ManualRule>) -> Self {
        Self { manual_rules }
    }

    pub fn classify(&self, evidence: &ActivityEvidence) -> Classification {
        if let Some(rule) = self.manual_rules.iter().find(|rule| rule.matches(evidence)) {
            return result(
                rule.category,
                rule.video_purpose,
                1.0,
                ClassificationSource::Manual,
                "Matched a user rule",
                evidence.duration_seconds,
                false,
            );
        }

        let text = normalize(&format!(
            "{} {} {}",
            evidence.app, evidence.title, evidence.domain
        ));

        if contains_any(
            &text,
            &[
                "leagueclient",
                "league of legends",
                "steam",
                "wegame",
                "valorant",
                "minecraft",
            ],
        ) {
            return rule_result(ActivityCategory::Game, "Known game application", evidence);
        }

        if contains_any(
            &text,
            &[
                "wechat",
                "微信",
                "qq",
                "telegram",
                "discord",
                "dingtalk",
                "钉钉",
                "feishu",
                "飞书",
                "teams",
                "weibo.com",
                "x.com",
                "twitter.com",
            ],
        ) {
            return rule_result(
                ActivityCategory::Social,
                "Known social application or domain",
                evidence,
            );
        }

        if contains_any(
            &text,
            &["explorer", "everything", "文件资源管理器", "finder"],
        ) {
            return rule_result(
                ActivityCategory::FileManagement,
                "Known file management application",
                evidence,
            );
        }

        if evidence.media_playing
            && contains_any(
                &text,
                &[
                    "youtube", "bilibili", "netflix", "video", "lecture", "course", "movie", "直播",
                ],
            )
        {
            let purpose = if contains_any(
                &text,
                &[
                    "lecture",
                    "course",
                    "tutorial",
                    "lesson",
                    "公开课",
                    "课程",
                    "教程",
                    "study",
                ],
            ) {
                VideoPurpose::Learning
            } else if contains_any(
                &text,
                &["movie", "comedy", "搞笑", "电影", "综艺", "game stream"],
            ) {
                VideoPurpose::Leisure
            } else {
                VideoPurpose::Unknown
            };
            return result(
                ActivityCategory::VideoInput,
                purpose,
                if purpose == VideoPurpose::Unknown {
                    0.68
                } else {
                    0.94
                },
                ClassificationSource::Rule,
                "Active Windows media session",
                evidence.duration_seconds,
                purpose == VideoPurpose::Unknown,
            );
        }

        if contains_any(
            &text,
            &[
                "google search",
                "bing.com",
                "baidu.com",
                "搜索",
                "perplexity",
                "chatgpt",
            ],
        ) || (evidence.key_presses >= 20 && evidence.window_switches >= 5)
        {
            return result(
                ActivityCategory::Research,
                VideoPurpose::Unknown,
                0.86,
                ClassificationSource::Behavior,
                "Search page or query-heavy switching pattern",
                evidence.duration_seconds,
                false,
            );
        }

        if evidence.duration_seconds >= 120
            && evidence.window_switches <= 2
            && evidence.key_presses <= 12
            && contains_any(
                &text,
                &[
                    "acrobat", ".pdf", "obsidian", "kindle", "reader", "阅读", "paper",
                ],
            )
        {
            return result(
                ActivityCategory::TextInput,
                VideoPurpose::Unknown,
                0.82,
                ClassificationSource::Behavior,
                "Sustained low-interaction reading pattern",
                evidence.duration_seconds,
                false,
            );
        }

        if evidence.key_presses > 0
            && contains_any(
                &text,
                &[
                    "codex",
                    "visual studio",
                    "vscode",
                    "code.exe",
                    "cursor",
                    "idea",
                    "pycharm",
                    "word",
                    "typora",
                ],
            )
        {
            return result(
                ActivityCategory::CreationDevelopment,
                VideoPurpose::Unknown,
                0.9,
                ClassificationSource::Behavior,
                "Editing activity in a creation tool",
                evidence.duration_seconds,
                false,
            );
        }

        result(
            ActivityCategory::Pending,
            VideoPurpose::Unknown,
            0.2,
            ClassificationSource::Pending,
            "Insufficient local evidence",
            evidence.duration_seconds,
            true,
        )
    }
}

fn rule_result(
    category: ActivityCategory,
    reason: &'static str,
    evidence: &ActivityEvidence,
) -> Classification {
    result(
        category,
        VideoPurpose::Unknown,
        0.98,
        ClassificationSource::Rule,
        reason,
        evidence.duration_seconds,
        false,
    )
}

fn result(
    category: ActivityCategory,
    video_purpose: VideoPurpose,
    confidence: f32,
    source: ClassificationSource,
    reason: impl Into<String>,
    duration_seconds: i64,
    needs_review: bool,
) -> Classification {
    Classification {
        category,
        video_purpose,
        confidence,
        source,
        reason: reason.into(),
        model_version: "rules-v1".into(),
        duration_seconds: duration_seconds.max(0),
        needs_review,
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn contains_any(text: &str, values: &[&str]) -> bool {
    values.iter().any(|value| text.contains(value))
}
