const extensionApi = globalThis.browser ?? globalThis.chrome;
const DEFAULT_ENDPOINT = "http://127.0.0.1:27123/v1/heartbeat";
let lastTab = null;

function browserName() {
  const agent = navigator.userAgent.toLowerCase();
  if (agent.includes("firefox")) return "firefox";
  if (agent.includes("edg/")) return "edge";
  if (agent.includes("safari") && !agent.includes("chrome")) return "safari";
  return "chromium";
}

async function configuration() {
  const saved = await extensionApi.storage.local.get(["endpoint", "token", "sourceId"]);
  let sourceId = saved.sourceId;
  if (!sourceId) {
    sourceId = `${browserName()}-${crypto.randomUUID()}`;
    await extensionApi.storage.local.set({ sourceId });
  }
  return { endpoint: saved.endpoint || DEFAULT_ENDPOINT, token: saved.token || "", sourceId };
}

async function sendHeartbeat(tab, active = true) {
  const config = await configuration();
  if (!config.token) return;
  const candidate = tab || lastTab;
  if (!candidate) return;
  if (tab) lastTab = tab;
  const url = candidate.url || "";
  const reportable = /^https?:\/\//i.test(url);
  const heartbeat = DailyTaskWatcherProtocol.createHeartbeat({
    sourceId: config.sourceId,
    browser: browserName(),
    profile: "default",
    tabId: candidate.id,
    capturedAtMs: Date.now(),
    url: reportable ? url : "",
    title: reportable ? candidate.title : "",
    active: active && reportable,
    private: Boolean(candidate.incognito),
  });
  try {
    await fetch(config.endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Daily-Task-Monitor-Token": config.token,
      },
      body: JSON.stringify(heartbeat),
    });
  } catch (_) {
    // The desktop app can be closed; the next event or alarm retries locally.
  }
}

async function reportActiveTab() {
  const windows = await extensionApi.windows.getLastFocused();
  const tabs = await extensionApi.tabs.query({ active: true, lastFocusedWindow: true });
  await sendHeartbeat(tabs[0], Boolean(windows?.focused));
}

extensionApi.runtime.onInstalled.addListener(() => {
  extensionApi.alarms.create("daily-task-monitor-heartbeat", { periodInMinutes: 0.5 });
  void reportActiveTab();
});
extensionApi.runtime.onStartup.addListener(() => void reportActiveTab());
extensionApi.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "daily-task-monitor-heartbeat") void reportActiveTab();
});
extensionApi.tabs.onActivated.addListener(() => void reportActiveTab());
extensionApi.tabs.onUpdated.addListener((_tabId, changeInfo, tab) => {
  if (tab.active && (changeInfo.url || changeInfo.status === "complete")) void sendHeartbeat(tab);
});
extensionApi.windows.onFocusChanged.addListener((windowId) => {
  if (windowId === extensionApi.windows.WINDOW_ID_NONE) void sendHeartbeat(lastTab, false);
  else void reportActiveTab();
});
