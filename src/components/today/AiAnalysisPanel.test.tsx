import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AiAnalysisPanel } from "./AiAnalysisPanel";

describe("AiAnalysisPanel", () => {
  it("identifies the activity scope used for the analysis", () => {
    const html = renderToStaticMarkup(
      <AiAnalysisPanel
        analysis={{
          portrait: "Focused work",
          recommendation: "Continue",
          source: "ai",
          evidenceHash: "scope-hash",
          generatedAtMs: 1,
          activityScope: "meaningful",
        }}
      />,
    );

    expect(html).toContain("当前口径：学习");
  });
});
