import { describe, expect, it } from "vitest";
import { createHeartbeat } from "./protocol.mjs";

describe("browser watcher protocol", () => {
  it("creates a versioned heartbeat with bounded page text", () => {
    const heartbeat = createHeartbeat({
      sourceId: "chrome-default",
      browser: "chrome",
      tabId: 12,
      capturedAtMs: 1_000,
      url: "https://example.test/path?token=secret",
      title: "x".repeat(300),
      active: true,
    });

    expect(heartbeat.protocolVersion).toBe(1);
    expect(heartbeat.tabId).toBe("12");
    expect(heartbeat.title).toHaveLength(240);
  });

  it("rejects internal pages and unsafe identifiers", () => {
    expect(() => createHeartbeat({ sourceId: "bad source", browser: "chrome", tabId: 1 }))
      .toThrow("sourceId");
    expect(() => createHeartbeat({ sourceId: "source", browser: "chrome", tabId: 1, url: "chrome://settings" }))
      .toThrow("HTTP(S)");
  });
});
