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
});
