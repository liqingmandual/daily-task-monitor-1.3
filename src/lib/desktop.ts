import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  appIdentityKey,
  fallbackAppIdentity,
  resolveDisplayName,
  type AppIdentity,
  type AppIdentitySource,
} from "./app-identity";
import { type ActivityCategory, type InactivityReason, type Segment, type VideoPurpose } from "./metrics";
import type { DailyAnalysisResult } from "./daily-analysis";
import type {
  ActivityCompositions,
  ActivityScope,
  MeaningfulReason,
} from "./activity-composition";

export const ANALYSIS_CHANGED_EVENT = "analysis-changed";

export interface AnalysisChangedEvent {
  page: "daily" | "trends";
  scope: ActivityScope;
  date?: string;
  rangeStart?: string;
  rangeEnd?: string;
  evidenceHash: string;
}

export type ScopedDailyAnalysisResult = DailyAnalysisResult & {
  activityScope: ActivityScope;
};

export interface BackendSegment {
  id: string;
  startedAtMs: number;
  endedAtMs: number;
  app: string;
  appPath?: string;
  title: string;
  category: ActivityCategory;
  videoPurpose: VideoPurpose;
  confidence: number;
  source: "manual" | "idle" | "rule" | "behavior" | "ai" | "pending";
  reason: string;
  modelVersion: string;
  needsReview: boolean;
  inactivityReason?: InactivityReason | null;
}

export interface DashboardSnapshot {
  timeline: BackendSegment[];
  totals: {
    monitoredSeconds: number;
    activeSeconds: number;
    idleSeconds: number;
    learningSeconds: number;
    categorySeconds: Record<string, number>;
  };
  workLedger: WorkLedgerRangeRollup;
  activityComposition?: ActivityCompositions;
}

export interface AiProvider {
  id: string;
  name: string;
  baseUrl: string;
  model: string;
  enabled: boolean;
  priority: number;
  hasCredential: boolean;
}

export interface BrowserSource {
  browser: string;
  profile: string;
  historyPath: string;
  available: boolean;
}

export interface AppSettings {
  idleThresholdMinutes: number;
  monitoringEnabled: boolean;
  aiBackfillEnabled: boolean;
  aiExecutionMode: AiExecutionMode;
  selectedApiProviderId: string | null;
  codexExecutable: string;
  codexModel: string;
  aiAutoResearchAnalysisEnabled: boolean;
  /** Read-only compatibility alias for pre-1.3 snapshots. */
  aiAutoTrendAnalysisEnabled?: boolean;
  aiAutoClassificationEnabled: boolean;
  aiAutoWorkflowAssignmentEnabled: boolean;
  aiAutomationNoticeVersion: number;
  excludedApps: string[];
  excludedDomains: string[];
  uiTheme: UiTheme;
  experimentalKnowledgeGraphEnabled: boolean;
}

export type SettingsPatch = Partial<Pick<AppSettings,
  | "idleThresholdMinutes"
  | "monitoringEnabled"
  | "aiBackfillEnabled"
  | "aiExecutionMode"
  | "selectedApiProviderId"
  | "codexExecutable"
  | "codexModel"
  | "aiAutoResearchAnalysisEnabled"
  | "aiAutoClassificationEnabled"
  | "aiAutoWorkflowAssignmentEnabled"
  | "aiAutomationNoticeVersion"
  | "excludedApps"
  | "excludedDomains"
  | "uiTheme"
  | "experimentalKnowledgeGraphEnabled"
>>;

export type AiExecutionMode = "api-key" | "codex";

export type AiConnectionHealthStatus =
  | "checking"
  | "healthy"
  | "reachable"
  | "rate-limited"
  | "unconfigured"
  | "unavailable"
  | "permission-denied"
  | "timed-out"
  | "error";

export interface AiConnectionHealth {
  executionMode: AiExecutionMode;
  executorId: string;
  executorLabel: string;
  model: string;
  status: AiConnectionHealthStatus;
  verificationLevel: "connectivity" | "inference";
  source: "background" | "manual" | "queue";
  checkedAtMs: number;
  verifiedAtMs: number | null;
  diagnostic: string | null;
}

export const AI_CONNECTION_HEALTH_CHANGED_EVENT = "ai-connection-health-changed";
export const WORKFLOW_CHANGED_EVENT = "workflow-changed";
export const ACTIVITY_CHANGED_EVENT = "activity-changed";

export interface ActivityChangedEvent {
  observedAtMs: number;
}

export type AiExecutionErrorKind = "provider" | "codex" | "invalid-job" | "invalid-response" | "persistence" | "unknown";
export type AiJobStatus = "pending" | "running" | "complete" | "awaiting-reassignment";
export type AiReviewKind = "classification" | "workflow_assignment" | "project_draft";
export type AiReviewState = "pending" | "auto_applied" | "manual_override" | "execution_error" | "dismissed" | "reverted";
export type AiReviewAction = "accept" | "change" | "ignore";

export interface AiExecutionAuditView {
  executionMode: AiExecutionMode | null;
  executorId: string | null;
  model: string | null;
  evidenceHash: string;
  generation: number;
  createdAtMs: number;
  startedAtMs: number | null;
  finishedAtMs: number | null;
  durationMs: number | null;
  exitCode: number | null;
  errorKind: AiExecutionErrorKind | null;
  diagnostic: string;
}

export interface AiQueueRecord {
  id: string;
  generation: number;
  kind: string;
  status: AiJobStatus;
  attempts: number;
  nextAttemptAtMs: number;
  lastError: string;
  execution: {
    executionMode: AiExecutionMode;
    executorId: string;
    model: string;
    evidenceHash: string;
    createdAtMs: number;
  };
  startedAtMs: number | null;
  finishedAtMs: number | null;
  durationMs: number | null;
  executorId: string | null;
  model: string | null;
  exitCode: number | null;
  errorKind: AiExecutionErrorKind | null;
}

export interface AiReviewRecord {
  id: string;
  kind: AiReviewKind;
  state: AiReviewState;
  subjectId: string;
  beforeJson: string;
  proposedJson: string;
  appliedJson: string | null;
  confidence: number | null;
  evidenceSummary: string;
  evidenceHash: string;
  execution: AiExecutionAuditView;
  createdAtMs: number;
  resolvedAtMs: number | null;
}

export interface AiReviewFilter {
  states: AiReviewState[];
  kinds: AiReviewKind[];
  subjectId: string | null;
  executionMode: AiExecutionMode | null;
  executorId: string | null;
  model: string | null;
  createdFromMs: number | null;
  createdToMs: number | null;
  minConfidence: number | null;
  maxConfidence: number | null;
}

export interface AiReviewResolution {
  reviewIds: string[];
  action: AiReviewAction;
  changedJson: string | null;
  evidenceHashes: Record<string, string>;
  resolvedAtMs: number;
}

export type CodexHealthStatus =
  | "healthy"
  | "permission-denied"
  | "timed-out"
  | "unavailable"
  | "error";

export interface CodexHealth {
  configuredPath: string;
  detectedPath: string | null;
  version: string | null;
  checkedAtMs: number;
  status: CodexHealthStatus;
  diagnostic: string | null;
}

export type UiTheme = "classic-workbench" | "moon-glass" | "soft-paper" | "blueprint-data" | "knowledge-space";

export type KnowledgeGraphNodeKind = "category" | "app" | "domain" | "day" | "activity" | "browser-visit";

export interface KnowledgeGraphNode {
  id: string;
  kind: KnowledgeGraphNodeKind;
  label: string;
  durationSeconds: number;
  category: string;
  confidence: number;
  occurredAtMs: number | null;
  metadata: Record<string, string>;
}

export interface KnowledgeGraphLink {
  source: string;
  target: string;
  kind: string;
  weightSeconds: number;
}

export interface KnowledgeGraphPayload {
  nodes: KnowledgeGraphNode[];
  links: KnowledgeGraphLink[];
  counts: Record<string, number>;
  totalSeconds: number;
  startMs: number;
  endMs: number;
}

export interface TrendDay {
  date: string;
  label: string;
  monitoredSeconds: number;
  activeSeconds: number;
  idleSeconds: number;
  learningSeconds: number;
  switchCount: number;
  longestFocusSeconds: number;
  completedTaskCount?: number;
  classificationCoverage: number;
  topCategory: TrendBreakdownItem | null;
  topApp: TrendBreakdownItem | null;
}

export interface TrendRange {
  startMs: number;
  endMs: number;
  startDate: string;
  endDate: string;
  dayCount: number;
}

export interface TrendBreakdownItem {
  name: string;
  seconds: number;
  share: number;
}

export interface TrendSummary {
  monitoredSeconds: number;
  activeSeconds: number;
  idleSeconds: number;
  learningSeconds: number;
  switchCount: number;
  longestFocusSeconds: number;
  completedTaskCount?: number;
  averageMonitoredSeconds: number;
  averageActiveSeconds: number;
  averageIdleSeconds: number;
  averageLearningSeconds: number;
  averageSwitchCount: number;
  learningRatio: number;
  switchesPerActiveHour: number;
  productiveDayCount: number;
  focusDayCount: number;
  categoryBreakdown: TrendBreakdownItem[];
  appBreakdown: TrendBreakdownItem[];
}

export interface TrendComparison {
  previousRange: TrendRange;
  dayCount: number;
  previousMonitoredSeconds: number;
  previousActiveSeconds: number;
  previousIdleSeconds: number;
  previousLearningSeconds: number;
  previousSwitchCount: number;
  previousLongestFocusSeconds: number;
  previousCompletedTaskCount?: number;
  previousLearningRatio: number;
  previousSwitchesPerActiveHour: number;
  previousClassificationCoverage: number;
  previousCategoryBreakdown: TrendBreakdownItem[];
  previousAppBreakdown: TrendBreakdownItem[];
  monitoredSecondsDeltaPercent: number | null;
  activeSecondsDeltaPercent: number | null;
  idleSecondsDeltaPercent: number | null;
  learningSecondsDeltaPercent: number | null;
  switchCountDeltaPercent: number | null;
  longestFocusSecondsDeltaPercent: number | null;
  completedTaskCountDeltaPercent?: number | null;
  learningRatioDeltaPercent: number | null;
  switchesPerActiveHourDeltaPercent: number | null;
  classificationCoverageDeltaPercent: number | null;
}

export interface TrendDataQuality {
  recordedDayCount: number;
  missingDayCount: number;
  classifiedSeconds: number;
  pendingSeconds: number;
  lowConfidenceSeconds: number;
  classificationCoverage: number;
}

export interface TrendPayload {
  range: TrendRange;
  days: TrendDay[];
  summary: TrendSummary;
  comparison: TrendComparison;
  quality: TrendDataQuality;
  workLedger: WorkLedgerRangeRollup;
  evidenceHash: string;
}

export type TrendGranularity = "day" | "week" | "month";
export type TrendSelectionMode = "continuous" | "selectedDates";
export type TrendMetric =
  | "monitoredSeconds"
  | "activeSeconds"
  | "learningSeconds"
  | "idleSeconds"
  | "switchCount"
  | "longestFocusSeconds"
  | "classificationCoverage"
  | "completedTaskCount"
  | "linkedTaskSeconds";
export type TrendBaselineKind =
  | "current"
  | "previousEqualLength"
  | "previousMonthSamePeriod"
  | "custom";
export type TrendEvidenceScope = "summary" | "bucket" | "baseline" | "rate";
export type TrendMetricAvailabilityStatus = "available" | "unavailable";
export type TrendWorkbenchErrorCode =
  | "invalidRequest"
  | "metricUnavailable"
  | "dataAccess"
  | "runtimeUnavailable";

export interface TrendDateRange {
  startDate: string;
  endDate: string;
}

export interface TrendWorkbenchRequest {
  startDate: string;
  endDate: string;
  selectedDates?: string[] | null;
  timezoneOffsetMinutes: number;
  granularity?: TrendGranularity | null;
  metric: TrendMetric;
  activityScope?: ActivityScope;
  customBaseline?: TrendDateRange | null;
}

export interface TrendWorkbenchRange extends TrendRange {
  selectionMode?: TrendSelectionMode;
  selectedDates?: string[];
  selectedDateCount?: number;
  envelopeDayCount?: number;
}

export interface TrendMetricAvailability {
  metric: TrendMetric;
  status: TrendMetricAvailabilityStatus;
  reasonCode: TrendWorkbenchErrorCode | null;
}

export interface TrendWorkbenchError {
  code: TrendWorkbenchErrorCode;
  message: string;
  metric: TrendMetric | null;
}

export interface TrendMetricValues {
  monitoredSeconds: number;
  activeSeconds: number;
  learningSeconds: number;
  idleSeconds: number;
  switchCount: number;
  longestFocusSeconds: number;
  classificationCoverage: number;
  completedTaskCount: number;
  linkedTaskSeconds: number;
}

export interface TrendStatisticalMetricValues {
  monitoredSeconds: number;
  activeSeconds: number;
  learningSeconds: number;
  idleSeconds: number;
  switchCount: number;
  longestFocusSeconds: number;
  classificationCoverage: number;
  completedTaskCount: number;
  linkedTaskSeconds: number;
}

export interface TrendBucket {
  id: string;
  startDate: string;
  endDate: string;
  values: TrendMetricValues;
  recordedDayCount: number;
  missingDayCount: number;
  selectionMode?: TrendSelectionMode;
  selectedDates?: string[];
  selectedDateCount?: number;
  envelopeDayCount?: number;
  evidenceIds: string[];
  activityComposition?: ActivityCompositions;
  drilldown: TrendBucketDrilldown;
}

export type TrendRawEvidenceKind = "activity" | "focus";
export type TrendReviewState = "confirmed" | "pending";

export interface TrendRawRow {
  rowId: string;
  bucketId: string;
  evidenceKind: TrendRawEvidenceKind;
  evidenceId: string;
  date: string;
  startTime: string;
  endTime: string;
  app: string;
  titleSummary: string;
  category: string;
  videoPurpose?: VideoPurpose | null;
  meaningful?: boolean;
  meaningfulReason?: MeaningfulReason;
  taskId: string | null;
  taskTitle: string | null;
  projectId: string | null;
  projectName: string | null;
  clippedDurationSeconds: number;
  confidence: number | null;
  reviewState: TrendReviewState;
  shared: boolean;
}

export interface TrendDistributionItem {
  key: string;
  label: string;
  seconds: number;
}

export interface TrendCompletedTaskItem {
  taskId: string;
  taskTitle: string;
  projectId: string;
  projectName: string;
  completedAtMs: number;
}

export interface TrendLinkedTaskRollup {
  taskId: string;
  taskTitle: string;
  projectId: string;
  projectName: string;
  linkedSeconds: number;
  activitySeconds: number;
  focusSeconds: number;
  evidenceCount: number;
  sharedEvidenceCount: number;
}

export interface TrendLinkedProjectRollup {
  projectId: string;
  projectName: string;
  linkedSeconds: number;
  activitySeconds: number;
  focusSeconds: number;
  evidenceCount: number;
  sharedEvidenceCount: number;
}

export interface TrendWorkflowOwnership {
  ownershipId: string;
  evidenceKind: TrendRawEvidenceKind;
  evidenceId: string;
  taskId: string;
  taskTitle: string;
  projectId: string;
  projectName: string;
  shared: boolean;
}

export interface TrendDrilldownQuality {
  recordedDayCount: number;
  missingDayCount: number;
  classifiedSeconds: number;
  classificationCoverage: number;
  lowConfidenceSeconds: number;
  pendingSeconds: number;
}

export interface TrendBucketDrilldown {
  bucketId: string;
  rawRows: TrendRawRow[];
  applicationDistribution: TrendDistributionItem[];
  categoryDistribution: TrendDistributionItem[];
  completedTasks: TrendCompletedTaskItem[];
  linkedTaskRollups: TrendLinkedTaskRollup[];
  linkedProjectRollups: TrendLinkedProjectRollup[];
  workflowOwnership: TrendWorkflowOwnership[];
  dataQuality: TrendDrilldownQuality;
}

export interface TrendBaselineSeries {
  kind: TrendBaselineKind;
  range: TrendDateRange;
  isValid: boolean;
  value: number | null;
  absoluteDelta: number | null;
  percentDelta: number | null;
  recordedDayCount: number;
  missingDayCount: number;
  selectionMode?: TrendSelectionMode;
  selectedDates?: string[];
  selectedDateCount?: number;
  envelopeDayCount?: number;
  evidenceIds: string[];
}

export interface TrendWorkbenchSummary {
  totals: TrendMetricValues;
  dailyAverage: {
    monitoredSeconds: number | null;
    activeSeconds: number | null;
    idleSeconds: number | null;
    learningSeconds: number | null;
  };
  averageSampleDayCount: number;
  switchesPerActiveHour: number | null;
  meanPerBucket: TrendStatisticalMetricValues;
  dailyMedian: TrendStatisticalMetricValues;
  dailyMax: TrendStatisticalMetricValues;
  dailySampleStddev: TrendStatisticalMetricValues;
  dailyCoefficientOfVariation: TrendStatisticalMetricValues;
  recordedDayCount: number;
  effectiveActivityDayCount: number;
  missingDayCount: number;
  classifiedSeconds: number;
  classificationCoverage: number;
  lowConfidenceSeconds: number;
  pendingSeconds: number;
  evidenceIds: string[];
}

export interface TrendEvidence {
  id: string;
  scope: TrendEvidenceScope;
  seriesKind: TrendBaselineKind;
  bucketId: string | null;
  metric: TrendMetric | null;
  value: number;
}

export interface TrendWorkbenchPayload {
  range: TrendWorkbenchRange;
  granularity: TrendGranularity;
  metric: TrendMetric;
  metricAvailability: TrendMetricAvailability[];
  buckets: TrendBucket[];
  summary: TrendWorkbenchSummary;
  activityComposition?: ActivityCompositions;
  inactivityReasonDistribution?: Array<{
    reason: InactivityReason;
    seconds: number;
  }>;
  baselines: TrendBaselineSeries[];
  evidence: TrendEvidence[];
  evidenceHash: string;
  /** Evidence hash for the currently requested AI activity scope. */
  analysisEvidenceHash?: string;
  analysisActivityScope?: ActivityScope;
  /** Scope-specific values used by the local facts shown beside the AI result. */
  analysisSummary?: TrendWorkbenchSummary;
  analysisEvidence?: TrendEvidence[];
}

export interface TrendAnalysisResult {
  rangeStart: string;
  rangeEnd: string;
  evidenceHash: string;
  summary: string;
  observations: string[];
  suggestions: string[];
  source: string;
  model: string;
  confidence: number;
  generatedAtMs: number;
  activityScope: ActivityScope;
}

export type ResearchStatus = "ready" | "limitations_only";
export type EvidenceRelation = "supports" | "increased" | "decreased" | "stable";

export interface TrendEvidenceClaim {
  evidenceId: string;
  relation: EvidenceRelation;
}

export interface TrendResearchFinding {
  observation: string;
  possibleExplanation: string;
  validationMethod: string;
  evidenceIds: string[];
  claims: TrendEvidenceClaim[];
  confidence: number;
  limitations: string[];
}

export interface TrendResearchAnalysis {
  status: ResearchStatus;
  findings: TrendResearchFinding[];
  limitations: string[];
  source: string;
  model: string;
  evidenceHash: string;
  activityScope: ActivityScope;
}

export type WorkLedgerEvidenceKind = "activity" | "browser";
export type WorkLedgerProjectStatus = "active" | "archived";
export type WorkLedgerTaskStatus = "todo" | "in_progress" | "blocked" | "completed" | "cancelled";
export type WorkLedgerTaskPriority = "low" | "medium" | "high" | "urgent";
export type WorkLedgerEvidenceProvenance = "manual" | "rule" | "ai";

export interface WorkLedgerProject {
  id: string;
  name: string;
  color: string;
  status: WorkLedgerProjectStatus;
  description: string;
  createdAtMs: number;
  updatedAtMs: number;
  archivedAtMs: number | null;
}

export interface WorkLedgerTask {
  id: string;
  projectId: string;
  title: string;
  status: WorkLedgerTaskStatus;
  priority: WorkLedgerTaskPriority;
  expectedOutput: string;
  dueDate: string | null;
  createdAtMs: number;
  updatedAtMs: number;
  completedAtMs: number | null;
  originKind?: "manual" | "ai";
  originKey?: string | null;
  originConfidence?: number | null;
  reviewState?: "confirmed" | "provisional" | "pending";
}

export interface TaskDailyPoint {
  date: string;
  investedSeconds: number;
  activitySeconds: number;
  focusSeconds: number;
  switchCount: number;
}

export interface TaskTimeSummary {
  taskId: string;
  lifecycleTotalSeconds: number;
  activeDayAverageSeconds: number;
  naturalDayAverageSeconds: number;
  activeDayCount: number;
  naturalDayCount: number;
  latestActivityAtMs: number | null;
  assignmentConfidence: number | null;
  reviewState: "confirmed" | "provisional" | "pending";
}

export interface TaskEfficiencyDimension {
  key: "time_investment" | "continuity" | "switching_cost" | "regularity" | "evidence_confidence" | string;
  label: string;
  conclusion: string;
  value: number | null;
  unit: string;
}

export interface TaskEfficiencyAssessment {
  dimensions: TaskEfficiencyDimension[];
  dataLimitations: string[];
}

export interface TaskTimeInsight {
  summary: TaskTimeSummary;
  dailyPoints: TaskDailyPoint[];
  medianDailySeconds: number;
  longestContinuousSeconds: number;
  focusSeconds: number;
  focusShare: number;
  switchesPerHour: number;
  regularity: number;
  manualCorrectionRate: number;
  pendingReviewSeconds: number;
  browserEvidenceCount: number;
  progressCount: number;
  expectedOutput: string;
  assessment: TaskEfficiencyAssessment;
}

export interface ProjectTimeSummary {
  projectId: string;
  lifecycleTotalSeconds: number;
  activeDayAverageSeconds: number;
  naturalDayAverageSeconds: number;
  activeDayCount: number;
  naturalDayCount: number;
  evidenceCount: number;
  completedTaskCount: number;
  taskCount: number;
  latestActivityAtMs: number | null;
}

export interface ProjectDailyPoint {
  date: string;
  investedSeconds: number;
  activitySeconds: number;
  focusSeconds: number;
  taskSeconds: Record<string, number>;
  sharedSeconds: number;
}

export interface ProjectTaskContribution {
  taskId: string;
  taskTitle: string;
  status: WorkLedgerTaskStatus;
  investedSeconds: number;
  evidenceCount: number;
}

export interface ProjectTimeInsight {
  summary: ProjectTimeSummary;
  dailyPoints: ProjectDailyPoint[];
  taskContributions: ProjectTaskContribution[];
  sharedSeconds: number;
  focusSeconds: number;
  longestContinuousSeconds: number;
  switchCount: number;
  switchesPerHour: number;
  browserEvidenceCount: number;
}

export interface AiTaskCancellation {
  task: WorkLedgerTask;
  releasedEvidenceCount: number;
  dismissedClusterCount: number;
  projectArchived: boolean;
}

export interface WorkLedgerProgressEntry {
  id: string;
  taskId: string;
  note: string;
  createdAtMs: number;
  originKind: "manual" | "focus_outcome" | "daily_actual_output";
  sourceId: string | null;
  sourceDate: string | null;
}

export interface DailyGoalRecord {
  date: string;
  goals: string;
  expectedOutput: string;
  actualOutput: string;
}

export interface DailyGoalTaskLink {
  goalRowId: string;
  goalDate: string;
  goalText: string;
  taskId: string;
  confirmedAtMs: number;
}

export interface DailyGoalTaskCreation {
  projectId: string;
  title: string;
  priority: WorkLedgerTaskPriority;
  expectedOutput: string;
  dueDate: string | null;
}

export interface DailyGoalTaskConfirmationRequest {
  goalRowId: string;
  goalDate: string;
  goalText: string;
  taskId: string;
  newTask?: DailyGoalTaskCreation;
}

export interface ConfirmedDailyGoalTask {
  link: DailyGoalTaskLink;
  task: WorkLedgerTask;
  taskCreated: boolean;
}

export interface WorkLedgerRangeRollup {
  startMs: number;
  endMs: number;
  projects: ProjectRangeRollup[];
  tasks: TaskRangeRollup[];
}

export interface ProjectRangeRollup {
  projectId: string;
  projectName: string;
  projectStatus: WorkLedgerProjectStatus;
  investedSeconds: number;
  focusSeconds: number;
  activitySegmentCount: number;
  browserVisitCount: number;
  progressCount: number;
  focusSessionCount: number;
}

export interface TaskRangeRollup {
  taskId: string;
  projectId: string;
  taskTitle: string;
  taskStatus: WorkLedgerTaskStatus;
  investedSeconds: number;
  focusSeconds: number;
  activitySegmentCount: number;
  browserVisitCount: number;
  progressCount: number;
  focusSessionCount: number;
}

export interface WorkLedgerEvidence {
  kind: WorkLedgerEvidenceKind;
  id: string;
  occurredAtMs: number;
  durationSeconds: number;
  application: string;
  title: string;
  domain: string;
  category?: string;
  workflowEligibility?: "initiator" | "context" | "excluded" | string;
  eligibilityReason?: string;
  classificationSource: string;
  classificationReason: string;
  classificationConfidence: number | null;
  evidenceHash: string;
}

export interface LinkedWorkLedgerEvidence {
  taskId: string;
  provenance: WorkLedgerEvidenceProvenance;
  assignmentConfidence: number;
  assignmentReason: string;
  assignedAtMs: number;
  evidence: WorkLedgerEvidence;
}

export interface WorkLedgerSuggestion {
  evidenceKind: WorkLedgerEvidenceKind;
  evidenceId: string;
  taskId: string;
  confidence: number;
  reason: string;
  evidenceHash: string;
  source: "local" | "ai";
  canAutoApply: boolean;
}

export interface WorkLedgerSummary {
  projectCount: number;
  taskCount: number;
  progressCount: number;
  linkedEvidenceCount: number;
  unassignedEvidenceCount: number;
  localSuggestionCount: number;
  ambiguousEvidenceCount: number;
}

export interface ProjectDraftTaskProposal {
  key: string;
  title: string;
  expectedOutput: string;
  clusterIds: string[];
  semanticReason?: string;
}

export interface ProjectDraftProposal {
  targetProjectId: string | null;
  name: string;
  description: string;
  tasks: ProjectDraftTaskProposal[];
  confidence: number;
  reasonCode: string;
  semanticReason?: string;
  dataLimitations?: string[];
}

export interface ProjectDraftView {
  reviewId: string;
  subjectId: string;
  proposal: ProjectDraftProposal;
  evidenceCount: number;
  accumulatedSeconds: number;
  createdAtMs: number;
}

export interface WorkLedgerSnapshot {
  projects: WorkLedgerProject[];
  tasks: WorkLedgerTask[];
  progress: WorkLedgerProgressEntry[];
  linkedEvidence: LinkedWorkLedgerEvidence[];
  unassignedEvidence: WorkLedgerEvidence[];
  suggestions: WorkLedgerSuggestion[];
  ambiguousEvidenceHashes: string[];
  projectDrafts?: ProjectDraftView[];
  taskSummaries?: TaskTimeSummary[];
  summary: WorkLedgerSummary;
}

export interface WorkflowAnalysisQueueResult {
  queuedCount: number;
  reusedCount: number;
  jobIds: string[];
}

export interface WorkLedgerProjectSaveRequest {
  id: string;
  name: string;
  color: string;
  description: string;
}

export interface WorkLedgerTaskSaveRequest {
  id: string;
  projectId: string;
  title: string;
  priority: WorkLedgerTaskPriority;
  expectedOutput: string;
  dueDate: string | null;
}

export interface WorkLedgerEvidenceAssignmentRequest {
  taskId: string;
  evidenceKind: WorkLedgerEvidenceKind;
  evidenceId: string;
  reason?: string;
}

export interface WorkLedgerSuggestedAssignmentRequest {
  taskId: string;
  evidenceKind: WorkLedgerEvidenceKind;
  evidenceId: string;
  evidenceHash: string;
  source: "local" | "ai";
  startMs: number;
  endMs: number;
  confidence: number;
  reason: string;
}

export interface TrendRangeArguments {
  [key: string]: unknown;
  startDate: string;
  endDate: string;
  dayBoundariesMs: number[];
  comparisonStartDate: string;
  comparisonEndDate: string;
  comparisonDayBoundariesMs: number[];
  timezoneOffsetMinutes: number;
}

const appIdentityCache = new Map<string, AppIdentity>();

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export function isDesktopRuntime(): boolean {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

export function dayBounds(date: string): { startMs: number; endMs: number } {
  const start = parseLocalDate(date);
  const end = new Date(start);
  end.setDate(end.getDate() + 1);
  return {
    startMs: start.getTime(),
    endMs: end.getTime(),
  };
}

export function buildTrendRangeArguments(startDate: string, endDate: string): TrendRangeArguments {
  const start = parseLocalDate(startDate);
  const end = parseLocalDate(endDate);
  if (start > end) throw new RangeError("Trend start date must not be after end date");
  const inclusiveDayCount = calendarDayNumber(end) - calendarDayNumber(start) + 1;
  if (inclusiveDayCount > 366) throw new RangeError("Trend range cannot exceed 366 days");

  const dayBoundariesMs = localDayBoundaries(start, end);
  const dayCount = dayBoundariesMs.length - 1;
  const comparisonEnd = new Date(start);
  comparisonEnd.setDate(comparisonEnd.getDate() - 1);
  const comparisonStart = new Date(start);
  comparisonStart.setDate(comparisonStart.getDate() - dayCount);

  return {
    startDate,
    endDate,
    dayBoundariesMs,
    comparisonStartDate: formatLocalDate(comparisonStart),
    comparisonEndDate: formatLocalDate(comparisonEnd),
    comparisonDayBoundariesMs: localDayBoundaries(comparisonStart, comparisonEnd),
    timezoneOffsetMinutes: new Date().getTimezoneOffset(),
  };
}

function parseLocalDate(value: string): Date {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) throw new RangeError("Date must use YYYY-MM-DD format");
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) {
    throw new RangeError("Date is not a valid local calendar day");
  }
  return date;
}

function localDayBoundaries(start: Date, end: Date): number[] {
  const cursor = new Date(start);
  const boundaries: number[] = [];
  while (cursor <= end) {
    boundaries.push(cursor.getTime());
    cursor.setDate(cursor.getDate() + 1);
  }
  boundaries.push(cursor.getTime());
  return boundaries;
}

function calendarDayNumber(date: Date): number {
  return Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86_400_000;
}

function formatLocalDate(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function toUiSegments(
  records: BackendSegment[],
  dayStartMs: number,
  dayEndMs = dayStartMs + 86_400_000,
): Segment[] {
  return records.map((record) => ({
    id: record.id,
    startMs: Math.max(0, record.startedAtMs - dayStartMs),
    endMs: Math.max(0, Math.min(record.endedAtMs, dayEndMs) - dayStartMs),
    app: record.app,
    appPath: record.appPath ?? "",
    title: record.title,
    category: record.category,
    videoPurpose: record.videoPurpose,
    confidence: record.confidence,
    needsReview: record.needsReview,
    inactivityReason: record.inactivityReason ?? null,
  }));
}

export async function loadDashboard(date: string): Promise<Segment[]> {
  const snapshot = await loadDashboardSnapshot(date);
  const { startMs, endMs } = dayBounds(date);
  return toUiSegments(snapshot.timeline, startMs, endMs);
}

export async function loadDashboardSnapshot(date: string): Promise<DashboardSnapshot> {
  const { startMs, endMs } = dayBounds(date);
  return invoke<DashboardSnapshot>("get_today_dashboard", { startMs, endMs });
}

export async function resolveAppIdentities(apps: AppIdentitySource[]): Promise<Map<string, AppIdentity>> {
  const unique = new Map<string, { rawName: string; executablePath: string }>();
  for (const app of apps) {
    const request = { rawName: app.app, executablePath: app.appPath ?? "" };
    const key = appIdentityKey(request.rawName, request.executablePath);
    if (!appIdentityCache.has(key)) unique.set(key, request);
  }

  if (unique.size) {
    const resolved = await invoke<AppIdentity[]>("resolve_app_identities", { apps: [...unique.values()] });
    for (const identity of resolved) {
      const normalized = {
        ...identity,
        displayName: resolveDisplayName(identity.rawName, identity.productName, identity.executablePath),
      };
      appIdentityCache.set(appIdentityKey(identity.rawName, identity.executablePath), normalized);
    }
    for (const [key, request] of unique) {
      if (!appIdentityCache.has(key)) {
        appIdentityCache.set(key, fallbackAppIdentity(request.rawName, request.executablePath));
      }
    }
  }

  return new Map(apps.map((app) => {
    const key = appIdentityKey(app.app, app.appPath ?? "");
    return [key, appIdentityCache.get(key) ?? fallbackAppIdentity(app.app, app.appPath ?? "")];
  }));
}

export async function updateMonitoring(enabled: boolean): Promise<void> {
  await invoke("set_monitoring_state", { enabled });
}

export async function classifySegment(
  segmentId: string,
  category: ActivityCategory,
  videoPurpose: VideoPurpose = "unknown",
): Promise<void> {
  await invoke("save_manual_classification", {
    request: {
      segmentId,
      category,
      videoPurpose: category === "video_input" ? videoPurpose : "unknown",
      reason: "User correction",
    },
  });
}

export async function beginFocus(
  date: string,
  goalText: string,
  plannedMinutes: number,
  taskId: string | null = null,
): Promise<string> {
  return invoke<string>("start_focus_session", { goalDate: date, goalText, plannedMinutes, taskId });
}

export async function completeFocus(id: string, outcome: string): Promise<boolean> {
  return invoke<boolean>("complete_focus_session", { id, outcome });
}

export async function finishFocus(id: string, outcome: string): Promise<boolean> {
  return completeFocus(id, outcome);
}

export async function getDailyGoal(date: string): Promise<DailyGoalRecord> {
  return invoke<DailyGoalRecord>("get_daily_goal", { date });
}

export async function saveDailyGoal(goal: DailyGoalRecord): Promise<void> {
  await invoke("save_daily_goal", { ...goal });
}

export async function persistGoal(
  date: string,
  goals: string,
  expectedOutput: string,
  actualOutput: string,
): Promise<void> {
  await saveDailyGoal({ date, goals, expectedOutput, actualOutput });
}

export async function confirmDailyGoalTask(
  request: DailyGoalTaskConfirmationRequest,
): Promise<ConfirmedDailyGoalTask> {
  return invoke<ConfirmedDailyGoalTask>("confirm_daily_goal_task", { request });
}

export async function listDailyGoalTaskLinks(date: string): Promise<DailyGoalTaskLink[]> {
  return invoke<DailyGoalTaskLink[]>("list_daily_goal_task_links", { date });
}

export async function recordDailyActualOutputProgress(
  taskId: string,
  date: string,
): Promise<WorkLedgerProgressEntry | null> {
  return invoke<WorkLedgerProgressEntry | null>("record_daily_actual_output_progress", {
    request: { taskId, date },
  });
}

export interface DailyGoalRow {
  id: string;
  text: string;
  goalText: string;
}

export function buildDailyGoalRows(date: string, goals: string): DailyGoalRow[] {
  const occurrences = new Map<string, number>();
  return goals.split(/\r?\n/).flatMap((line) => {
    const goalText = line.trim();
    if (!goalText) return [];
    const text = goalText.replace(/\s+/g, " ");
    const occurrence = (occurrences.get(text) ?? 0) + 1;
    occurrences.set(text, occurrence);
    return [{ id: `goal-${date}-${encodeURIComponent(text)}-${occurrence}`, text, goalText }];
  });
}

export async function loadDailyAnalysis(
  date: string,
  activityScope: ActivityScope = "all",
): Promise<ScopedDailyAnalysisResult> {
  const { startMs, endMs } = dayBounds(date);
  const result = await invoke<DailyAnalysisResult & { activityScope?: ActivityScope }>("get_daily_analysis", {
    date,
    startMs,
    endMs,
    activityScope,
  });
  return { ...result, activityScope: result.activityScope === "meaningful" ? "meaningful" : "all" };
}

export async function enqueueDailyAnalysis(
  date: string,
  activityScope: ActivityScope = "all",
): Promise<string> {
  const { startMs, endMs } = dayBounds(date);
  return invoke<string>("queue_daily_analysis", { date, startMs, endMs, activityScope });
}

export async function listenDailyAnalysisChanged(
  handler: (event: AnalysisChangedEvent) => void,
): Promise<UnlistenFn> {
  return listen<AnalysisChangedEvent>(ANALYSIS_CHANGED_EVENT, (event) => {
    handler(event.payload);
  });
}

export async function listenAnalysisChanged(
  handler: (event: AnalysisChangedEvent) => void,
): Promise<UnlistenFn> {
  return listenDailyAnalysisChanged(handler);
}

export async function listAiProviders(): Promise<AiProvider[]> {
  return invoke<AiProvider[]>("list_ai_providers");
}

export async function saveAiKey(providerId: string, apiKey: string): Promise<void> {
  await invoke("save_ai_provider_key", { providerId, apiKey });
}

export async function saveCustomProvider(baseUrl: string, model: string): Promise<void> {
  await invoke("save_custom_ai_provider", { baseUrl, model });
}

export async function testAiProvider(providerId: string): Promise<string> {
  return invoke<string>("test_ai_provider", { providerId });
}

export type ReportFormat = "markdown" | "docx";
export type ReportScope = "daily" | "trend" | "task" | "project";

export interface ExportReportRequest {
  format: ReportFormat;
  scope: ReportScope;
  date?: string;
  startMs?: number;
  endMs?: number;
  startDate?: string;
  endDate?: string;
  dayBoundariesMs?: number[];
  comparisonStartDate?: string;
  comparisonEndDate?: string;
  comparisonDayBoundariesMs?: number[];
  trendRequest?: TrendWorkbenchRequest;
  taskId?: string;
  projectId?: string;
}

export async function getCodexHealth(): Promise<CodexHealth> {
  return invoke<CodexHealth>("get_codex_health");
}

export async function testCodexCli(): Promise<CodexHealth> {
  return invoke<CodexHealth>("test_codex_cli");
}

export async function getAiConnectionHealth(): Promise<AiConnectionHealth> {
  return invoke<AiConnectionHealth>("get_ai_connection_health");
}

export async function refreshAiConnectionHealth(): Promise<AiConnectionHealth> {
  return invoke<AiConnectionHealth>("refresh_ai_connection_health");
}

export async function listenAiConnectionHealthChanged(
  handler: (health: AiConnectionHealth) => void,
): Promise<UnlistenFn> {
  return listen<AiConnectionHealth>(AI_CONNECTION_HEALTH_CHANGED_EVENT, (event) => {
    handler(event.payload);
  });
}

export async function listenWorkflowChanged(
  handler: (event: { jobId?: string; status: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ jobId?: string; status: string }>(WORKFLOW_CHANGED_EVENT, (event) => {
    handler(event.payload);
  });
}

export async function listenActivityChanged(
  handler: (event: ActivityChangedEvent) => void,
): Promise<UnlistenFn> {
  return listen<ActivityChangedEvent>(ACTIVITY_CHANGED_EVENT, (event) => {
    handler(event.payload);
  });
}

export async function listBrowserSources(): Promise<BrowserSource[]> {
  return invoke<BrowserSource[]>("get_browser_sources");
}

export async function scanBrowserHistory(date: string): Promise<{ visitsFound: number; errors: string[] }> {
  const { startMs, endMs } = dayBounds(date);
  return invoke("scan_browsers", { startMs, endMs });
}

export async function updateIdleThreshold(idleThresholdMinutes: number): Promise<void> {
  await invoke("update_settings", { patch: { idleThresholdMinutes } });
}

export async function updateAiBackfill(aiBackfillEnabled: boolean): Promise<void> {
  await invoke("update_settings", { patch: { aiBackfillEnabled } });
}

export async function updateAiExecutionMode(aiExecutionMode: AiExecutionMode): Promise<void> {
  await invoke("update_settings", { patch: { aiExecutionMode } });
}

export async function updateAiAutomation(patch: Pick<AppSettings, "aiAutoResearchAnalysisEnabled" | "aiAutoClassificationEnabled" | "aiAutoWorkflowAssignmentEnabled">): Promise<void> {
  await invoke("update_settings", { patch });
}

export async function updateSettings(patch: SettingsPatch): Promise<AppSettings> {
  return invoke<AppSettings>("update_settings", { patch });
}

export async function updatePrivacyExclusions(excludedApps: string[], excludedDomains: string[]): Promise<void> {
  await invoke("update_settings", { patch: { excludedApps, excludedDomains } });
}

export async function updateUiTheme(uiTheme: UiTheme): Promise<void> {
  await invoke("update_settings", { patch: { uiTheme } });
}

export async function updateKnowledgeGraphExperiment(experimentalKnowledgeGraphEnabled: boolean): Promise<void> {
  await invoke("update_settings", { patch: { experimentalKnowledgeGraphEnabled } });
}

export async function loadKnowledgeGraph(
  startMs: number,
  endMs: number,
  search = "",
  nodeTypes: KnowledgeGraphNodeKind[] = [],
): Promise<KnowledgeGraphPayload> {
  return invoke<KnowledgeGraphPayload>("get_knowledge_graph", {
    startMs,
    endMs,
    filters: {
      search,
      nodeTypes,
      timezoneOffsetMinutes: new Date().getTimezoneOffset(),
    },
  });
}

export async function loadWorkLedger(
  startMs: number,
  endMs: number,
  projectId?: string,
): Promise<WorkLedgerSnapshot> {
  return invoke<WorkLedgerSnapshot>("get_work_ledger", { startMs, endMs, projectId: projectId ?? null });
}

export async function saveWorkLedgerProject(
  request: WorkLedgerProjectSaveRequest,
): Promise<WorkLedgerProject> {
  return invoke<WorkLedgerProject>("save_work_ledger_project", { request });
}

export async function archiveWorkLedgerProject(projectId: string): Promise<boolean> {
  return invoke<boolean>("archive_work_ledger_project", { projectId });
}

export async function saveWorkLedgerTask(
  request: WorkLedgerTaskSaveRequest,
): Promise<WorkLedgerTask> {
  return invoke<WorkLedgerTask>("save_work_ledger_task", { request });
}

export async function updateWorkLedgerTaskStatus(
  taskId: string,
  status: WorkLedgerTaskStatus,
): Promise<WorkLedgerTask> {
  return invoke<WorkLedgerTask>("update_work_ledger_task_status", {
    request: { taskId, status },
  });
}

export async function cancelWorkLedgerAiTask(
  taskId: string,
): Promise<AiTaskCancellation> {
  return invoke<AiTaskCancellation>("cancel_work_ledger_ai_task", { taskId });
}

export async function addWorkLedgerProgress(
  taskId: string,
  note: string,
): Promise<WorkLedgerProgressEntry> {
  return invoke<WorkLedgerProgressEntry>("add_work_ledger_progress", {
    request: { taskId, note },
  });
}

export async function assignWorkLedgerEvidence(
  request: WorkLedgerEvidenceAssignmentRequest,
): Promise<boolean> {
  return invoke<boolean>("assign_work_ledger_evidence", { request });
}

export async function removeWorkLedgerEvidence(
  request: WorkLedgerEvidenceAssignmentRequest,
): Promise<boolean> {
  return invoke<boolean>("remove_work_ledger_evidence", { request });
}

export async function applyWorkLedgerSuggestion(
  request: WorkLedgerSuggestedAssignmentRequest,
): Promise<boolean> {
  return invoke<boolean>("apply_work_ledger_suggestion", { request });
}

export async function loadWorkLedgerTaskInsight(
  taskId: string,
  endMs: number,
): Promise<TaskTimeInsight> {
  return invoke<TaskTimeInsight>("get_work_ledger_task_insight", { taskId, endMs });
}

export async function loadWorkLedgerProjectInsight(
  projectId: string,
  endMs: number,
  rangeDays: number,
): Promise<ProjectTimeInsight> {
  return invoke<ProjectTimeInsight>("get_work_ledger_project_insight", {
    projectId,
    endMs,
    rangeDays,
  });
}

export async function mergeWorkLedgerTasks(
  sourceTaskId: string,
  targetTaskId: string,
): Promise<boolean> {
  return invoke<boolean>("merge_work_ledger_tasks", {
    request: { sourceTaskId, targetTaskId },
  });
}

export async function runWorkflowAiSuggestions(
  startMs: number,
  endMs: number,
): Promise<WorkflowAnalysisQueueResult> {
  const queued = await invoke<WorkflowAnalysisQueueResult>("queue_workflow_ai_suggestions", {
    startMs,
    endMs,
  });
  await invoke<boolean>("run_ai_job_now");
  return queued;
}

export async function getAppSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function listAiReviews(filter: AiReviewFilter): Promise<AiReviewRecord[]> {
  return invoke<AiReviewRecord[]>("list_ai_reviews", { filter });
}

export async function listPendingAiJobs({
  status = "awaiting-reassignment",
  limit = 500,
}: {
  status?: AiJobStatus | null;
  limit?: number;
} = {}): Promise<AiQueueRecord[]> {
  return invoke<AiQueueRecord[]>("list_pending_ai_jobs", { status, limit });
}

export async function bulkRetryAiJobsWithCurrentMode(jobIds: string[]): Promise<AiQueueRecord[]> {
  return invoke<AiQueueRecord[]>("bulk_retry_ai_jobs_with_current_mode", { jobIds });
}

export async function resolveAiReview(request: AiReviewResolution): Promise<AiReviewRecord[]> {
  return invoke<AiReviewRecord[]>("resolve_ai_review", { request });
}

export async function revertAiAutoApply(reviewId: string): Promise<AiReviewRecord> {
  return invoke<AiReviewRecord>("revert_ai_auto_apply", { reviewId });
}

export async function retryAiReview(reviewId: string, useCurrentMode: boolean): Promise<boolean> {
  return invoke<boolean>("retry_ai_review", { reviewId, useCurrentMode });
}

export async function loadTrendSeries(endDate: string, days = 7): Promise<TrendDay[]> {
  const endBounds = dayBounds(endDate);
  const rangeStart = new Date(endBounds.startMs);
  rangeStart.setDate(rangeStart.getDate() - (days - 1));
  return (await loadTrendRange(formatLocalDate(rangeStart), endDate)).days;
}

export async function loadTrendRange(startDate: string, endDate: string): Promise<TrendPayload> {
  return invoke<TrendPayload>("get_trends", buildTrendRangeArguments(startDate, endDate));
}

export async function loadTrendWorkbench(
  request: TrendWorkbenchRequest,
): Promise<TrendWorkbenchPayload> {
  const { activityScope = "all", ...backendRequest } = request;
  const full = await invoke<TrendWorkbenchPayload>("get_trend_workbench", { request: backendRequest });
  if (activityScope === "all") {
    return {
      ...full,
      analysisEvidenceHash: full.evidenceHash,
      analysisActivityScope: "all",
      analysisSummary: full.summary,
      analysisEvidence: full.evidence,
    };
  }
  const scoped = await invoke<TrendWorkbenchPayload>("get_trend_workbench", {
    request: backendRequest,
    activityScope,
  });
  return {
    ...full,
    analysisEvidenceHash: scoped.evidenceHash,
    analysisActivityScope: activityScope,
    analysisSummary: scoped.summary,
    analysisEvidence: scoped.evidence,
  };
}

export function isTrendWorkbenchError(value: unknown): value is TrendWorkbenchError {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<TrendWorkbenchError>;
  const metric = candidate.metric;
  return ["invalidRequest", "metricUnavailable", "dataAccess", "runtimeUnavailable"].includes(
    candidate.code ?? "",
  ) && typeof candidate.message === "string"
    && (metric === null || (typeof metric === "string" && [
      "monitoredSeconds",
      "activeSeconds",
      "learningSeconds",
      "idleSeconds",
      "switchCount",
      "longestFocusSeconds",
      "classificationCoverage",
      "completedTaskCount",
      "linkedTaskSeconds",
    ].includes(metric)));
}

export async function loadTrendAnalysis(
  startDate: string,
  endDate: string,
  activityScope: ActivityScope = "all",
): Promise<TrendAnalysisResult> {
  const result = await invoke<Omit<TrendAnalysisResult, "activityScope"> & { activityScope?: ActivityScope }>(
    "get_trend_analysis",
    { ...buildTrendRangeArguments(startDate, endDate), activityScope },
  );
  return { ...result, activityScope: result.activityScope === "meaningful" ? "meaningful" : "all" };
}

export async function queueTrendAnalysis(
  startDate: string,
  endDate: string,
  force = false,
  activityScope: ActivityScope = "all",
): Promise<string | null> {
  return invoke<string | null>("queue_trend_analysis", {
    ...buildTrendRangeArguments(startDate, endDate),
    force,
    activityScope,
  });
}

export async function loadTrendResearchAnalysis(
  request: TrendWorkbenchRequest,
): Promise<TrendResearchAnalysis> {
  const result = await invoke<Omit<TrendResearchAnalysis, "activityScope"> & { activityScope?: ActivityScope }>(
    "get_trend_research_analysis",
    { request, activityScope: request.activityScope ?? "all" },
  );
  return { ...result, activityScope: result.activityScope === "meaningful" ? "meaningful" : "all" };
}

export async function queueTrendResearchAnalysis(
  request: TrendWorkbenchRequest,
  force = false,
): Promise<string | null> {
  return invoke<string | null>("queue_trend_research_analysis", {
    request,
    force,
    activityScope: request.activityScope ?? "all",
  });
}

export async function exportTrendMarkdown(request: TrendWorkbenchRequest): Promise<string> {
  return invoke<string>("export_trend_markdown", {
    ...buildTrendRangeArguments(request.startDate, request.endDate),
    path: "",
    request,
  });
}

export async function exportDailyMarkdown(date: string): Promise<string> {
  const { startMs, endMs } = dayBounds(date);
  return invoke<string>("export_markdown_report", { path: "", date, startMs, endMs });
}

export async function exportReport(request: ExportReportRequest): Promise<string | null> {
  return invoke<string | null>("export_report", { request });
}

export async function exportDailyReport(
  date: string,
  format: ReportFormat,
): Promise<string | null> {
  const { startMs, endMs } = dayBounds(date);
  return exportReport({ format, scope: "daily", date, startMs, endMs });
}

export async function exportTrendReport(
  request: TrendWorkbenchRequest,
  format: ReportFormat,
): Promise<string | null> {
  return exportReport({
    format,
    scope: "trend",
    trendRequest: request,
    ...buildTrendRangeArguments(request.startDate, request.endDate),
  });
}

export async function exportTaskReport(
  taskId: string,
  endMs: number,
  format: ReportFormat,
): Promise<string | null> {
  return exportReport({ format, scope: "task", taskId, endMs });
}

export async function exportProjectReport(
  projectId: string,
  endMs: number,
  format: ReportFormat,
): Promise<string | null> {
  return exportReport({ format, scope: "project", projectId, endMs });
}

export async function importLegacyActivity(path: string): Promise<{ imported: number; skipped: number }> {
  return invoke("import_legacy_data", { path });
}

export async function revealDataFolder(): Promise<string> {
  return invoke<string>("open_data_folder");
}
