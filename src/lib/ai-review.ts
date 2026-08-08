import type {
  AiExecutionMode,
  AiReviewAction,
  AiReviewFilter,
  AiReviewKind,
  AiReviewRecord,
  AiReviewResolution,
  AiReviewState,
} from "./desktop";
import { isActivityCategory, isVideoPurpose } from "./activity-composition";

export type AiReviewTab = "pending" | "auto-applied" | "manual-override" | "execution-error";

export interface AiReviewUiFilters {
  kind: AiReviewKind | "all";
  executionMode: AiExecutionMode | "manual" | "all";
  executorId: string;
  model: string;
  dateFrom: string;
  dateTo: string;
  confidenceMin: number;
  confidenceMax: number;
}

export interface ReviewCandidate {
  label: string;
  score: number | null;
  changedJson: string;
}

export const defaultAiReviewFilters: AiReviewUiFilters = {
  kind: "all",
  executionMode: "all",
  executorId: "",
  model: "",
  dateFrom: "",
  dateTo: "",
  confidenceMin: 0,
  confidenceMax: 100,
};

export const reviewStatesByTab: Record<AiReviewTab, AiReviewState[]> = {
  pending: ["pending"],
  "auto-applied": ["auto_applied"],
  "manual-override": ["manual_override", "dismissed", "reverted"],
  "execution-error": ["execution_error"],
};

export interface PendingReviewSubjectIds {
  classification: Set<string>;
  workflowAssignment: Set<string>;
  projectDraft: Set<string>;
}

export type ChangedJsonValidation =
  | { ok: true; changedJson: string }
  | { ok: false; reason: string };

export interface LatestRequestGuard {
  next: () => number;
  isCurrent: (requestId: number) => boolean;
  dispose: () => void;
}

export function pendingReviewSubjectIds(records: AiReviewRecord[]): PendingReviewSubjectIds {
  const classification = new Set<string>();
  const workflowAssignment = new Set<string>();
  const projectDraft = new Set<string>();
  records.forEach((record) => {
    if (record.state !== "pending") return;
    if (record.kind === "classification") classification.add(record.subjectId);
    if (record.kind === "workflow_assignment") workflowAssignment.add(record.subjectId);
    if (record.kind === "project_draft") projectDraft.add(record.subjectId);
  });
  return { classification, workflowAssignment, projectDraft };
}

export function createLatestRequestGuard(): LatestRequestGuard {
  let latestRequestId = 0;
  let disposed = false;
  return {
    next: () => ++latestRequestId,
    isCurrent: (requestId) => !disposed && requestId === latestRequestId,
    dispose: () => {
      disposed = true;
      latestRequestId += 1;
    },
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function validConfidence(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 1;
}

function workflowEvidenceKinds(record: AiReviewRecord): Set<"activity" | "browser"> {
  const proposed = parseReviewJson(record.proposedJson);
  const candidates = Array.isArray(proposed.candidates) ? proposed.candidates : [];
  const values: unknown[] = [proposed, ...candidates.map((candidate) => {
    if (!isRecord(candidate)) return candidate;
    return candidate.value ?? candidate;
  })];
  const kinds = new Set<"activity" | "browser">();

  values.forEach((value) => {
    if (!isRecord(value) || value.evidenceId !== record.subjectId) return;
    if (value.evidenceKind === "activity" || value.evidenceKind === "browser") kinds.add(value.evidenceKind);
  });
  return kinds;
}

export function validateChangedJson(record: AiReviewRecord, changedJson: string): ChangedJsonValidation {
  let parsed: unknown;
  try {
    parsed = JSON.parse(changedJson);
  } catch {
    return { ok: false, reason: "替代 JSON 不是有效对象" };
  }
  if (!isRecord(parsed)) return { ok: false, reason: "替代 JSON 不是有效对象" };

  if (record.kind === "classification") {
    if (!isActivityCategory(parsed.category)) {
      return { ok: false, reason: "活动分类 category 不合法" };
    }
    if (!isVideoPurpose(parsed.videoPurpose)) {
      return { ok: false, reason: "视频用途 videoPurpose 不合法" };
    }
    if (!validConfidence(parsed.confidence)) {
      return { ok: false, reason: "置信度 confidence 必须是 0 到 1 的有限数值" };
    }
    if (typeof parsed.reason !== "string") return { ok: false, reason: "reason 必须是字符串" };
    if (typeof parsed.modelVersion !== "string") return { ok: false, reason: "modelVersion 必须是字符串" };
    return { ok: true, changedJson };
  }

  if (record.kind === "project_draft") {
    const original = parseReviewJson(record.proposedJson);
    if (parsed.targetProjectId !== original.targetProjectId) {
      return { ok: false, reason: "目标项目不能在草稿编辑阶段变更" };
    }
    if (typeof parsed.name !== "string" || !parsed.name.trim() || parsed.name.length > 80) {
      return { ok: false, reason: "项目名称必须是 1–80 个字符" };
    }
    if (typeof parsed.description !== "string" || parsed.description.length > 500) {
      return { ok: false, reason: "项目说明不能超过 500 个字符" };
    }
    if (!validConfidence(parsed.confidence) || typeof parsed.reasonCode !== "string") {
      return { ok: false, reason: "草稿置信度或原因代码无效" };
    }
    if (!Array.isArray(parsed.tasks) || parsed.tasks.length < 1 || parsed.tasks.length > 3) {
      return { ok: false, reason: "项目草稿必须保留 1–3 个任务" };
    }
    const keys = new Set<string>();
    const clusters = new Set<string>();
    for (const task of parsed.tasks) {
      if (!isRecord(task)
        || typeof task.key !== "string"
        || !task.key.trim()
        || keys.has(task.key)
        || typeof task.title !== "string"
        || !task.title.trim()
        || task.title.length > 80
        || typeof task.expectedOutput !== "string"
        || task.expectedOutput.length > 300
        || !Array.isArray(task.clusterIds)
        || task.clusterIds.length === 0
        || task.clusterIds.some((value) => typeof value !== "string" || clusters.has(value))) {
        return { ok: false, reason: "任务标题、预期产出或候选簇分配无效" };
      }
      keys.add(task.key);
      task.clusterIds.forEach((value) => clusters.add(value as string));
    }
    const originalClusters = new Set(
      (Array.isArray(original.tasks) ? original.tasks : [])
        .flatMap((task) => isRecord(task) && Array.isArray(task.clusterIds) ? task.clusterIds : [])
        .filter((value): value is string => typeof value === "string"),
    );
    if (clusters.size !== originalClusters.size
      || [...clusters].some((cluster) => !originalClusters.has(cluster))) {
      return { ok: false, reason: "候选簇只能在现有任务之间调整，不能新增或遗漏" };
    }
    return { ok: true, changedJson };
  }

  if (parsed.evidenceKind !== "activity" && parsed.evidenceKind !== "browser") {
    return { ok: false, reason: "evidenceKind 必须是 activity 或 browser" };
  }
  if (typeof parsed.evidenceId !== "string" || parsed.evidenceId !== record.subjectId) {
    return { ok: false, reason: "evidenceId 必须匹配当前审核对象" };
  }
  if (!workflowEvidenceKinds(record).has(parsed.evidenceKind)) {
    return { ok: false, reason: "evidenceKind 必须匹配当前审核对象或候选" };
  }
  if (parsed.taskId !== null && typeof parsed.taskId !== "string") {
    return { ok: false, reason: "taskId 必须是字符串或 null" };
  }
  if (!validConfidence(parsed.confidence)) {
    return { ok: false, reason: "置信度 confidence 必须是 0 到 1 的有限数值" };
  }
  if (typeof parsed.reason !== "string") return { ok: false, reason: "reason 必须是字符串" };
  if (typeof parsed.createdAtMs !== "number" || !Number.isFinite(parsed.createdAtMs)) {
    return { ok: false, reason: "createdAtMs 必须是有限数值" };
  }
  return { ok: true, changedJson };
}

function startOfDate(date: string): number | null {
  if (!date) return null;
  const value = new Date(`${date}T00:00:00`).getTime();
  return Number.isFinite(value) ? value : null;
}

function endOfDate(date: string): number | null {
  const start = startOfDate(date);
  return start === null ? null : start + 86_400_000 - 1;
}

export function buildAiReviewFilter(tab: AiReviewTab, filters: AiReviewUiFilters, subjectId?: string | null): AiReviewFilter {
  return {
    states: reviewStatesByTab[tab],
    kinds: filters.kind === "all" ? [] : [filters.kind],
    subjectId: subjectId || null,
    executionMode: filters.executionMode === "api-key" || filters.executionMode === "codex" ? filters.executionMode : null,
    executorId: filters.executorId.trim() || null,
    model: filters.model.trim() || null,
    createdFromMs: startOfDate(filters.dateFrom),
    createdToMs: endOfDate(filters.dateTo),
    minConfidence: filters.confidenceMin <= 0 ? null : filters.confidenceMin / 100,
    maxConfidence: filters.confidenceMax >= 100 ? null : filters.confidenceMax / 100,
  };
}

export function applyAiReviewFilters(records: AiReviewRecord[], filters: AiReviewUiFilters, subjectId?: string | null): AiReviewRecord[] {
  const from = startOfDate(filters.dateFrom);
  const to = endOfDate(filters.dateTo);
  const executor = filters.executorId.trim().toLocaleLowerCase();
  const model = filters.model.trim().toLocaleLowerCase();
  return records.filter((record) => {
    if (subjectId && record.subjectId !== subjectId) return false;
    if (filters.kind !== "all" && record.kind !== filters.kind) return false;
    if (filters.executionMode === "manual" && record.execution.executionMode !== null) return false;
    if (filters.executionMode !== "all" && filters.executionMode !== "manual" && record.execution.executionMode !== filters.executionMode) return false;
    if (executor && !(record.execution.executorId ?? "").toLocaleLowerCase().includes(executor)) return false;
    if (model && !(record.execution.model ?? "").toLocaleLowerCase().includes(model)) return false;
    if (from !== null && record.createdAtMs < from) return false;
    if (to !== null && record.createdAtMs > to) return false;
    const confidencePercent = (record.confidence ?? 0) * 100;
    if (record.confidence !== null && confidencePercent < filters.confidenceMin) return false;
    if (record.confidence !== null && confidencePercent > filters.confidenceMax) return false;
    if (record.confidence === null && (filters.confidenceMin > 0 || filters.confidenceMax < 100)) return false;
    return true;
  });
}

export function parseReviewJson(value: string): Record<string, unknown> {
  if (!value) return {};
  try {
    const parsed: unknown = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? parsed as Record<string, unknown>
      : { value: parsed };
  } catch {
    return { raw: value };
  }
}

function candidateLabel(value: unknown, index: number): string {
  if (value && typeof value === "object") {
    const item = value as Record<string, unknown>;
    const label = item.label ?? item.category ?? item.taskTitle ?? item.taskId ?? item.value;
    if (typeof label === "string" && label.trim()) return label;
  }
  return `候选 ${index + 1}`;
}

export function reviewCandidates(record: AiReviewRecord): ReviewCandidate[] {
  const proposed = parseReviewJson(record.proposedJson);
  const candidates = Array.isArray(proposed.candidates) ? proposed.candidates : [];
  if (candidates.length) {
    return candidates.flatMap((candidate, index) => {
      if (!candidate || typeof candidate !== "object") return [];
      const item = candidate as Record<string, unknown>;
      const value = item.value ?? item;
      const score = typeof item.score === "number" ? item.score : typeof item.confidence === "number" ? item.confidence : null;
      return [{
        label: typeof item.label === "string" ? item.label : candidateLabel(value, index),
        score,
        changedJson: JSON.stringify(value),
      }];
    });
  }
  if (!record.proposedJson) return [];
  return [{
    label: candidateLabel(proposed, 0),
    score: record.confidence,
    changedJson: record.proposedJson,
  }];
}

export function reviewSourceLabel(record: AiReviewRecord): string {
  if (record.execution.executionMode === null) return "人工修改";
  if (record.execution.executionMode === "codex") {
    return record.execution.model ? `本机 Codex · ${record.execution.model}` : "本机 Codex";
  }
  const provider = record.execution.executorId || "API Key";
  return record.execution.model ? `${provider} · ${record.execution.model}` : provider;
}

export function sanitizeReviewDiagnostic(value: string): string {
  let result = value.replace(/[\r\n]+/g, " ");
  result = result.replace(/\b(?:https?|wss?):\/\/[^\s"'<>\])}]+/gi, "[redacted-url]");
  result = result.replace(/\bBearer\s+[^\s,;]+/gi, "Bearer [redacted]");
  result = result.replace(/\b(authorization|x-api-key|api[-_]?key|access_token|refresh_token|token|password|secret)\s*[:=]\s*[^\s,;]+/gi, "$1: [redacted]");
  result = result.replace(/stderr:\s*.*/i, "stderr: [redacted]");
  return result.replace(/\s+/g, " ").trim().slice(0, 1_000);
}

export function validateBatchAccept(records: AiReviewRecord[]): { ok: true; kind: AiReviewKind } | { ok: false; reason: string } {
  if (!records.length) return { ok: false, reason: "请先选择审核记录" };
  const kind = records[0].kind;
  if (records.some((record) => record.kind !== kind)) {
    return { ok: false, reason: "批量接受只能包含同一种审核类型" };
  }
  if (records.some((record) => record.state !== "pending")) {
    return { ok: false, reason: "批量接受只能处理待审核记录" };
  }
  return { ok: true, kind };
}

export function buildAiReviewResolution(
  records: AiReviewRecord[],
  action: AiReviewAction,
  changedJson?: string,
  resolvedAtMs = Date.now(),
): AiReviewResolution {
  return {
    reviewIds: records.map((record) => record.id),
    action,
    changedJson: changedJson ?? null,
    evidenceHashes: Object.fromEntries(records.map((record) => [record.id, record.evidenceHash])),
    resolvedAtMs,
  };
}
