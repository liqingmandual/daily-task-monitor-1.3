import { useState } from "react";
import { AppWindow } from "lucide-react";
import { FaEdge } from "react-icons/fa";
import { SiGooglechrome, SiObsidian, SiQq, SiWechat } from "react-icons/si";
import { VscCode } from "react-icons/vsc";
import type { AppIdentity } from "../lib/app-identity";

function BundledAppIcon({ identity }: { identity: AppIdentity }) {
  const normalized = `${identity.displayName} ${identity.rawName}`.toLowerCase();
  const props = { size: 18, "aria-hidden": true } as const;
  if (normalized.includes("chrome")) return <span className="app-logo chrome" data-app-icon="chrome"><SiGooglechrome {...props} /></span>;
  if (normalized.includes("edge")) return <span className="app-logo edge" data-app-icon="edge"><FaEdge {...props} /></span>;
  if (normalized.includes("visual studio code") || normalized.includes("vscode")) return <span className="app-logo code" data-app-icon="code"><VscCode {...props} /></span>;
  if (normalized.includes("obsidian")) return <span className="app-logo obsidian" data-app-icon="obsidian"><SiObsidian {...props} /></span>;
  if (normalized.includes("wechat") || normalized.includes("weixin")) return <span className="app-logo wechat" data-app-icon="wechat"><SiWechat {...props} /></span>;
  if (normalized.trim() === "qq qq" || normalized.includes("tencentqq")) return <span className="app-logo qq" data-app-icon="qq"><SiQq {...props} /></span>;
  return <span className="app-logo fallback" data-app-icon="generic"><AppWindow {...props} /></span>;
}

export function AppIcon({ identity }: { identity: AppIdentity }) {
  const [nativeFailed, setNativeFailed] = useState(false);
  if (identity.iconDataUrl && !nativeFailed) {
    return <img
      className="app-logo native"
      data-app-icon="native"
      src={identity.iconDataUrl}
      alt=""
      style={{ objectFit: "contain", padding: 2 }}
      onError={() => setNativeFailed(true)}
    />;
  }
  return <BundledAppIcon identity={identity} />;
}
