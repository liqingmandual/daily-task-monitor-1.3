import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = join(root, "extensions", "src");
const output = join(root, "extensions", "dist");
const targets = ["chromium", "firefox", "safari"];
const protocol = (await readFile(join(source, "protocol.mjs"), "utf8"))
  .replaceAll("export const ", "const ")
  .replaceAll("export function ", "function ")
  .concat("\nglobalThis.DailyTaskWatcherProtocol = { createHeartbeat };\n");
const background = await readFile(join(source, "background.js"), "utf8");

await rm(output, { recursive: true, force: true });
for (const target of targets) {
  const directory = join(output, target);
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, "background.js"), `${protocol}\n${background}`);
  await cp(join(source, "options.html"), join(directory, "options.html"));
  await cp(join(source, "options.js"), join(directory, "options.js"));
  await cp(join(source, "options.css"), join(directory, "options.css"));
  await cp(join(root, "extensions", "manifests", `${target}.json`), join(directory, "manifest.json"));
}
