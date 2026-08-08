import { lazy, StrictMode, Suspense } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";

const demo = new URLSearchParams(window.location.search).get("demo");
const InteractionDemo = lazy(() => import("./InteractionDemo"));
const KnowledgeGraphPage = lazy(() => import("./KnowledgeGraphPage"));

function PreviewRouter() {
  if (demo === "interaction-v2") {
    return <Suspense fallback={<div className="demo-loading">正在加载交互预览...</div>}><InteractionDemo /></Suspense>;
  }
  if (demo === "knowledge-space") {
    return <Suspense fallback={<div className="demo-loading">正在构建知识空间...</div>}><KnowledgeGraphPage onBack={() => { window.location.href = "/"; }} onOpenTimeline={() => { window.location.href = "/"; }} /></Suspense>;
  }
  return <App />;
}

createRoot(document.getElementById("root")!).render(<StrictMode><PreviewRouter /></StrictMode>);
