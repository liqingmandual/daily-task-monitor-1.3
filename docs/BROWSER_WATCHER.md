# Browser watcher protocol

## Purpose and trust boundary

Browser history proves that a URL was visited; it does not prove foreground
duration. Protocol v1 adds measured active-tab slices. An extension sends a
heartbeat every 30 seconds and on tab/window changes to the desktop listener at
`http://127.0.0.1:27123/v1/heartbeat`.

The listener binds only to loopback and requires the random connection token
shown under **Settings → Collection health diagnostics**. The token is local
configuration, not an account credential. Private/incognito tabs, browser
internal pages, file URLs, credentials, fragments, and sensitive query values
are not stored. Domain exclusions apply before persistence.

Every measured slice has provenance `watcher-heartbeat-v1`. Slices are produced
only by two consecutive heartbeats for the same source, tab, and redacted URL,
with a gap no longer than 45 seconds. A missing heartbeat therefore creates no
invented duration.

## Protocol v1

Send `POST /v1/heartbeat`, `Content-Type: application/json`, and header
`X-Daily-Task-Monitor-Token`. The JSON body is:

```json
{
  "protocolVersion": 1,
  "sourceId": "chromium-device-uuid",
  "browser": "chromium",
  "profile": "default",
  "tabId": "123",
  "capturedAtMs": 1786521600000,
  "url": "https://example.com/path",
  "title": "Example",
  "active": true,
  "private": false
}
```

`202` means accepted, `401` means the local token is wrong, and `422` means the
heartbeat violates privacy, clock, or protocol validation.

## Build and local installation

```bash
pnpm run build:extensions
```

- Chromium/Edge/Brave/Arc: open the extensions page, enable developer mode,
  choose **Load unpacked**, and select `extensions/dist/chromium`.
- Firefox: open `about:debugging#/runtime/this-firefox`, choose **Load Temporary
  Add-on**, and select `extensions/dist/firefox/manifest.json`.
- Safari: the generated `extensions/dist/safari` is WebExtension source. Convert
  it with Xcode's `safari-web-extension-converter`, run the generated host, and
  enable the extension in Safari. Signing/distribution remains outside P0.

Open the extension options, copy the endpoint and token from the desktop health
panel, save, then browse between two public pages. Within two heartbeats the
health panel should show a source and measured slices.
