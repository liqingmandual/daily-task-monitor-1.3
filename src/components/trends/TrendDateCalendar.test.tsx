import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TrendDateCalendar } from "./TrendDateCalendar";

function keyboardEvent(event: Event, key: string) {
  Object.defineProperty(event, "key", { value: key });
  return event;
}

function trackActiveElement(document: Document, window: { HTMLElement: typeof HTMLElement }) {
  let activeElement: Element | null = null;
  Object.defineProperty(document, "activeElement", { configurable: true, get: () => activeElement });
  Object.defineProperty(window.HTMLElement.prototype, "focus", {
    configurable: true,
    value(this: Element) { activeElement = this; },
  });
}

async function mountCalendar(initialDates: string[] = []) {
  const { document, window } = parseHTML('<!doctype html><html><body><div id="root"></div></body></html>');
  vi.stubGlobal("window", window);
  vi.stubGlobal("document", document);
  vi.stubGlobal("navigator", window.navigator);
  vi.stubGlobal("HTMLElement", window.HTMLElement);
  vi.stubGlobal("Node", window.Node);
  vi.stubGlobal("Event", window.Event);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const container = document.getElementById("root") as unknown as HTMLDivElement;
  const root = createRoot(container);

  function Harness() {
    const [dates, setDates] = useState(initialDates);
    return <TrendDateCalendar initialMonth="2026-07-01" selectedDates={dates} onSelectedDatesChange={setDates} />;
  }

  await act(async () => root.render(<Harness />));
  return { container, document, root, window };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("TrendDateCalendar", () => {
  it("renders adjacent fixed month grids and exposes each day as an accessible toggle", async () => {
    const mounted = await mountCalendar(["2026-07-31"]);
    try {
      expect([...mounted.container.querySelectorAll('[role="grid"]')].map((grid) => grid.getAttribute("aria-label"))).toEqual([
        "2026年7月",
        "2026年8月",
      ]);
      expect(mounted.container.querySelectorAll('[role="grid"] [role="gridcell"]')).toHaveLength(84);
      const day = mounted.container.querySelector('[data-date="2026-07-31"]') as unknown as HTMLButtonElement;
      expect(day.getAttribute("aria-label")).toBe("2026年7月31日，星期五");
      expect(day.getAttribute("aria-pressed")).toBe("true");
      await act(async () => day.dispatchEvent(new mounted.window.Event("click", { bubbles: true })));
      expect((mounted.container.querySelector('[data-date="2026-07-31"]') as Element).getAttribute("aria-pressed")).toBe("false");
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("navigates months and moves keyboard focus across month boundaries before toggling", async () => {
    const mounted = await mountCalendar();
    try {
      const july31 = mounted.container.querySelector('[data-date="2026-07-31"]') as unknown as HTMLButtonElement;
      july31.focus();
      await act(async () => july31.dispatchEvent(keyboardEvent(new mounted.window.Event("keydown", { bubbles: true }) as unknown as Event, "ArrowRight")));
      const august1 = mounted.container.querySelector('[data-date="2026-08-01"]') as unknown as HTMLButtonElement;
      expect(august1.getAttribute("tabindex")).toBe("0");
      await act(async () => august1.dispatchEvent(keyboardEvent(new mounted.window.Event("keydown", { bubbles: true }) as unknown as Event, " ")));
      expect((mounted.container.querySelector('[data-date="2026-08-01"]') as Element).getAttribute("aria-pressed")).toBe("true");

      const august31 = mounted.container.querySelector('[data-date="2026-08-31"]') as unknown as HTMLButtonElement;
      august31.focus();
      await act(async () => august31.dispatchEvent(keyboardEvent(new mounted.window.Event("keydown", { bubbles: true }) as unknown as Event, "ArrowRight")));
      expect([...mounted.container.querySelectorAll('[role="grid"]')].map((grid) => grid.getAttribute("aria-label"))).toEqual([
        "2026年8月",
        "2026年9月",
      ]);
      expect(mounted.container.querySelector('[data-date="2026-09-01"]')?.getAttribute("tabindex")).toBe("0");

      const previous = mounted.container.querySelector('button[aria-label="上一个月"]') as unknown as HTMLButtonElement;
      await act(async () => previous.dispatchEvent(new mounted.window.Event("click", { bubbles: true })));
      expect(mounted.container.querySelector('[role="grid"]')?.getAttribute("aria-label")).toBe("2026年7月");
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("keeps a roving tab stop when an applied selection moves to another month", async () => {
    const { document, window } = parseHTML('<!doctype html><html><body><div id="root"></div></body></html>');
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);

    function Harness() {
      const [dates, setDates] = useState(["2026-07-06"]);
      return <>
        <button type="button" data-action="shift" onClick={() => setDates(["2026-10-06"])}>平移</button>
        <TrendDateCalendar initialMonth={dates[0]} selectedDates={dates} onSelectedDatesChange={setDates} />
      </>;
    }

    try {
      await act(async () => root.render(<Harness />));
      const shift = container.querySelector('[data-action="shift"]') as unknown as HTMLButtonElement;
      await act(async () => shift.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(container.querySelector('[role="grid"]')?.getAttribute("aria-label")).toBe("2026年10月");
      expect(container.querySelectorAll('.trend-calendar-cell button[tabindex="0"]')).toHaveLength(1);
      expect(container.querySelector('.trend-calendar-cell button[tabindex="0"]')?.getAttribute("data-date")).toBe("2026-10-06");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("keeps real focus in the visible month when narrow keyboard navigation crosses a month boundary", async () => {
    const { document, window } = parseHTML('<!doctype html><html><body><div id="root"></div></body></html>');
    const mediaQuery = {
      matches: true,
      media: "(max-width: 760px)",
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    };
    Object.defineProperty(window, "matchMedia", { configurable: true, value: vi.fn(() => mediaQuery) });
    trackActiveElement(document as unknown as Document, window as unknown as { HTMLElement: typeof HTMLElement });
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);

    try {
      await act(async () => root.render(<TrendDateCalendar initialMonth="2026-07-01" selectedDates={[]} onSelectedDatesChange={() => undefined} />));
      const july31 = container.querySelector('[data-date="2026-07-31"]') as unknown as HTMLButtonElement;
      july31.focus();
      await act(async () => july31.dispatchEvent(keyboardEvent(new window.Event("keydown", { bubbles: true }) as unknown as Event, "ArrowRight")));
      expect(container.querySelector('[role="grid"]')?.getAttribute("aria-label")).toBe("2026年8月");
      expect(document.activeElement).toBe(container.querySelector('[data-date="2026-08-01"]'));
      expect(document.activeElement?.closest(".trend-calendar-month-secondary")).toBeNull();

      const august1 = document.activeElement as unknown as HTMLButtonElement;
      await act(async () => august1.dispatchEvent(keyboardEvent(new window.Event("keydown", { bubbles: true }) as unknown as Event, "ArrowLeft")));
      expect(container.querySelector('[role="grid"]')?.getAttribute("aria-label")).toBe("2026年7月");
      expect(document.activeElement).toBe(container.querySelector('[data-date="2026-07-31"]'));
    } finally {
      await act(async () => root.unmount());
    }
  });
});
