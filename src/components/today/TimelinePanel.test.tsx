import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "../../App";
import appSource from "../../App.tsx?raw";
import { appIdentityKey, type AppIdentity } from "../../lib/app-identity";
import type { Segment } from "../../lib/metrics";
import { TimelinePanel, scrollIntoViewWithHeaderOffset } from "./TimelinePanel";

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

  it("renders filtered activity newest first without mutating the input", () => {
    const latest: Segment = {
      ...segments[0],
      id: "latest",
      startMs: 11 * 3_600_000,
      endMs: 12 * 3_600_000,
      title: "Latest activity",
    };
    const middle: Segment = {
      ...segments[0],
      id: "middle",
      startMs: 10 * 3_600_000,
      endMs: 10.5 * 3_600_000,
      title: "Middle activity",
    };
    const unordered = [segments[0], latest, middle];
    const originalIds = unordered.map((item) => item.id);

    const html = renderToStaticMarkup(
      <TimelinePanel
        segments={unordered}
        filter={{ mode: "all" }}
        onFilterChange={() => {}}
        onChangeClassification={() => {}}
      />,
    );

    expect(html.indexOf("Latest activity")).toBeLessThan(html.indexOf("Middle activity"));
    expect(html.indexOf("Middle activity")).toBeLessThan(html.indexOf("Psychology research"));
    expect(unordered.map((item) => item.id)).toEqual(originalIds);
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
