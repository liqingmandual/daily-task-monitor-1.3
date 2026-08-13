use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalContextKind {
    CalendarEvent,
    Project,
    Task,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalContextItem {
    pub id: String,
    pub source_id: String,
    pub source_name: String,
    pub source_kind: String,
    pub external_id: String,
    pub kind: ExternalContextKind,
    pub title: String,
    pub start_at_ms: Option<i64>,
    pub end_at_ms: Option<i64>,
    pub project_name: String,
    pub status: String,
    pub imported_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalContextImport {
    pub source_id: String,
    pub source_name: String,
    pub source_kind: String,
    pub items: Vec<ExternalContextItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectContextDocument {
    source_id: String,
    source_name: String,
    projects: Vec<ProjectContextProject>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectContextProject {
    id: String,
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    tasks: Vec<ProjectContextTask>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectContextTask {
    id: String,
    title: String,
    #[serde(default)]
    status: String,
}

pub fn parse_project_context_json(
    json: &str,
    imported_at_ms: i64,
) -> Result<ExternalContextImport, String> {
    let document: ProjectContextDocument = serde_json::from_str(json)
        .map_err(|error| format!("project context JSON is invalid: {error}"))?;
    validate_source(&document.source_id, &document.source_name)?;
    let mut items = Vec::new();
    for project in document.projects {
        if project.id.trim().is_empty() || project.name.trim().is_empty() {
            return Err("project context requires non-empty project IDs and names".to_string());
        }
        items.push(context_item(
            &document.source_id,
            &document.source_name,
            "project_json",
            &project.id,
            ExternalContextKind::Project,
            &project.name,
            None,
            None,
            &project.name,
            &project.status,
            imported_at_ms,
        ));
        for task in project.tasks {
            if task.id.trim().is_empty() || task.title.trim().is_empty() {
                return Err("project context requires non-empty task IDs and titles".to_string());
            }
            items.push(context_item(
                &document.source_id,
                &document.source_name,
                "project_json",
                &task.id,
                ExternalContextKind::Task,
                &task.title,
                None,
                None,
                &project.name,
                &task.status,
                imported_at_ms,
            ));
        }
    }
    Ok(ExternalContextImport {
        source_id: document.source_id,
        source_name: document.source_name,
        source_kind: "project_json".to_string(),
        items,
    })
}

pub fn parse_ics_calendar(
    source_id: &str,
    source_name: &str,
    ics: &str,
    imported_at_ms: i64,
) -> Result<ExternalContextImport, String> {
    validate_source(source_id, source_name)?;
    let unfolded = unfold_ics(ics);
    let mut items = Vec::new();
    let mut current = Vec::<(String, String)>::new();
    let mut in_event = false;
    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');
        if line == "BEGIN:VEVENT" {
            in_event = true;
            current.clear();
            continue;
        }
        if line == "END:VEVENT" {
            if in_event {
                items.push(calendar_item(
                    source_id,
                    source_name,
                    &current,
                    imported_at_ms,
                )?);
            }
            in_event = false;
            current.clear();
            continue;
        }
        if in_event && let Some((key, value)) = line.split_once(':') {
            current.push((key.to_string(), unescape_ics(value)));
        }
    }
    Ok(ExternalContextImport {
        source_id: source_id.to_string(),
        source_name: source_name.to_string(),
        source_kind: "ics".to_string(),
        items,
    })
}

fn calendar_item(
    source_id: &str,
    source_name: &str,
    fields: &[(String, String)],
    imported_at_ms: i64,
) -> Result<ExternalContextItem, String> {
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key.split(';').next() == Some(name))
            .map(|(key, value)| (key.as_str(), value.as_str()))
    };
    let external_id = field("UID")
        .map(|(_, value)| value)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "calendar event is missing UID".to_string())?;
    let title = field("SUMMARY")
        .map(|(_, value)| value)
        .unwrap_or("Untitled event");
    let start_at_ms = field("DTSTART")
        .map(|(property, value)| parse_calendar_time(property, value))
        .transpose()?;
    let end_at_ms = field("DTEND")
        .map(|(property, value)| parse_calendar_time(property, value))
        .transpose()?;
    Ok(context_item(
        source_id,
        source_name,
        "ics",
        external_id,
        ExternalContextKind::CalendarEvent,
        title,
        start_at_ms,
        end_at_ms,
        "",
        field("STATUS").map(|(_, value)| value).unwrap_or(""),
        imported_at_ms,
    ))
}

#[allow(clippy::too_many_arguments)]
fn context_item(
    source_id: &str,
    source_name: &str,
    source_kind: &str,
    external_id: &str,
    kind: ExternalContextKind,
    title: &str,
    start_at_ms: Option<i64>,
    end_at_ms: Option<i64>,
    project_name: &str,
    status: &str,
    imported_at_ms: i64,
) -> ExternalContextItem {
    let identity = format!("{source_id}\n{source_kind}\n{external_id}\n{kind:?}");
    let hash = format!("{:x}", Sha256::digest(identity.as_bytes()));
    ExternalContextItem {
        id: format!("context-{}", &hash[..32]),
        source_id: source_id.to_string(),
        source_name: source_name.to_string(),
        source_kind: source_kind.to_string(),
        external_id: external_id.to_string(),
        kind,
        title: title.trim().to_string(),
        start_at_ms,
        end_at_ms,
        project_name: project_name.trim().to_string(),
        status: status.trim().to_string(),
        imported_at_ms,
    }
}

fn validate_source(source_id: &str, source_name: &str) -> Result<(), String> {
    if source_id.trim().is_empty() || source_name.trim().is_empty() {
        Err("external context source ID and name must not be empty".to_string())
    } else {
        Ok(())
    }
}

fn parse_calendar_time(property: &str, value: &str) -> Result<i64, String> {
    if property
        .split(';')
        .skip(1)
        .any(|parameter| parameter.starts_with("TZID="))
    {
        return Err(format!(
            "calendar timezone parameters are not supported yet; export this event as UTC: {property}"
        ));
    }
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Ok(value.timestamp_millis());
    }
    if let Ok(value) = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ") {
        return Ok(value.and_utc().timestamp_millis());
    }
    if let Ok(value) = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S") {
        return Local
            .from_local_datetime(&value)
            .single()
            .map(|value| value.timestamp_millis())
            .ok_or_else(|| format!("calendar time is ambiguous in the local timezone: {value}"));
    }
    if let Ok(value) = NaiveDate::parse_from_str(value, "%Y%m%d") {
        return Local
            .from_local_datetime(&value.and_hms_opt(0, 0, 0).unwrap())
            .single()
            .map(|value| value.timestamp_millis())
            .ok_or_else(|| format!("calendar date is ambiguous in the local timezone: {value}"));
    }
    Err(format!("unsupported calendar date: {value}"))
}

fn unfold_ics(value: &str) -> String {
    value
        .replace("\r\n ", "")
        .replace("\r\n\t", "")
        .replace("\n ", "")
        .replace("\n\t", "")
}

fn unescape_ics(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_calendar_without_urls_or_activity_evidence() {
        let parsed = parse_ics_calendar(
            "calendar-work",
            "Work Calendar",
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:event-1\r\nSUMMARY:Planning\\, weekly\r\nDTSTART:20260813T010000Z\r\nDTEND:20260813T013000Z\r\nEND:VEVENT\r\nEND:VCALENDAR",
            10,
        )
        .unwrap();
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].title, "Planning, weekly");
        assert_eq!(parsed.items[0].kind, ExternalContextKind::CalendarEvent);
        assert_eq!(parsed.items[0].start_at_ms, Some(1_786_582_800_000));
    }

    #[test]
    fn parses_provider_neutral_project_exports() {
        let parsed = parse_project_context_json(
            r#"{"sourceId":"linear-export","sourceName":"Linear","projects":[{"id":"p1","name":"Orbit","status":"active","tasks":[{"id":"t1","title":"Sync context","status":"started"}]}]}"#,
            10,
        )
        .unwrap();
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].kind, ExternalContextKind::Project);
        assert_eq!(parsed.items[1].kind, ExternalContextKind::Task);
        assert_eq!(parsed.items[1].project_name, "Orbit");
    }

    #[test]
    fn rejects_explicit_calendar_timezones_instead_of_silently_shifting_them() {
        let result = parse_ics_calendar(
            "calendar-work",
            "Work Calendar",
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:event-1\r\nSUMMARY:Planning\r\nDTSTART;TZID=America/New_York:20260813T090000\r\nEND:VEVENT\r\nEND:VCALENDAR",
            10,
        );
        assert!(result.unwrap_err().contains("export this event as UTC"));
    }
}
