export interface AppIdentity {
  rawName: string;
  displayName: string;
  executablePath: string;
  productName: string;
  iconDataUrl: string | null;
}

export interface AppIdentitySource {
  app: string;
  appPath?: string;
}

const BUILTIN_ALIASES: Record<string, string> = {
  msedge: "Microsoft Edge",
  explorer: "File Explorer",
};

function isPackagedChatGpt(rawName: string, executablePath: string): boolean {
  const path = executablePath.replaceAll("/", "\\").toLowerCase();
  return rawName.trim().toLowerCase() === "chatgpt"
    && path.includes("\\windowsapps\\openai.codex_")
    && path.endsWith("\\app\\chatgpt.exe");
}

export function resolveDisplayName(rawName: string, productName = "", executablePath = ""): string {
  const raw = rawName.trim();
  if (isPackagedChatGpt(raw, executablePath)) return "ChatGPT";
  const product = productName.trim();
  if (product) return product;
  return BUILTIN_ALIASES[raw.toLowerCase()] ?? raw;
}

export function appIdentityKey(rawName: string, executablePath = ""): string {
  return `${rawName.trim().toLowerCase()}\n${executablePath.trim().replaceAll("/", "\\").toLowerCase()}`;
}

export function fallbackAppIdentity(rawName: string, executablePath = ""): AppIdentity {
  return {
    rawName,
    displayName: resolveDisplayName(rawName, "", executablePath),
    executablePath,
    productName: "",
    iconDataUrl: null,
  };
}

export function identityFor(
  identities: ReadonlyMap<string, AppIdentity>,
  rawName: string,
  executablePath = "",
): AppIdentity {
  const exact = identities.get(appIdentityKey(rawName, executablePath));
  if (exact) return exact;
  for (const identity of identities.values()) {
    if (identity.rawName.toLowerCase() === rawName.toLowerCase()) return identity;
  }
  return fallbackAppIdentity(rawName, executablePath);
}
