export const PROTOCOL_VERSION = 1;

export function createHeartbeat(input) {
  const capturedAtMs = Number(input.capturedAtMs ?? Date.now());
  const url = String(input.url ?? "");
  if (url && !/^https?:\/\//i.test(url)) {
    throw new Error("Only HTTP(S) pages can be reported");
  }
  return {
    protocolVersion: PROTOCOL_VERSION,
    sourceId: requireIdentifier(input.sourceId, "sourceId"),
    browser: requireIdentifier(input.browser, "browser"),
    profile: String(input.profile ?? "default").slice(0, 128),
    tabId: requireIdentifier(String(input.tabId), "tabId"),
    capturedAtMs,
    url,
    title: String(input.title ?? "").slice(0, 240),
    active: Boolean(input.active),
    private: Boolean(input.private),
  };
}

function requireIdentifier(value, label) {
  const normalized = String(value ?? "").trim();
  if (!normalized || !/^[a-zA-Z0-9_.-]+$/.test(normalized)) {
    throw new Error(`${label} contains unsupported characters`);
  }
  return normalized;
}
