use std::io::{Cursor, Write};

use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDocument {
    pub title: String,
    pub metadata: Vec<(String, String)>,
    pub blocks: Vec<ReportBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "blockType", rename_all = "snake_case")]
pub enum ReportBlock {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph(String),
    BulletList(Vec<String>),
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Callout {
        kind: String,
        title: String,
        body: String,
    },
}

impl ReportDocument {
    pub fn from_markdown(title: impl Into<String>, markdown: &str) -> Self {
        let title = title.into();
        let mut blocks = Vec::new();
        let mut lines = markdown.lines().peekable();
        let mut in_frontmatter = false;
        let mut frontmatter_seen = false;
        while let Some(line) = lines.next() {
            if line.trim() == "---" && !frontmatter_seen {
                in_frontmatter = !in_frontmatter;
                if !in_frontmatter {
                    frontmatter_seen = true;
                }
                continue;
            }
            if in_frontmatter || line.trim().is_empty() {
                continue;
            }
            if let Some(text) = line.strip_prefix("### ") {
                blocks.push(ReportBlock::Heading {
                    level: 3,
                    text: text.trim().into(),
                });
                continue;
            }
            if let Some(text) = line.strip_prefix("## ") {
                blocks.push(ReportBlock::Heading {
                    level: 2,
                    text: text.trim().into(),
                });
                continue;
            }
            if line.starts_with("# ") {
                continue;
            }
            if let Some(callout) = line.strip_prefix("> [!") {
                let (kind, callout_title) = callout
                    .split_once(']')
                    .map(|(kind, title)| (kind.trim(), title.trim()))
                    .unwrap_or(("info", ""));
                let mut body = Vec::new();
                while let Some(next) = lines.peek().copied() {
                    let Some(content) = next.strip_prefix('>') else {
                        break;
                    };
                    lines.next();
                    body.push(content.trim().to_string());
                }
                blocks.push(ReportBlock::Callout {
                    kind: kind.into(),
                    title: callout_title.into(),
                    body: body.join("\n"),
                });
                continue;
            }
            if line.trim_start().starts_with("- ") {
                let mut items = vec![line.trim_start()[2..].trim().to_string()];
                while let Some(next) = lines.peek().copied() {
                    if !next.trim_start().starts_with("- ") {
                        break;
                    }
                    lines.next();
                    items.push(next.trim_start()[2..].trim().to_string());
                }
                blocks.push(ReportBlock::BulletList(items));
                continue;
            }
            if line.trim_start().starts_with('|') {
                let headers = parse_markdown_row(line);
                if lines
                    .peek()
                    .is_some_and(|next| next.contains("---") && next.trim_start().starts_with('|'))
                {
                    lines.next();
                }
                let mut rows = Vec::new();
                while let Some(next) = lines.peek().copied() {
                    if !next.trim_start().starts_with('|') {
                        break;
                    }
                    lines.next();
                    rows.push(parse_markdown_row(next));
                }
                blocks.push(ReportBlock::Table { headers, rows });
                continue;
            }
            blocks.push(ReportBlock::Paragraph(line.trim().to_string()));
        }
        Self {
            title,
            metadata: Vec::new(),
            blocks,
        }
    }
}

fn parse_markdown_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().replace("\\|", "|"))
        .collect()
}

pub fn render_markdown(report: &ReportDocument) -> String {
    let mut output = String::from("---\n");
    output.push_str(&format!("title: \"{}\"\n", yaml_escape(&report.title)));
    for (key, value) in &report.metadata {
        output.push_str(&format!(
            "{}: \"{}\"\n",
            safe_frontmatter_key(key),
            yaml_escape(value)
        ));
    }
    output.push_str("---\n\n");
    output.push_str(&format!("# {}\n", report.title));
    for block in &report.blocks {
        output.push('\n');
        match block {
            ReportBlock::Heading { level, text } => {
                output.push_str(&format!(
                    "{} {}\n",
                    "#".repeat((*level).clamp(2, 6) as usize),
                    text
                ));
            }
            ReportBlock::Paragraph(text) => {
                output.push_str(text);
                output.push('\n');
            }
            ReportBlock::BulletList(items) => {
                for item in items {
                    output.push_str(&format!("- {item}\n"));
                }
            }
            ReportBlock::Table { headers, rows } => {
                output.push_str(&markdown_table_row(headers));
                output.push_str(&markdown_table_row(
                    &headers
                        .iter()
                        .map(|_| "---".to_string())
                        .collect::<Vec<_>>(),
                ));
                for row in rows {
                    output.push_str(&markdown_table_row(row));
                }
            }
            ReportBlock::Callout { kind, title, body } => {
                output.push_str(&format!("> [!{}] {}\n", kind, title));
                for line in body.lines() {
                    output.push_str(&format!("> {line}\n"));
                }
            }
        }
    }
    output
}

fn markdown_table_row(cells: &[String]) -> String {
    format!(
        "| {} |\n",
        cells
            .iter()
            .map(|cell| cell.replace('|', "\\|").replace('\n', "<br>"))
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

fn yaml_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn safe_frontmatter_key(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = normalized.trim_matches('_');
    if trimmed.is_empty() {
        "meta".into()
    } else {
        trimmed.into()
    }
}

pub fn render_docx(report: &ReportDocument) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(Vec::new());
    let mut archive = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    write_zip_entry(&mut archive, "[Content_Types].xml", CONTENT_TYPES, options)?;
    write_zip_entry(&mut archive, "_rels/.rels", ROOT_RELATIONSHIPS, options)?;
    write_zip_entry(
        &mut archive,
        "word/_rels/document.xml.rels",
        DOCUMENT_RELATIONSHIPS,
        options,
    )?;
    write_zip_entry(&mut archive, "word/styles.xml", STYLES_XML, options)?;
    write_zip_entry(
        &mut archive,
        "word/document.xml",
        &document_xml(report),
        options,
    )?;
    archive
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|error| error.to_string())
}

fn write_zip_entry(
    archive: &mut ZipWriter<Cursor<Vec<u8>>>,
    path: &str,
    content: &str,
    options: SimpleFileOptions,
) -> Result<(), String> {
    archive
        .start_file(path, options)
        .map_err(|error| error.to_string())?;
    archive
        .write_all(content.as_bytes())
        .map_err(|error| error.to_string())
}

fn document_xml(report: &ReportDocument) -> String {
    let mut body = paragraph_xml(&report.title, Some("Title"), false);
    if !report.metadata.is_empty() {
        body.push_str(&table_xml(
            &["项目".into(), "内容".into()],
            &report
                .metadata
                .iter()
                .map(|(key, value)| vec![key.clone(), value.clone()])
                .collect::<Vec<_>>(),
        ));
    }
    for block in &report.blocks {
        match block {
            ReportBlock::Heading { level, text } => {
                body.push_str(&paragraph_xml(
                    text,
                    Some(if *level <= 2 { "Heading1" } else { "Heading2" }),
                    false,
                ));
            }
            ReportBlock::Paragraph(text) => body.push_str(&paragraph_xml(text, None, false)),
            ReportBlock::BulletList(items) => {
                for item in items {
                    body.push_str(&paragraph_xml(&format!("• {item}"), None, false));
                }
            }
            ReportBlock::Table { headers, rows } => body.push_str(&table_xml(headers, rows)),
            ReportBlock::Callout {
                title, body: text, ..
            } => {
                body.push_str(&paragraph_xml(title, Some("Heading2"), false));
                body.push_str(&paragraph_xml(text, None, false));
            }
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body>
</w:document>"#
    )
}

fn paragraph_xml(text: &str, style: Option<&str>, bold: bool) -> String {
    let style_xml = style
        .map(|style| format!(r#"<w:pPr><w:pStyle w:val="{style}"/></w:pPr>"#))
        .unwrap_or_default();
    let bold_xml = if bold { "<w:b/>" } else { "" };
    let lines = text.lines().collect::<Vec<_>>();
    let mut runs = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            runs.push_str("<w:r><w:br/></w:r>");
        }
        runs.push_str(&format!(
            r#"<w:r><w:rPr>{bold_xml}<w:rFonts w:eastAsia="Microsoft YaHei"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
            xml_escape(line)
        ));
    }
    format!("<w:p>{style_xml}{runs}</w:p>")
}

fn table_xml(headers: &[String], rows: &[Vec<String>]) -> String {
    let mut xml = String::from(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="0" w:type="auto"/></w:tblPr>"#,
    );
    if !headers.is_empty() {
        xml.push_str(&table_row_xml(headers, true));
    }
    for row in rows {
        xml.push_str(&table_row_xml(row, false));
    }
    xml.push_str("</w:tbl>");
    xml
}

fn table_row_xml(cells: &[String], bold: bool) -> String {
    let mut xml = String::from("<w:tr>");
    for cell in cells {
        xml.push_str("<w:tc><w:tcPr><w:tcW w:w=\"0\" w:type=\"auto\"/></w:tcPr>");
        xml.push_str(&paragraph_xml(cell, None, bold));
        xml.push_str("</w:tc>");
    }
    xml.push_str("</w:tr>");
    xml
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;

const ROOT_RELATIONSHIPS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELATIONSHIPS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

const STYLES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:rFonts w:eastAsia="Microsoft YaHei"/><w:sz w:val="21"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="36"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="24"/></w:rPr></w:style>
  <w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/></w:tblBorders></w:tblPr></w:style>
</w:styles>"#;
