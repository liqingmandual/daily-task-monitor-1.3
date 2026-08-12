import { accessSync, constants } from "node:fs";
import { homedir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const executableName = process.platform === "win32" ? "cargo.exe" : "cargo";
const environment = { ...process.env };

function isExecutable(path) {
  try {
    accessSync(path, process.platform === "win32" ? constants.F_OK : constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function cargoOnPath(pathValue = "") {
  return pathValue
    .split(delimiter)
    .filter(Boolean)
    .some((directory) => isExecutable(join(directory, executableName)));
}

if (!cargoOnPath(environment.PATH)) {
  const candidates = [
    { bin: join(homedir(), ".cargo", "bin") },
    {
      bin: resolve(repositoryRoot, "..", "work", "cargo", "bin"),
      cargoHome: resolve(repositoryRoot, "..", "work", "cargo"),
      rustupHome: resolve(repositoryRoot, "..", "work", "rustup"),
    },
  ];
  const candidate = candidates.find(({ bin }) =>
    isExecutable(join(bin, executableName)),
  );

  if (!candidate) {
    console.error(
      [
        "Rust/Cargo was not found.",
        "Install stable Rust from https://rustup.rs, restart the terminal,",
        "and then run `pnpm tauri dev` again.",
      ].join(" "),
    );
    process.exit(127);
  }

  environment.PATH = `${candidate.bin}${delimiter}${environment.PATH ?? ""}`;
  if (candidate.cargoHome && candidate.rustupHome) {
    environment.CARGO_HOME = candidate.cargoHome;
    environment.RUSTUP_HOME = candidate.rustupHome;
  }
}

const tauriCli = resolve(
  repositoryRoot,
  "node_modules",
  "@tauri-apps",
  "cli",
  "tauri.js",
);
const child = spawn(process.execPath, [tauriCli, ...process.argv.slice(2)], {
  cwd: repositoryRoot,
  detached: process.platform !== "win32",
  env: environment,
  stdio: "inherit",
});

function forwardSignal(signal) {
  if (child.exitCode !== null) return;
  if (process.platform === "win32") {
    child.kill(signal);
  } else {
    process.kill(-child.pid, signal);
  }
}

process.once("SIGINT", () => forwardSignal("SIGINT"));
process.once("SIGTERM", () => forwardSignal("SIGTERM"));

const result = await new Promise((resolveResult) => {
  child.once("error", (error) => resolveResult({ error }));
  child.once("exit", (code, signal) => resolveResult({ code, signal }));
});

if (result.error) console.error(result.error.message);
process.exitCode = result.error || result.signal ? 1 : (result.code ?? 1);
