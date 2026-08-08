const fs = require("fs");
const http = require("http");
const path = require("path");
const { spawn } = require("child_process");

const projectRoot = path.resolve(process.argv[2] || path.join(__dirname, ".."));
const port = Number(process.argv[3] || 8765);
const url = `http://localhost:${port}/`;
const logPath = path.join(projectRoot, "data", "launcher.log");

function log(message) {
  try {
    fs.mkdirSync(path.dirname(logPath), { recursive: true });
    fs.appendFileSync(logPath, `[${new Date().toISOString()}] ${message}\n`, "utf8");
  } catch {
  }
}

function dashboardIsReady() {
  return new Promise((resolve) => {
    const req = http.get(
      { host: "127.0.0.1", port, path: "/", timeout: 1600 },
      (res) => {
        res.resume();
        resolve(res.statusCode === 200);
      },
    );

    req.on("timeout", () => {
      req.destroy();
      resolve(false);
    });
    req.on("error", () => resolve(false));
  });
}

function startDashboard() {
  const child = spawn(process.execPath, ["server.js", String(port), "."], {
    cwd: projectRoot,
    detached: true,
    stdio: "ignore",
    windowsHide: true,
  });
  child.unref();
  log(`Started dashboard server pid=${child.pid} project=${projectRoot}`);
}

function chromeCandidates() {
  const candidates = [];
  const programFiles = process.env.ProgramFiles;
  const programFilesX86 = process.env["ProgramFiles(x86)"];
  const localAppData = process.env.LOCALAPPDATA;

  if (programFiles) candidates.push(path.join(programFiles, "Google", "Chrome", "Application", "chrome.exe"));
  if (programFilesX86) candidates.push(path.join(programFilesX86, "Google", "Chrome", "Application", "chrome.exe"));
  if (localAppData) candidates.push(path.join(localAppData, "Google", "Chrome", "Application", "chrome.exe"));
  return candidates;
}

function openDashboard() {
  const chrome = chromeCandidates().find((candidate) => fs.existsSync(candidate));
  if (chrome) {
    spawn(chrome, ["--new-window", url], {
      detached: true,
      stdio: "ignore",
      windowsHide: false,
    }).unref();
    log(`Opened Chrome ${url}`);
    return;
  }

  spawn("cmd.exe", ["/c", "start", "", url], {
    detached: true,
    stdio: "ignore",
    windowsHide: true,
  }).unref();
  log(`Opened default browser ${url}`);
}

async function main() {
  log("Launcher invoked.");
  if (!(await dashboardIsReady())) {
    startDashboard();
    for (let attempt = 0; attempt < 35; attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 300));
      if (await dashboardIsReady()) break;
    }
  }

  if (await dashboardIsReady()) {
    openDashboard();
    return;
  }

  log("Dashboard did not become ready.");
  process.exitCode = 1;
}

main().catch((error) => {
  log(`Launcher failed: ${error && error.stack ? error.stack : error}`);
  process.exitCode = 1;
});
