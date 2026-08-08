import { describe, expect, it } from "vitest";
import type { AiReviewRecord } from "./desktop";
import {
  applyAiReviewFilters,
  buildAiReviewResolution,
  buildAiReviewFilter,
  createLatestRequestGuard,
  pendingReviewSubjectIds,
  parseReviewJson,
  reviewCandidates,
  reviewSourceLabel,
  sanitizeReviewDiagnostic,
  validateBatchAccept,
  validateChangedJson,
  defaultAiReviewFilters,
  reviewStatesByTab,
  type AiReviewUiFilters,
} from "./ai-review";

function review(overrides: Partial<AiReviewRecord> = {}): AiReviewRecord {
  return {
    id: "review-1",
    kind: "classification",
    state: "pending",
    subjectId: "segment-1",
    beforeJson: JSON.stringify({ category: "pending" }),
    proposedJson: JSON.stringify({
      category: "research",
      videoPurpose: "unknown",
      confidence: 0.78,
      reason: "bounded AI result",
      modelVersion: "review-test-model",
      candidates: [
        { label: "研究", score: 0.78, value: { category: "research", videoPurpose: "unknown", confidence: 0.78, reason: "候选一", modelVersion: "review-test-model" } },
        { label: "工作", score: 0.19, value: { category: "work", confidence: 0.19 } },
      ],
    }),
    appliedJson: null,
    confidence: 0.78,
    evidenceSummary: "Code: review implementation",
    evidenceHash: "hash-1",
    execution: {
      executionMode: "api-key",
      executorId: "openai",
      model: "gpt-4.1-mini",
      evidenceHash: "hash-1",
      generation: 1,
      createdAtMs: new Date("2026-07-13T09:00:00+08:00").getTime(),
      startedAtMs: null,
      finishedAtMs: null,
      durationMs: null,
      exitCode: null,
      errorKind: null,
      diagnostic: "",
    },
    createdAtMs: new Date("2026-07-13T09:00:00+08:00").getTime(),
    resolvedAtMs: null,
    ...overrides,
  };
}

const emptyFilters: AiReviewUiFilters = {
  kind: "all",
  executionMode: "all",
  executorId: "",
  model: "",
  dateFrom: "",
  dateTo: "",
  confidenceMin: 0,
  confidenceMax: 100,
};

describe("AI review helpers", () => {
  it("combines kind, execution, provider, model, date, confidence, and subject filters", () => {
    const matching = review();
    const records = [
      matching,
      review({ id: "wrong-kind", kind: "workflow_assignment" }),
      review({ id: "wrong-mode", execution: { ...matching.execution, executionMode: "codex" } }),
      review({ id: "wrong-confidence", confidence: 0.41 }),
      review({ id: "wrong-subject", subjectId: "segment-2" }),
    ];

    const filtered = applyAiReviewFilters(records, {
      ...emptyFilters,
      kind: "classification",
      executionMode: "api-key",
      executorId: "OPEN",
      model: "4.1-mini",
      dateFrom: "2026-07-13",
      dateTo: "2026-07-13",
      confidenceMin: 70,
      confidenceMax: 90,
    }, "segment-1");

    expect(filtered.map((item) => item.id)).toEqual(["review-1"]);
  });

  it("labels nullable execution records as manual changes instead of API key", () => {
    const manual = review({
      state: "manual_override",
      execution: {
        ...review().execution,
        executionMode: null,
        executorId: null,
        model: null,
      },
    });

    expect(reviewSourceLabel(manual)).toBe("人工修改");
  });

  it("parses candidate values and preserves their scores for low-confidence review", () => {
    const candidates = reviewCandidates(review());

    expect(candidates.map((candidate) => [candidate.label, candidate.score])).toEqual([
      ["研究", 0.78],
      ["工作", 0.19],
    ]);
    expect(JSON.parse(candidates[1].changedJson)).toMatchObject({ category: "work" });
  });

  it("returns a safe fallback for malformed JSON and sanitizes defensive diagnostics", () => {
    expect(parseReviewJson("not-json")).toEqual({ raw: "not-json" });
    const diagnostic = sanitizeReviewDiagnostic(
      "Provider failed\nAuthorization: Bearer secret-token https://api.example.test/v1 stderr: private output",
    );

    expect(diagnostic).not.toContain("secret-token");
    expect(diagnostic).not.toContain("api.example.test");
    expect(diagnostic).not.toContain("private output");
    expect(diagnostic).toContain("[redacted]");
  });

  it("builds evidence-bound resolutions and rejects mixed-kind batch accept", () => {
    const first = review();
    const second = review({ id: "review-2", subjectId: "segment-2", evidenceHash: "hash-2" });
    const mixed = review({ id: "review-3", kind: "workflow_assignment" });

    expect(validateBatchAccept([first, second])).toEqual({ ok: true, kind: "classification" });
    expect(validateBatchAccept([first, mixed])).toEqual({ ok: false, reason: "批量接受只能包含同一种审核类型" });
    expect(buildAiReviewResolution([first, second], "accept", undefined, 12_345)).toEqual({
      reviewIds: ["review-1", "review-2"],
      action: "accept",
      changedJson: null,
      evidenceHashes: { "review-1": "hash-1", "review-2": "hash-2" },
      resolvedAtMs: 12_345,
    });
  });

  it("maps dismissed and reverted records into the existing manual history tab", () => {
    expect(reviewStatesByTab["manual-override"]).toEqual(["manual_override", "dismissed", "reverted"]);
    expect(buildAiReviewFilter("manual-override", defaultAiReviewFilters).states).toEqual([
      "manual_override",
      "dismissed",
      "reverted",
    ]);
  });

  it("derives marker subjects only from real pending review records", () => {
    const subjects = pendingReviewSubjectIds([
      review({ subjectId: "segment-pending" }),
      review({ id: "workflow", kind: "workflow_assignment", subjectId: "visit-pending" }),
      review({ id: "resolved", state: "manual_override", subjectId: "segment-resolved" }),
    ]);

    expect([...subjects.classification]).toEqual(["segment-pending"]);
    expect([...subjects.workflowAssignment]).toEqual(["visit-pending"]);
  });

  it("invalidates stale and disposed asynchronous requests", () => {
    const guard = createLatestRequestGuard();
    const first = guard.next();
    const second = guard.next();

    expect(guard.isCurrent(first)).toBe(false);
    expect(guard.isCurrent(second)).toBe(true);
    guard.dispose();
    expect(guard.isCurrent(second)).toBe(false);
  });

  it("validates classification changed JSON against the backend shape", () => {
    const valid = JSON.stringify({
      category: "research",
      videoPurpose: "unknown",
      confidence: 0.72,
      reason: "用户选择替代候选",
      modelVersion: "review-test-model",
    });

    expect(validateChangedJson(review(), valid)).toEqual({ ok: true, changedJson: valid });
    expect(validateChangedJson(review(), JSON.stringify({
      category: "work",
      videoPurpose: "unknown",
      confidence: 1.2,
      reason: 42,
      modelVersion: "",
    }))).toEqual(expect.objectContaining({ ok: false }));
    expect(validateChangedJson(review(), "not-json")).toEqual({ ok: false, reason: "替代 JSON 不是有效对象" });
  });

  it("validates workflow changed JSON against the current review subject", () => {
    const workflow = review({
      kind: "workflow_assignment",
      subjectId: "visit-1",
      proposedJson: JSON.stringify({
        evidenceKind: "browser",
        evidenceId: "visit-1",
        taskId: "task-1",
        confidence: 0.66,
        reason: "页面与任务匹配",
        createdAtMs: 12_345,
      }),
    });
    const valid = JSON.stringify({
      evidenceKind: "browser",
      evidenceId: "visit-1",
      taskId: "task-1",
      confidence: 0.66,
      reason: "页面与任务匹配",
      createdAtMs: 12_345,
    });

    expect(validateChangedJson(workflow, valid)).toEqual({ ok: true, changedJson: valid });
    expect(validateChangedJson(workflow, JSON.stringify({
      evidenceKind: "browser",
      evidenceId: "visit-2",
      taskId: null,
      confidence: Number.NaN,
      reason: "stale subject",
      createdAtMs: 12_345,
    }))).toEqual(expect.objectContaining({ ok: false, reason: expect.stringContaining("当前审核对象") }));
    expect(validateChangedJson(workflow, JSON.stringify({
      evidenceKind: "activity",
      evidenceId: "visit-1",
      taskId: null,
      confidence: 0.5,
      reason: "错误证据类型",
      createdAtMs: 12_345,
    }))).toEqual(expect.objectContaining({ ok: false, reason: expect.stringContaining("evidenceKind") }));
  });

  it("allows project draft edits while preserving every bounded candidate cluster", () => {
    const proposal = {
      targetProjectId: null,
      name: "学习雅思",
      description: "准备雅思考试",
      tasks: [
        { key: "research", title: "检索雅思资料", expectedOutput: "整理资料", clusterIds: ["cluster-browser"] },
        { key: "words", title: "背诵雅思词汇", expectedOutput: "复习词汇", clusterIds: ["cluster-pdf"] },
      ],
      confidence: 0.93,
      reasonCode: "shared_goal",
    };
    const projectDraft = review({
      kind: "project_draft",
      subjectId: "workflow-batch-ielts",
      proposedJson: JSON.stringify(proposal),
    });
    const merged = JSON.stringify({
      ...proposal,
      name: "雅思学习",
      tasks: [{
        ...proposal.tasks[0],
        title: "整理资料并复习词汇",
        clusterIds: ["cluster-browser", "cluster-pdf"],
      }],
    });
    expect(validateChangedJson(projectDraft, merged)).toEqual({ ok: true, changedJson: merged });
    expect(validateChangedJson(projectDraft, JSON.stringify({
      ...proposal,
      tasks: [{ ...proposal.tasks[0], clusterIds: ["cluster-browser"] }],
    }))).toEqual(expect.objectContaining({ ok: false, reason: expect.stringContaining("候选簇") }));
  });
});
