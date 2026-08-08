import { useState } from "react";
import type { ReportFormat } from "../../lib/desktop";

export function DailyMarkdownExportButton({
  date,
  desktopRuntime,
  exporter,
  onMessage,
}: {
  date: string;
  desktopRuntime: boolean;
  exporter: (date: string, format: ReportFormat) => Promise<string | null>;
  onMessage: (message: string) => void;
}) {
  const [pendingFormat, setPendingFormat] = useState<ReportFormat | null>(null);

  const exportDocument = async (format: ReportFormat) => {
    if (pendingFormat) return;
    if (!desktopRuntime) {
      onMessage("请在桌面安装版中选择报告保存位置");
      return;
    }
    setPendingFormat(format);
    const label = format === "markdown" ? "Markdown" : "Word";
    onMessage(`正在导出今日日报（${label}）…`);
    try {
      const path = await exporter(date, format);
      onMessage(path ? `报告已保存：${path}` : "已取消导出");
    } catch (error) {
      onMessage(`报告导出失败：${String(error)}`);
    } finally {
      setPendingFormat(null);
    }
  };

  return <div className="report-export-menu" aria-label="今日日报导出">
    <button
      className="wide-button"
      aria-label="导出今日日报 Markdown"
      onClick={() => void exportDocument("markdown")}
      disabled={pendingFormat !== null}
    >
      {pendingFormat === "markdown" ? "正在导出…" : "导出 Markdown"}
    </button>
    <button
      className="wide-button"
      aria-label="导出今日日报 Word"
      onClick={() => void exportDocument("docx")}
      disabled={pendingFormat !== null}
    >
      {pendingFormat === "docx" ? "正在导出…" : "导出 Word"}
    </button>
  </div>;
}
