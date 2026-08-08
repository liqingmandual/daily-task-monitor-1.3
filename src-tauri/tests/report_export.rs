use std::io::{Cursor, Read};

use daily_task_monitor_core::report::{ReportBlock, ReportDocument, render_docx, render_markdown};
use zip::ZipArchive;

fn fixture() -> ReportDocument {
    ReportDocument {
        title: "任务质量报告：桌面重写".into(),
        metadata: vec![
            ("范围".into(), "2026-07-20 至 2026-07-25".into()),
            ("生成方式".into(), "本地统计".into()),
        ],
        blocks: vec![
            ReportBlock::Heading {
                level: 2,
                text: "摘要".into(),
            },
            ReportBlock::Paragraph("累计投入 5 小时 30 分钟。".into()),
            ReportBlock::Table {
                headers: vec!["日期".into(), "投入".into()],
                rows: vec![
                    vec!["2026-07-24".into(), "2 小时".into()],
                    vec!["2026-07-25".into(), "3 小时 30 分钟".into()],
                ],
            },
            ReportBlock::Callout {
                kind: "info".into(),
                title: "数据质量".into(),
                body: "浏览记录仅作为证据，不增加时长。".into(),
            },
        ],
    }
}

#[test]
fn markdown_and_docx_are_rendered_from_the_same_report_document() {
    let report = fixture();
    let markdown = render_markdown(&report);
    assert!(markdown.starts_with("---\n"));
    assert!(markdown.contains("# 任务质量报告：桌面重写"));
    assert!(markdown.contains("| 日期 | 投入 |"));
    assert!(markdown.contains("> [!info] 数据质量"));

    let bytes = render_docx(&report).unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut document_xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut document_xml)
        .unwrap();
    assert!(document_xml.contains("任务质量报告：桌面重写"));
    assert!(document_xml.contains("2026-07-25"));
    assert!(document_xml.contains("<w:tbl>"));
    assert!(document_xml.contains("浏览记录仅作为证据"));
}
