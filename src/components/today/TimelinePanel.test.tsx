import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "../../App";
import appSource from "../../App.tsx?raw";
import { appIdentityKey, type AppIdentity } from "../../lib/app-identity";
import type { Segment } from "../../lib/metrics";
import { anchoredTimelineScrollLeft, buildTimelineClusters, IDLE_TIMELINE_COLOR, TimelinePanel, scrollIntoViewWithHeaderOffset } from "./TimelinePanel";

const segments: Segment[] = [
  {
    id: "research",
    startMs: 8 * 3_600_000,
    endMs: 9 * 3_600_000,
    app: "Chrome",
    title: "Psychology research",
    category: "research",
    videoPurpose: "unknown",
    confidence: 0.94,
    needsReview: false,
  },
  {
    id: "idle",
    startMs: 9 * 3_600_000,
    endMs: 9.5 * 3_600_000,
    app: "Idle",
    title: "Away",
    category: "idle",
    videoPurpose: "unknown",
    confidence: 0.98,
    needsReview: false,
  },
];

afterEach(() => vi.unstubAllGlobals());

describe("TimelinePanel", () => {
  it("anchors the timeline heading and renders only shared-filter matches", () => {
    const html = renderToStaticMarkup(
      <TimelinePanel
        segments={segments}
        filter={{ mode: "category", category: "research" }}
        onFilterChange={() => {}}
        onChangeClassification={() => {}}
      />,
    );

    expect(html).toContain('id="activity-timeline-heading"');
    expect(html).toContain('data-scroll-offset="header"');
    expect(html).toContain("Psychology research");
    expect(html).not.toContain(">Away<");
  });

  it("renders a zoomable continuous day track from midnight to midnight", () => {
    const html = renderToStaticMarkup(<TimelinePanel segments={segments} filter={{ mode: "all" }} onFilterChange={() => {}} onChangeClassification={() => {}} />);

    expect(html).toContain('aria-label="可缩放的全天应用时间线，每格五分钟，仅显示占比最高的应用"');
    expect(html).toContain('aria-label="时间线缩放级别"');
    expect(html).toContain("滚轮缩放 · 拖动平移 · 5 分钟 TOP 1");
    expect(html).toContain(">00:00<");
    expect(html).toContain(">24:00<");
    expect(html.match(/class="orbit-hour-grid"/g)).toHaveLength(24);
  });

  it("keeps the time beneath the mouse at the same viewport position while zooming", () => {
    expect(anchoredTimelineScrollLeft(.5, 2_400, 300)).toBe(900);
    expect(anchoredTimelineScrollLeft(.75, 4_800, 500)).toBe(3_100);
  });

  it("uses five-minute buckets and keeps only the top application in each bucket", () => {
    const hour = 3_600_000;
    const minute = 60_000;
    const base = { ...segments[0], startMs: 10 * hour, endMs: 10 * hour + 5 * minute };
    const summaries = buildTimelineClusters([
      { ...base, id: "chrome", app: "Chrome", endMs: 10 * hour + 4 * minute },
      { ...base, id: "code-early", app: "Code", startMs: 10 * hour + 4 * minute },
      { ...base, id: "code", app: "Code", startMs: 10 * hour + 5 * minute, endMs: 10 * hour + 8 * minute },
      { ...base, id: "notes", app: "Notes", startMs: 10 * hour + 8 * minute, endMs: 10 * hour + 10 * minute },
    ]);

    expect(summaries).toHaveLength(2);
    expect(summaries[0]).toMatchObject({ startMs: 10 * hour, endMs: 10 * hour + 5 * minute });
    expect(summaries[0].apps.map((item) => item.app)).toEqual(["Chrome"]);
    expect(summaries[1]).toMatchObject({ startMs: 10 * hour + 5 * minute, endMs: 10 * hour + 10 * minute });
    expect(summaries[1].apps.map((item) => item.app)).toEqual(["Code"]);
    expect(summaries.every((item) => item.apps.length === 1)).toBe(true);
  });

  it("keeps idle bands neutral instead of consuming an application palette color", () => {
    const clusters = buildTimelineClusters(segments);
    const idleBand = clusters.flatMap((cluster) => cluster.apps).find((app) => app.app === "Idle");
    const researchBand = clusters.flatMap((cluster) => cluster.apps).find((app) => app.app === "Chrome");

    expect(idleBand?.color).toBe(IDLE_TIMELINE_COLOR);
    expect(researchBand?.color).not.toBe(IDLE_TIMELINE_COLOR);
  });

  it("keeps common application colors visually distinct", () => {
    const clusters = buildTimelineClusters([
      { ...segments[0], id: "chatgpt", app: "ChatGPT", startMs: 8 * 3_600_000, endMs: 8.5 * 3_600_000 },
      { ...segments[0], id: "orbit", app: "Daily Task Monitor", startMs: 8.5 * 3_600_000, endMs: 9 * 3_600_000 },
    ]);
    const colors = new Map(clusters.map((cluster) => [cluster.apps[0].app, cluster.apps[0].color]));
    const channels = (color: string) => color.match(/[\da-f]{2}/gi)!.map((channel) => Number.parseInt(channel, 16));
    const chatgpt = channels(colors.get("ChatGPT")!);
    const orbit = channels(colors.get("Daily Task Monitor")!);
    const distance = Math.hypot(...chatgpt.map((channel, index) => channel - orbit[index]));

    expect(distance).toBeGreaterThan(120);
  });

  it("renders the resolved app identity and shared native icon", () => {
    const appPath = "C:\\Program Files\\Microsoft VS Code\\Code.exe";
    const identity: AppIdentity = {
      rawName: "Code",
      displayName: "Visual Studio Code",
      executablePath: appPath,
      productName: "Visual Studio Code",
      iconDataUrl: "data:image/png;base64,iVBORw0KGgo=",
    };
    const html = renderToStaticMarkup(
      <TimelinePanel
        segments={[{ ...segments[0], app: "Code", appPath }]}
        identities={new Map([[appIdentityKey("Code", appPath), identity]])}
        filter={{ mode: "all" }}
        onFilterChange={() => {}}
        onChangeClassification={() => {}}
      />,
    );

    expect(html).toContain("Visual Studio Code");
    expect(html).toContain('data-app-icon="native"');
  });

  it("labels macOS Visual Studio Code from its app bundle path", () => {
    const html = renderToStaticMarkup(
      <TimelinePanel
        segments={[{
          ...segments[0],
          app: "Code",
          appPath: "/Applications/Visual Studio Code.app/Contents/MacOS/Electron",
        }]}
        filter={{ mode: "all" }}
        onFilterChange={() => {}}
        onChangeClassification={() => {}}
      />,
    );

    expect(html).toContain("Visual Studio Code");
    expect(html).not.toContain(">Code</span>");
  });

  it("renders an accessible marker only for exact pending review subjects", () => {
    const html = renderToStaticMarkup(
      <TimelinePanel
        segments={[{ ...segments[0], needsReview: false }, { ...segments[1], needsReview: true }]}
        filter={{ mode: "all" }}
        onFilterChange={() => {}}
        onChangeClassification={() => {}}
        reviewSubjectIds={new Set(["research"])}
        onOpenAiReview={() => {}}
      />,
    );

    expect(html).toContain('aria-label="在 AI 审核中心查看 Psychology research"');
    expect(html).toContain('data-ai-review-subject="research"');
    expect(html).not.toContain('aria-label="在 AI 审核中心查看 Away"');
  });

  it("offers explicit learning and leisure video choices while merging unknown video into unclassified", () => {
    const html = renderToStaticMarkup(
      <TimelinePanel
        segments={[{ ...segments[0], category: "video_input", videoPurpose: "leisure" }]}
        filter={{ mode: "all" }}
        onFilterChange={() => {}}
        onChangeClassification={() => {}}
      />,
    );

    expect(html).toContain('value="video_input:learning"');
    expect(html).toContain('value="video_input:leisure" selected=""');
    expect(html).not.toContain('value="video_input:unknown"');
    expect(html).toContain('value="pending"');
    expect(html).toContain("学习视频");
    expect(html).toContain("休闲视频");
    expect(html).toContain("未分类");
  });

  it("places the timeline heading below the rendered header with 24px of breathing room", () => {
    const scrollTo = vi.fn();
    const heading = {
      getBoundingClientRect: () => ({ top: 250 }),
    } as unknown as HTMLElement;
    const header = {
      getBoundingClientRect: () => ({ height: 76 }),
    } as unknown as HTMLElement;
    vi.stubGlobal("window", { scrollY: 300, scrollTo });

    scrollIntoViewWithHeaderOffset(heading, header);

    expect(scrollTo).toHaveBeenCalledWith({ top: 450, behavior: "smooth" });
  });

  it("uses container coordinates with the rendered header height and 24px of clearance", () => {
    const scrollTo = vi.fn();
    const heading = {
      getBoundingClientRect: () => ({ top: 600 }),
    } as unknown as HTMLElement;
    const header = {
      getBoundingClientRect: () => ({ height: 92 }),
    } as unknown as HTMLElement;
    const container = {
      scrollTop: 200,
      getBoundingClientRect: () => ({ top: 100 }),
      scrollTo,
    } as unknown as HTMLElement;

    scrollIntoViewWithHeaderOffset(heading, header, container);

    expect(scrollTo).toHaveBeenCalledWith({ top: 584, behavior: "smooth" });
  });

  it("wires the rendered header ref into the existing pending-scroll handoff", () => {
    expect(appSource).toContain("const headerRef = useRef<HTMLElement>(null)");
    expect(appSource).toContain("<header ref={headerRef} className=\"app-header\">");
    expect(appSource).toContain("scrollIntoViewWithHeaderOffset(timeline, headerRef.current, scrollContainer)");
  });
});

describe("App focus and timeline composition", () => {
  it("keeps focus in the compact header popover and removes the action queue", () => {
    const html = renderToStaticMarkup(<App initialSegments={segments} />);

    expect(html).not.toContain("今日行动队列");
    expect(html).not.toContain('class="panel focus-card"');
    expect(html).toContain('aria-label="打开专注工具"');
  });
});
