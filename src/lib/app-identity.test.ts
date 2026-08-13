import { describe, expect, it } from "vitest";
import { resolveDisplayName } from "./app-identity";

describe("resolveDisplayName", () => {
  it("uses Windows product metadata when it is more descriptive", () => {
    expect(resolveDisplayName("Code", "Visual Studio Code", "C:\\Program Files\\Microsoft VS Code\\Code.exe")).toBe("Visual Studio Code");
  });

  it("recognizes Visual Studio Code from its macOS app bundle path", () => {
    expect(resolveDisplayName(
      "Code",
      "",
      "/Applications/Visual Studio Code.app/Contents/MacOS/Electron",
    )).toBe("Visual Studio Code");
  });

  it("keeps standalone Codex helpers named Codex", () => {
    expect(resolveDisplayName("codex", "Codex", "C:\\Users\\me\\bin\\codex.exe")).toBe("Codex");
    expect(resolveDisplayName("Codex", "", "")).toBe("Codex");
  });

  it("prefers ChatGPT.exe for the known OpenAI Codex WindowsApps package with stale Codex metadata", () => {
    expect(resolveDisplayName(
      "ChatGPT",
      "Codex",
      "C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0\\app\\ChatGPT.exe",
    )).toBe("ChatGPT");
  });
});
