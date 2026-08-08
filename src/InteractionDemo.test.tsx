import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import InteractionDemo from "./InteractionDemo";

describe("InteractionDemo", () => {
  it("renders the refined mineral interaction prototype", () => {
    const html = renderToStaticMarkup(<InteractionDemo />);

    expect(html).toContain("雾蓝矿物分析台");
    expect(html).toContain("两小时节律矩阵");
    expect(html).toContain('class="demo-period-grid"');
    expect((html.match(/class="period-quadrant"/g) ?? []).length).toBe(4);
    expect((html.match(/class="demo-bucket"/g) ?? []).length).toBe(12);
    expect(html).toContain("返回当前表盘");
    expect(html).toContain("声音反馈");
    expect((html.match(/class="mineral-donut"/g) ?? []).length).toBe(2);
    expect(html).not.toContain("three-donut");
  });
});
