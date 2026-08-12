import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AppIcon } from "./AppIcon";

describe("AppIcon", () => {
  it("renders a cached local icon data URL", () => {
    const markup = renderToStaticMarkup(<AppIcon identity={{
      rawName: "ChatGPT",
      displayName: "ChatGPT",
      executablePath: "C:\\Program Files\\OpenAI\\ChatGPT.exe",
      productName: "ChatGPT",
      iconDataUrl: "data:image/png;base64,iVBORw0KGgo=",
    }} />);

    expect(markup).toContain("data-app-icon=\"native\"");
    expect(markup).toContain("data:image/png;base64,iVBORw0KGgo=");
  });

  it("renders a generic bundled icon when native extraction is unavailable", () => {
    const markup = renderToStaticMarkup(<AppIcon identity={{
      rawName: "UnknownTool",
      displayName: "UnknownTool",
      executablePath: "",
      productName: "",
      iconDataUrl: null,
    }} />);

    expect(markup).toContain("data-app-icon=\"generic\"");
  });

  it("renders Chrome with a centered multicolor bundled icon", () => {
    const markup = renderToStaticMarkup(<AppIcon identity={{
      rawName: "Google Chrome",
      displayName: "Google Chrome",
      executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      productName: "",
      iconDataUrl: null,
    }} />);

    expect(markup).toContain("data-app-icon=\"chrome\"");
    expect(markup).toContain("chrome-color-icon");
    expect(markup).toContain("#ea4335");
    expect(markup).toContain("#fbbc04");
    expect(markup).toContain("#34a853");
    expect(markup).toContain("#4285f4");
  });

  it("renders a bundled Cursor icon when native extraction is unavailable", () => {
    const markup = renderToStaticMarkup(<AppIcon identity={{
      rawName: "Cursor",
      displayName: "Cursor",
      executablePath: "/Applications/Cursor.app/Contents/MacOS/Cursor",
      productName: "",
      iconDataUrl: null,
    }} />);

    expect(markup).toContain("data-app-icon=\"cursor\"");
    expect(markup).not.toContain("data-app-icon=\"generic\"");
  });

});
