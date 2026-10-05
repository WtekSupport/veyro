import { convertFileSrc } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  analyzeSpeechAnalysisFile,
  buildSpeechAnalysisCache,
  copyTextToClipboard,
  downloadLocalSttVariant,
  EVENTS,
  type SpeechAnalysisModelPolicy,
  type SpeechAnalysisOptions,
  type SpeechModelStatusDto,
  exportSpeechAnalysisReport,
  listSpeechAnalysisModels,
  speechAnalysisResolvePlan,
  speechAnalysisLoadDiskCache,
  type SpeechAnalysisPlanDto,
  normalizeLocalSttFamily,
  pickSpeechAnalysisSavePath,
  pickVoiceFiles,
  type LocalSttFamily,
  type LocalSttQuant,
  type SpeechAnalysisProgressPayload,
  type SpeechAnalysisProgressPhase,
  type SpeechAnalysisReport,
  type SpeechAnalysisSummary,
  type SpeechAnalysisSummaryDimension,
  type SpeechAnalysisProblems,
  type WhisperModelDownloadProgress,
} from "../api";
import { formatToolErrorForResultField } from "../lib/tool-error-display";
import { runToolWithSttLanguageRecovery } from "../lib/tool-stt-auto-recovery";
import { escapeHtml } from "../lib/html";
import {
  modelDownloadProgressPercent,
  patchModelDownloadProgressDom,
  renderModelDownloadProgressBlock,
} from "../lib/model-download-progress";
import { pathsMatch } from "../lib/tool-file-queue";
import { ToolDecodeProgressSmoother } from "../lib/tool-decode-progress";
import { t } from "../i18n";
import { localSttFamilyPlateName } from "./model-labels";

type JobStatus = "pending" | "processing" | "done" | "error";

interface AnalysisJob {
  id: string;
  path: string;
  pathKey: string | null;
  fileName: string;
  status: JobStatus;
  phase: SpeechAnalysisProgressPhase | null;
  percent: number | null;
  report: SpeechAnalysisReport | null;
  errorKey: string | null;
}

interface DownloadState {
  family: LocalSttFamily;
  quant: LocalSttQuant;
  progress: WhisperModelDownloadProgress | null;
  error: string | null;
}

const CTC_DOWNLOAD_ATTR = 'data-speech-analysis-ctc-download="active"';
const MODEL_POLICY_KEY = "speechAnalysis.modelPolicy";
const MANUAL_VARIANT_KEY = "speechAnalysis.manualVariant";
const COACH_ENABLED_KEY = "speechAnalysis.enableLlmCoach";

type ManualVariant = { family: LocalSttFamily; quant: LocalSttQuant };

function loadModelPolicy(): SpeechAnalysisModelPolicy {
  const raw = localStorage.getItem(MODEL_POLICY_KEY);
  if (raw === "follow_global" || raw === "manual") {
    return raw;
  }
  return "auto";
}

function saveModelPolicy(policy: SpeechAnalysisModelPolicy): void {
  localStorage.setItem(MODEL_POLICY_KEY, policy);
}

function loadLlmCoachEnabled(): boolean {
  const raw = localStorage.getItem(COACH_ENABLED_KEY);
  if (raw === "0" || raw === "false") {
    return false;
  }
  return true;
}

function saveLlmCoachEnabled(enabled: boolean): void {
  localStorage.setItem(COACH_ENABLED_KEY, enabled ? "1" : "0");
}

function loadManualVariant(): ManualVariant | null {
  const raw = localStorage.getItem(MANUAL_VARIANT_KEY);
  if (!raw) {
    return null;
  }
  try {
    const parsed = JSON.parse(raw) as ManualVariant;
    if (parsed?.family && parsed?.quant) {
      return parsed;
    }
  } catch {
    /* ignore */
  }
  return null;
}

function saveManualVariant(variant: ManualVariant): void {
  localStorage.setItem(MANUAL_VARIANT_KEY, JSON.stringify(variant));
}

function variantKey(variant: ManualVariant): string {
  return `${variant.family}:${variant.quant}`;
}

function modelOptionLabel(entry: SpeechModelStatusDto): string {
  const name = localSttFamilyPlateName(entry.family);
  return entry.quant === "legacy" ? name : `${name} (${entry.quant})`;
}

function analysisOptions(
  policy: SpeechAnalysisModelPolicy,
  manualVariant: ManualVariant | null,
  enableLlmCoach: boolean,
  extra: Partial<SpeechAnalysisOptions> = {},
): SpeechAnalysisOptions {
  return {
    modelPolicy: policy,
    autoDownloadModels: policy === "auto",
    manualVariant: policy === "manual" ? manualVariant : null,
    enableLlmCoach: enableLlmCoach ? null : false,
    ...extra,
  };
}

function jobNeedsMetricBackfill(report: SpeechAnalysisReport): boolean {
  if (report.articulation.missingCtcDownload) {
    return true;
  }
  const confidence = report.summary.dimensions.find((d) => d.id === "confidence");
  const intelligibility = report.summary.dimensions.find(
    (d) => d.id === "intelligibility",
  );
  const key = "tools.speechAnalysis.summary.reason.wordConfidenceUnavailable";
  return (
    confidence?.reasonKey === key || intelligibility?.reasonKey === key
  );
}

function stageLabel(phase: SpeechAnalysisProgressPhase | null): string {
  switch (phase) {
    case "decoding":
      return t("tools.speechAnalysis.stage.decoding");
    case "downloading_model":
      return t("tools.speechAnalysis.stage.downloadingModel");
    case "transcribing":
      return t("tools.speechAnalysis.stage.transcribing");
    case "analyzing":
      return t("tools.speechAnalysis.stage.analyzing");
    case "interpreting":
      return t("tools.speechAnalysis.stage.interpreting");
    case "done":
    case null:
      return t("tools.speechAnalysis.status.processing");
  }
}

function expressivenessLabel(
  value: SpeechAnalysisReport["prosody"]["expressiveness"],
): string {
  switch (value) {
    case "monotone":
      return t("tools.speechAnalysis.expressiveness.monotone");
    case "expressive":
      return t("tools.speechAnalysis.expressiveness.expressive");
    default:
      return t("tools.speechAnalysis.expressiveness.moderate");
  }
}

function qcFlagsLabel(flags: string[]): string {
  if (flags.length === 0) {
    return "—";
  }
  return flags
    .map((flag) => {
      const key = `tools.speechAnalysis.qc.${flagToQcKey(flag)}` as Parameters<
        typeof t
      >[0];
      const translated = t(key);
      return translated === key ? flag : translated;
    })
    .join(" · ");
}

function flagToQcKey(flag: string): string {
  return flag.replace(/_([a-z])/g, (_, ch: string) => ch.toUpperCase());
}

function caveatLabel(code: string): string {
  const key = `tools.speechAnalysis.caveat.${code}` as Parameters<typeof t>[0];
  const translated = t(key);
  return translated === key ? code : translated;
}

function dimensionTitle(id: string): string {
  const key = `tools.speechAnalysis.summary.dimension.${id}` as Parameters<
    typeof t
  >[0];
  const translated = t(key);
  return translated === key ? id : translated;
}

function missingAxisLabel(
  id: string,
  coverage: NonNullable<SpeechAnalysisSummary["overallCoverage"]>,
  report: SpeechAnalysisReport,
): string {
  const title = dimensionTitle(id);
  if (id === "confidence" && coverage.includedIds.includes("intelligibility")) {
    return `${title} (${t("tools.speechAnalysis.summary.missingReason.confidenceMerged")})`;
  }
  if (id === "articulation") {
    const dim = report.summary.dimensions.find((d) => d.id === "articulation");
    if (
      dim?.status === "insufficient_data" &&
      dim.reasonKey ===
        "tools.speechAnalysis.summary.reason.needsLongerRecording"
    ) {
      return `${title} (${t("tools.speechAnalysis.summary.missingReason.articulationShortSpeech")})`;
    }
  }
  return title;
}

function formatCoverageBreakdown(report: SpeechAnalysisReport): string {
  const cov = report.summary.overallCoverage;
  if (!cov || cov.includedIds.length === 0) {
    return "";
  }
  const includedList = cov.includedIds.map((id) => dimensionTitle(id)).join(", ");
  const missingList = cov.missingIds
    .map((id) => missingAxisLabel(id, cov, report))
    .join(", ");
  return t("tools.speechAnalysis.summary.coverageBreakdown", {
    includedList,
    missingList: missingList || "—",
  });
}

function weakSpotHtml(summary: SpeechAnalysisSummary): string {
  const id = summary.overallWeakSpotId;
  if (!id) {
    return "";
  }
  const dim = summary.dimensions.find((d) => d.id === id);
  if (dim?.score == null) {
    return "";
  }
  const key =
    dim.score < 70
      ? "tools.speechAnalysis.summary.weakSpot"
      : "tools.speechAnalysis.summary.lowestAxis";
  return `<p class="speech-analysis-weak-spot muted">${escapeHtml(
    t(key, {
      name: dimensionTitle(id),
      score: dim.score,
    }),
  )}</p>`;
}

function articulationPendingHero(report: SpeechAnalysisReport): string {
  const art = report.summary.dimensions.find((d) => d.id === "articulation");
  if (
    art?.status !== "insufficient_data" ||
    art.reasonKey !==
      "tools.speechAnalysis.summary.reason.needsLongerRecording"
  ) {
    return "";
  }
  const actual = Math.round(report.qc.speechDurationMs / 1000);
  const required = art.reasonRequiredSec ?? 120;
  const remaining = Math.max(0, required - actual);
  return `<p class="speech-analysis-articulation-pending muted">${escapeHtml(
    t("tools.speechAnalysis.summary.articulationNotScored", {
      remaining,
      actual,
      required,
    }),
  )}</p>`;
}

function renderFluencyMetricsList(report: SpeechAnalysisReport): string {
  const f = report.fluency;
  const speechSec = Math.max(0.1, report.qc.speechDurationMs / 1000);
  const articulationSec = Math.max(
    0.1,
    (report.qc.speechDurationMs - f.pauseTotalMs) / 1000,
  );
  const meanPause =
    f.pauseCount > 0 ? f.meanPauseMs.toFixed(0) : "—";
  const items: Array<[string, string]> = [
    [
      t("tools.speechAnalysis.summary.fluencyMetric.sylPerSec"),
      `${(f.syllableCount / speechSec).toFixed(1)}`,
    ],
    [
      t("tools.speechAnalysis.summary.fluencyMetric.sylPerSecArticulation"),
      `${(f.syllableCount / articulationSec).toFixed(1)}`,
    ],
    [
      t("tools.speechAnalysis.summary.fluencyMetric.wpm"),
      f.wpmPhonation != null ? f.wpmPhonation.toFixed(0) : "—",
    ],
    [
      t("tools.speechAnalysis.summary.fluencyMetric.pauses"),
      t("tools.speechAnalysis.summary.fluencyMetric.pausesValue", {
        count: f.pauseCount,
        minMs: f.minPauseMs,
        meanMs: meanPause,
      }),
    ],
    [
      t("tools.speechAnalysis.summary.fluencyMetric.longPauses"),
      t("tools.speechAnalysis.summary.fluencyMetric.longPausesValue", {
        count: f.longPauseCount,
        minMs: f.longPauseMs,
      }),
    ],
    [
      t("tools.speechAnalysis.summary.fluencyMetric.midPhrasePauses"),
      String(f.pauseCountMidPhrase ?? 0),
    ],
    [
      t("tools.speechAnalysis.summary.fluencyMetric.punctuationPauses"),
      String(f.pauseCountPunctuation ?? 0),
    ],
    [
      t("tools.speechAnalysis.summary.fluencyMetric.veryLongPauses"),
      t("tools.speechAnalysis.summary.fluencyMetric.longPausesValue", {
        count: f.veryLongPauseCount ?? 0,
        minMs: f.veryLongPauseMs ?? 1500,
      }),
    ],
    [
      t("tools.speechAnalysis.summary.fluencyMetric.fillers"),
      f.fillersPer100Words.toFixed(1),
    ],
    [
      t("tools.speechAnalysis.summary.fluencyMetric.reps"),
      String(f.repetitionCount),
    ],
  ];
  return `<ul class="speech-analysis-metric-list">${items
    .map(
      ([label, value]) =>
        `<li><span class="speech-analysis-metric-label">${escapeHtml(label)}</span> ${escapeHtml(value)}</li>`,
    )
    .join("")}</ul>`;
}

function renderF0ContourSvg(report: SpeechAnalysisReport): string {
  const contour = report.prosody.f0Contour ?? [];
  const voiced = contour.filter((p) => p.hz != null && p.hz > 0);
  if (voiced.length < 2) {
    return "";
  }
  const hzValues = voiced.map((p) => p.hz!);
  const minHz = Math.min(...hzValues);
  const maxHz = Math.max(...hzValues);
  const span = Math.max(maxHz - minHz, 1);
  const t0 = voiced[0].timeMs;
  const t1 = voiced[voiced.length - 1].timeMs;
  const timeSpan = Math.max(t1 - t0, 1);
  const padL = 36;
  const padR = 8;
  const padT = 8;
  const padB = 22;
  const w = 640;
  const h = 140;
  const plotW = w - padL - padR;
  const plotH = h - padT - padB;
  const toX = (timeMs: number) =>
    padL + ((timeMs - t0) / timeSpan) * plotW;
  const toY = (hz: number) =>
    padT + plotH - ((hz - minHz) / span) * plotH;
  const points = voiced
    .map((p) => `${toX(p.timeMs).toFixed(1)},${toY(p.hz!).toFixed(1)}`)
    .join(" ");
  const midHz = (minHz + maxHz) * 0.5;
  const median = report.prosody.f0MedianHz ?? midHz;
  const stSpan =
    maxHz > 0 && minHz > 0
      ? (12 * Math.log2(maxHz / minHz)).toFixed(1)
      : "—";
  const xMid = padL + plotW / 2;
  return `<figure class="speech-analysis-f0-chart speech-analysis-f0-chart--large" aria-label="F0">
    <svg viewBox="0 0 ${w} ${h}" width="100%" height="${h}" role="img">
      <line x1="${padL}" y1="${padT + plotH}" x2="${padL + plotW}" y2="${padT + plotH}" stroke="currentColor" stroke-opacity="0.25" />
      <line x1="${padL}" y1="${padT}" x2="${padL}" y2="${padT + plotH}" stroke="currentColor" stroke-opacity="0.25" />
      <text x="${padL}" y="${h - 4}" fill="currentColor" fill-opacity="0.55" font-size="10">${escapeHtml(formatMs(t0))}</text>
      <text x="${xMid.toFixed(0)}" y="${h - 4}" text-anchor="middle" fill="currentColor" fill-opacity="0.55" font-size="10">${escapeHtml(formatMs(t0 + timeSpan / 2))}</text>
      <text x="${(padL + plotW).toFixed(0)}" y="${h - 4}" text-anchor="end" fill="currentColor" fill-opacity="0.55" font-size="10">${escapeHtml(formatMs(t1))}</text>
      <text x="4" y="${toY(maxHz).toFixed(0)}" fill="currentColor" fill-opacity="0.55" font-size="10">${maxHz.toFixed(0)} Hz</text>
      <text x="4" y="${toY(minHz).toFixed(0)}" fill="currentColor" fill-opacity="0.55" font-size="10">${minHz.toFixed(0)} Hz</text>
      <text x="4" y="${(padT + 8).toFixed(0)}" fill="currentColor" fill-opacity="0.45" font-size="9">~${stSpan} st</text>
      <polyline fill="none" stroke="currentColor" stroke-width="1.75" points="${points}" />
      <line x1="${padL}" y1="${toY(median)}" x2="${padL + plotW}" y2="${toY(median)}" stroke="currentColor" stroke-opacity="0.15" stroke-dasharray="4 3" />
    </svg>
    <figcaption class="muted">${escapeHtml(t("tools.speechAnalysis.summary.prosodyF0ChartCaption"))}</figcaption>
  </figure>`;
}

function renderVoiceQualityBlock(report: SpeechAnalysisReport): string {
  const vq = report.prosody.voiceQuality;
  if (!vq || vq.lowConfidence) {
    return "";
  }
  const parts: string[] = [];
  if (vq.jitterLocalPercent != null) {
    parts.push(
      t("tools.speechAnalysis.summary.voiceQuality.jitter", {
        value: vq.jitterLocalPercent.toFixed(2),
      }),
    );
  }
  if (vq.shimmerLocalPercent != null) {
    parts.push(
      t("tools.speechAnalysis.summary.voiceQuality.shimmer", {
        value: vq.shimmerLocalPercent.toFixed(2),
      }),
    );
  }
  if (vq.cppsDb != null) {
    parts.push(
      t("tools.speechAnalysis.summary.voiceQuality.cpps", {
        value: vq.cppsDb.toFixed(1),
      }),
    );
  }
  if (parts.length === 0) {
    return "";
  }
  return `<p class="speech-analysis-dimension-detail muted">${escapeHtml(parts.join(" · "))}</p>`;
}

function renderInteractiveTranscript(report: SpeechAnalysisReport): string {
  const problemStarts = new Set(
    report.summary.problems.problemWords.map((w) => w.startMs),
  );
  const words = (report.timedSegments ?? []).flatMap((seg) => seg.words ?? []);
  if (words.length === 0) {
    return `<pre class="speech-analysis-transcript">${escapeHtml(report.transcript || "—")}</pre>`;
  }
  const tokens = words.map((w) => {
    const issue = problemStarts.has(w.startMs);
    const cls = issue ? " speech-analysis-transcript-word--issue" : "";
    return `<button type="button" class="speech-analysis-transcript-word${cls}" data-play-start="${w.startMs}" data-play-end="${w.endMs}">${escapeHtml(w.text)}</button>`;
  });
  return `<p class="speech-analysis-transcript speech-analysis-transcript--interactive">${tokens.join(" ")}</p>`;
}

function renderTranscriptSection(report: SpeechAnalysisReport): string {
  return `<section class="speech-analysis-block speech-analysis-transcript-block">
    <h3>${escapeHtml(t("tools.speechAnalysis.sectionTranscript"))}</h3>
    <p class="muted">${escapeHtml(t("tools.speechAnalysis.transcript.clickHint"))}</p>
    ${renderInteractiveTranscript(report)}
  </section>`;
}

function gradeLabel(gradeKey: string): string {
  const translated = t(gradeKey as Parameters<typeof t>[0]);
  return translated === gradeKey ? gradeKey : translated;
}

function formatMs(ms: number): string {
  const totalSec = Math.max(0, Math.floor(ms / 1000));
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return `${min}:${sec.toString().padStart(2, "0")}`;
}

function reasonHowToKey(reasonKey: string): string {
  return `${reasonKey}.howTo`;
}

function dimensionReasonHtml(
  dimension: SpeechAnalysisSummaryDimension,
  report: SpeechAnalysisReport,
): string {
  if (!dimension.reasonKey) {
    return "";
  }
  const key = dimension.reasonKey as Parameters<typeof t>[0];
  let main = "";
  if (
    dimension.id === "articulation" &&
    dimension.reasonKey ===
      "tools.speechAnalysis.summary.reason.needsLongerRecording" &&
    dimension.reasonRequiredSec != null
  ) {
    const actual = Math.round(report.qc.speechDurationMs / 1000);
    const remaining = Math.max(0, dimension.reasonRequiredSec - actual);
    main = t("tools.speechAnalysis.summary.reason.articulationAwait", {
      actual,
      required: dimension.reasonRequiredSec,
      remaining,
    });
    return `<p class="speech-analysis-dimension-detail muted">${escapeHtml(main)}</p>`;
  }
  if (
    dimension.reasonKey ===
      "tools.speechAnalysis.summary.reason.needsLongerRecording" &&
    dimension.reasonRequiredSec != null
  ) {
    main = t("tools.speechAnalysis.summary.reason.recordingDuration", {
      actual: Math.round(report.qc.speechDurationMs / 1000),
      required: dimension.reasonRequiredSec,
    });
  } else if (
    dimension.reasonKey === "tools.speechAnalysis.summary.reason.needsMoreWords" &&
    dimension.reasonRequiredWords != null
  ) {
    main = t("tools.speechAnalysis.summary.reason.wordCount", {
      actual: report.fluency.wordCount,
      required: dimension.reasonRequiredWords,
    });
  } else if (
    dimension.reasonKey ===
      "tools.speechAnalysis.summary.reason.needsMoreArticulationTokens" &&
    dimension.reasonRequiredTokens != null
  ) {
    main = t("tools.speechAnalysis.summary.reason.articulationTokenCount", {
      actual: report.articulation.alignmentTokenCount,
      required: dimension.reasonRequiredTokens,
    });
  } else {
    const translated = t(key);
    main = translated === dimension.reasonKey ? dimension.reasonKey : translated;
  }
  const howToKey = reasonHowToKey(dimension.reasonKey) as Parameters<typeof t>[0];
  const howToRaw = t(howToKey);
  const howTo =
    howToRaw !== reasonHowToKey(dimension.reasonKey) ? howToRaw : "";
  const mainHtml = `<p class="speech-analysis-dimension-detail muted">${escapeHtml(main)}</p>`;
  const howToHtml = howTo
    ? `<p class="speech-analysis-dimension-hint muted">${escapeHtml(howTo)}</p>`
    : "";
  return `${mainHtml}${howToHtml}`;
}

function playFragmentButton(startMs: number, endMs: number): string {
  if (endMs <= startMs && startMs === 0) {
    return "";
  }
  const end = endMs > startMs ? endMs : startMs + 1500;
  return `<button type="button" class="btn btn-compact speech-analysis-play-fragment" data-play-start="${startMs}" data-play-end="${end}" aria-label="${escapeHtml(t("tools.speechAnalysis.playFragment"))}">▶</button>`;
}

function renderProblemsSection(problems: SpeechAnalysisProblems): string {
  const hasWords = problems.problemWords.length > 0;
  const hasReps = problems.repetitionExamples.length > 0;
  if (!hasWords && !hasReps) {
    return "";
  }
  const words = hasWords
    ? `<h4>${escapeHtml(t("tools.speechAnalysis.problems.wordsTitle"))}</h4>
        <ul class="speech-analysis-problem-list">
          ${problems.problemWords
            .map(
              (w) =>
                `<li>${playFragmentButton(w.startMs, w.endMs)}<span class="speech-analysis-time">${escapeHtml(formatMs(w.startMs))}</span> ${escapeHtml(w.text)}${
                  w.scoreHint != null
                    ? ` <span class="muted">(${(w.scoreHint * 100).toFixed(1)}%)</span>`
                    : ""
                }${
                  w.context
                    ? ` <span class="muted">«${escapeHtml(w.context)}»</span>`
                    : ""
                }</li>`,
            )
            .join("")}
        </ul>`
    : "";
  const sourceHint = `<p class="speech-analysis-problems-source muted">${escapeHtml(
    t(problems.sourceKey as Parameters<typeof t>[0]),
  )}</p>
  <p class="speech-analysis-problems-source muted">${escapeHtml(
    t("tools.speechAnalysis.problems.uncertaintyDisclaimer"),
  )}</p>`;
  const reps = hasReps
    ? `<h4>${escapeHtml(t("tools.speechAnalysis.problems.repetitionsTitle"))}</h4>
        <ul class="speech-analysis-problem-list">${problems.repetitionExamples
          .map((r) => {
            const kind =
              r.kind === "emphasis"
                ? t("tools.speechAnalysis.problems.repetitionEmphasis")
                : t("tools.speechAnalysis.problems.repetitionStutter");
            const time =
              r.startMs > 0 || r.endMs > 0
                ? `<span class="speech-analysis-time">${escapeHtml(formatMs(r.startMs))}</span> `
                : "";
            return `<li>${playFragmentButton(r.startMs, r.endMs)}${time}<strong>${escapeHtml(r.token)}</strong> — ${escapeHtml(r.context)} <span class="muted">(${escapeHtml(kind)})</span></li>`;
          })
          .join("")}</ul>`
    : "";
  return `
    <section class="speech-analysis-problems">
      <h3>${escapeHtml(t("tools.speechAnalysis.problems.sectionTitle"))}</h3>
      ${sourceHint}
      ${words}
      ${reps}
    </section>`;
}

function renderLetterMismatchTechnical(report: SpeechAnalysisReport): string {
  const art = report.articulation;
  if (!art.ctcVariantUsed && art.topSubstitutions.length === 0) {
    return "";
  }
  const baseline =
    art.letterBaselineErrorRatePercent != null
      ? Math.round(art.letterBaselineErrorRatePercent)
      : null;
  const baselineLine =
    baseline != null
      ? `<p class="muted">${escapeHtml(
          t("tools.speechAnalysis.technical.letterBaseline", { percent: baseline }),
        )}</p>`
      : "";
  const weakRows =
    art.weakSymbols.length > 0
      ? `<ul class="speech-analysis-problem-list">${art.weakSymbols
          .map((s) => {
            const pct = Math.round(s.errorRatePercent);
            const excess = Math.round(s.excessVsBaselinePercent);
            return `<li>${escapeHtml(
              t("tools.speechAnalysis.technical.weakSymbolRow", {
                symbol: s.symbol,
                errors: s.errorCount,
                count: s.tokenCount,
                percent: pct,
                excess,
              }),
            )}</li>`;
          })
          .join("")}</ul>`
      : `<p class="muted">${escapeHtml(t("tools.speechAnalysis.technical.noLetterIssuesAboveBaseline"))}</p>`;
  const subs =
    art.topSubstitutions.length > 0
      ? `<table class="speech-analysis-metrics"><tbody>${art.topSubstitutions
          .map((p) => {
            const left = p.expected.trim() || "—";
            const right = p.observed.trim() || "—";
            return `<tr><td>${escapeHtml(`${left} → ${right}: ${p.count}`)}</td></tr>`;
          })
          .join("")}</tbody></table>`
      : `<p class="muted">${escapeHtml(t("tools.speechAnalysis.technical.noSubstitutionsAboveThreshold"))}</p>`;
  return `
    <section class="speech-analysis-block">
      <h3>${escapeHtml(t("tools.speechAnalysis.technical.letterMismatchTitle"))}</h3>
      <p class="muted">${escapeHtml(t("tools.speechAnalysis.problems.ctcSelfConsistencySource"))}</p>
      ${baselineLine}
      <h4>${escapeHtml(t("tools.speechAnalysis.technical.letterOutliersTitle"))}</h4>
      ${weakRows}
      <h4>${escapeHtml(t("tools.speechAnalysis.problems.substitutionsTitle"))}</h4>
      ${subs}
    </section>`;
}

function renderMetricsTable(report: SpeechAnalysisReport): string {
  const rows: Array<[string, string]> = [
    [
      t("tools.speechAnalysis.sectionQc"),
      [
        report.qc.snrDbEstimate != null
          ? `SNR ~${report.qc.snrDbEstimate.toFixed(1)} dB`
          : "SNR —",
        `${(report.qc.clipRatio * 100).toFixed(2)}% clip`,
        report.qc.narrowband
          ? t("tools.speechAnalysis.qc.narrowband")
          : t("tools.speechAnalysis.technical.workingSampleRate", {
              hz: report.qc.sampleRateHz,
            }),
        report.qc.sourceFileSampleRateHz != null &&
        report.qc.sourceFileSampleRateHz !== report.qc.sampleRateHz
          ? t("tools.speechAnalysis.technical.sourceSampleRate", {
              hz: report.qc.sourceFileSampleRateHz,
            })
          : null,
        report.qc.flags.length > 0 ? qcFlagsLabel(report.qc.flags) : null,
      ]
        .filter((part): part is string => part != null && part.length > 0)
        .join(" · "),
    ],
    [
      t("tools.speechAnalysis.sectionFluency"),
      [
        t("tools.speechAnalysis.technical.words", { count: report.fluency.wordCount }),
        report.fluency.wpmPhonation != null
          ? `${report.fluency.wpmPhonation.toFixed(0)} wpm`
          : null,
        t("tools.speechAnalysis.technical.longPauses", {
          count: report.fluency.longPauseCount,
        }),
        t("tools.speechAnalysis.technical.fillers", {
          value: report.fluency.fillersPer100Words.toFixed(1),
        }),
      ]
        .filter(Boolean)
        .join(" · "),
    ],
    [
      t("tools.speechAnalysis.sectionProsody"),
      [
        report.prosody.f0MedianHz != null
          ? `F0 ${report.prosody.f0MedianHz.toFixed(0)} Hz`
          : null,
        `σ ${report.prosody.f0StdSemitones.toFixed(2)} st`,
        expressivenessLabel(report.prosody.expressiveness),
      ]
        .filter(Boolean)
        .join(" · "),
    ],
    [
      t("tools.speechAnalysis.sectionIntelligibility"),
      report.intelligibility.meanWordConfidence != null
        ? t("tools.speechAnalysis.technical.meanConfidence", {
            percent: (report.intelligibility.meanWordConfidence * 100).toFixed(0),
          })
        : "—",
    ],
    [
      t("tools.speechAnalysis.sectionArticulation"),
      report.articulation.ctcVariantUsed
        ? t("tools.speechAnalysis.technical.ctcModel", {
            model: report.articulation.ctcVariantUsed,
          })
        : report.articulation.unavailableReasonKey
          ? formatToolErrorForResultField(
              report.articulation.unavailableReasonKey,
              "tools.speechAnalysis.readFailed",
            )
          : [
              report.articulation.per != null
                ? `PER ${(report.articulation.per * 100).toFixed(1)}%`
                : null,
              report.articulation.meanGop != null
                ? `GOP ${report.articulation.meanGop.toFixed(2)}`
                : null,
            ]
              .filter(Boolean)
              .join(" · ") || "—",
    ],
  ];

  return `
    <table class="speech-analysis-metrics">
      <tbody>
        ${rows
          .map(
            ([label, value]) => `
          <tr>
            <th scope="row">${escapeHtml(label)}</th>
            <td>${escapeHtml(value)}</td>
          </tr>`,
          )
          .join("")}
      </tbody>
    </table>`;
}

function renderDimensionCard(
  dimension: SpeechAnalysisSummaryDimension,
  report: SpeechAnalysisReport,
  download: DownloadState | null,
  ctcDownloading: boolean,
  modelPolicy: SpeechAnalysisModelPolicy,
): string {
  const preliminary =
    dimension.status === "insufficient_data" && dimension.score != null;
  const score =
    dimension.score != null
      ? `${dimension.score}<span class="speech-analysis-score-denom">/100</span>${
          preliminary
            ? `<span class="speech-analysis-badge speech-analysis-badge--preliminary-card">${escapeHtml(t("tools.speechAnalysis.summary.badgePreliminary"))}</span>`
            : ""
        }`
      : `<span class="muted">—</span>`;
  const detail = dimension.detailKey
    ? t(dimension.detailKey as Parameters<typeof t>[0])
    : "";
  const isArticulation = dimension.id === "articulation";
  const missing = report.articulation.missingCtcDownload;
  let extra = "";
  if (
    dimension.id === "fluency" &&
    report.fluency.wordCount > 0
  ) {
    extra = renderFluencyMetricsList(report);
  }
  if (dimension.id === "prosody" && dimension.status === "available") {
    const p = report.prosody;
    const range =
      p.f0RangeSemitones != null ? p.f0RangeSemitones.toFixed(1) : "—";
    extra = `<p class="speech-analysis-dimension-detail muted">${escapeHtml(
      t("tools.speechAnalysis.summary.detail.prosodyValues", {
        std: p.f0StdSemitones.toFixed(2),
        voiced: (p.voicedFraction * 100).toFixed(0),
        range,
      }),
    )}</p>${renderF0ContourSvg(report)}${renderVoiceQualityBlock(report)}`;
  }
  if (isArticulation && missing && modelPolicy === "auto") {
    extra = `<p class="speech-analysis-card-hint muted">${escapeHtml(t("tools.speechAnalysis.models.autoDownloadHint"))}</p>`;
  } else if (isArticulation && missing && modelPolicy !== "auto") {
    const modelName = t("settings.sttFamilyGigaAmV3E2eCtc");
    extra = `
      <p class="speech-analysis-card-hint">${escapeHtml(t("tools.speechAnalysis.downloadCtcHint", { model: modelName }))}</p>
      <button type="button" class="btn btn-secondary btn-compact" data-download-ctc-model
        ${ctcDownloading ? "disabled" : ""}>
        ${escapeHtml(t("tools.speechAnalysis.downloadCtcModel"))}
      </button>
      ${
        download && ctcDownloading
          ? renderModelDownloadProgressBlock(
              download.progress ?? { downloaded: 0, total: null, percent: null },
              modelDownloadProgressPercent(download.progress),
              CTC_DOWNLOAD_ATTR,
            )
          : ""
      }
      ${
        download?.error
          ? `<p class="voice-files-stage-hint" role="alert">${escapeHtml(download.error)}</p>`
          : ""
      }`;
  }
  return `
    <article class="speech-analysis-dimension-card">
      <h4 class="speech-analysis-dimension-title">${escapeHtml(dimensionTitle(dimension.id))}</h4>
      <div class="speech-analysis-dimension-score">${score}</div>
      <p class="speech-analysis-dimension-grade">${escapeHtml(gradeLabel(dimension.gradeKey))}</p>
      ${detail ? `<p class="speech-analysis-dimension-detail muted">${escapeHtml(detail)}</p>` : ""}
      ${
        dimension.status !== "available" && dimension.reasonKey
          ? dimensionReasonHtml(dimension, report)
          : ""
      }
      ${extra}
    </article>`;
}

function renderCoachSection(report: SpeechAnalysisReport): string {
  const coach = report.coach;
  if (!coach) {
    return "";
  }
  const wrapCoach = (body: string, muted = false) =>
    `<details class="speech-analysis-coach${muted ? " speech-analysis-coach--muted" : ""}">
      <summary>${escapeHtml(t("tools.speechAnalysis.coach.expand"))}</summary>
      <div class="speech-analysis-coach-body">${body}</div>
    </details>`;

  if (coach.status === "skipped_unavailable") {
    const key = coach.failedMessageKey as Parameters<typeof t>[0] | undefined;
    const msg = key ? t(key) : t("tools.speechAnalysis.coach.unavailable");
    return wrapCoach(
      `<p class="muted">${escapeHtml(msg)}</p>`,
      true,
    );
  }
  if (coach.status === "failed") {
    const detail = coach.failedMessageKey
      ? formatToolErrorForResultField(
          coach.failedMessageKey,
          "tools.speechAnalysis.coach.failed",
        )
      : t("tools.speechAnalysis.coach.failed");
    const preview =
      coach.summary.trim().length > 0
        ? `<details class="speech-analysis-coach-raw"><summary class="muted">${escapeHtml(t("tools.speechAnalysis.coach.rawResponse"))}</summary><pre>${escapeHtml(coach.summary)}</pre></details>`
        : "";
    return wrapCoach(
      `<p class="voice-files-stage-hint" role="alert">${escapeHtml(detail)}</p>${preview}`,
    );
  }
  return wrapCoach(
    `<p class="speech-analysis-coach-disclaimer muted">${escapeHtml(t("tools.speechAnalysis.coach.disclaimer"))}</p>
    <p>${escapeHtml(coach.summary)}</p>`,
  );
}

function renderPlanBanner(
  plan: SpeechAnalysisPlanDto | null,
  modelPolicy: SpeechAnalysisModelPolicy,
  speechModels: SpeechModelStatusDto[],
): string {
  if (modelPolicy !== "auto" || !plan || plan.toDownload.length === 0) {
    return "";
  }
  const first = plan.toDownload[0];
  const name = localSttFamilyPlateName(first.family);
  const entry = speechModels.find(
    (m) => m.family === first.family && m.quant === first.quant,
  );
  const sizeLabel =
    entry != null && entry.sizeMb > 0 ? ` (~${entry.sizeMb} MB)` : "";
  return `<p class="speech-analysis-plan-banner muted">${escapeHtml(
    t("tools.speechAnalysis.models.autoDownloadHint"),
  )} (${escapeHtml(name)}${escapeHtml(sizeLabel)})</p>`;
}

function renderVisualReport(
  report: SpeechAnalysisReport,
  audioPath: string | null,
  download: DownloadState | null,
  ctcDownloading: boolean,
  modelPolicy: SpeechAnalysisModelPolicy,
): string {
  const summary = report.summary;
  const showScore =
    summary.overallScoreMode !== "hidden" && summary.overallScore != null;
  const scoreMuted =
    summary.overallScoreMode === "preliminary" || !summary.overallShowGrade;
  const overall = showScore
    ? `<span class="speech-analysis-overall-value${scoreMuted ? " speech-analysis-overall-value--muted" : ""}">${summary.overallScore}</span><span class="speech-analysis-overall-denom">/100</span>`
    : `<span class="muted">${escapeHtml(t("tools.speechAnalysis.summary.noScore"))}</span>`;
  const modeBadge =
    summary.overallScoreMode === "preliminary"
      ? `<span class="speech-analysis-badge">${escapeHtml(t("tools.speechAnalysis.summary.badgePreliminary"))}</span>`
      : summary.overallScoreMode === "full"
        ? `<span class="speech-analysis-badge speech-analysis-badge--full">${escapeHtml(t("tools.speechAnalysis.summary.badgeFull"))}</span>`
        : "";
  const coverage = summary.overallCoverage
    ? `<p class="speech-analysis-coverage muted">${escapeHtml(
        t("tools.speechAnalysis.summary.coverage", {
          included: summary.overallCoverage.included,
          total: summary.overallCoverage.total,
        }),
      )}</p>
      ${
        formatCoverageBreakdown(report)
          ? `<p class="speech-analysis-coverage-breakdown muted">${escapeHtml(formatCoverageBreakdown(report))}</p>`
          : ""
      }`
    : "";
  const weakSpot = weakSpotHtml(summary);
  const reliability = `<p class="speech-analysis-reliability">${escapeHtml(t("tools.speechAnalysis.reliability.title"))}: ${escapeHtml(gradeLabel(summary.reliability.labelKey))} (${summary.reliability.score}/100)</p>`;
  const caveats =
    summary.caveats.length > 0
      ? `<ul class="speech-analysis-caveats">
          ${summary.caveats.map((c) => `<li>${escapeHtml(caveatLabel(c.code))}</li>`).join("")}
        </ul>`
      : "";

  const problemsBlock = renderProblemsSection(summary.problems);

  return `
    <div class="speech-analysis-report">
      <section class="speech-analysis-hero" aria-label="${escapeHtml(t("tools.speechAnalysis.summary.overallTitle"))}">
        <p class="speech-analysis-hero-label">${escapeHtml(gradeLabel(summary.overallLabelKey))} ${modeBadge}</p>
        <div class="speech-analysis-overall">${overall}</div>
        ${weakSpot}
        <p class="speech-analysis-overall-grade">${
          showScore && summary.overallShowGrade
            ? escapeHtml(gradeLabel(summary.overallGradeKey))
            : showScore
              ? escapeHtml(t("tools.speechAnalysis.summary.verdictPending"))
              : escapeHtml(t("tools.speechAnalysis.summary.hiddenHint"))
        }</p>
        ${coverage}
        ${articulationPendingHero(report)}
        ${reliability}
      </section>
      <div class="speech-analysis-dimension-grid">
        ${summary.dimensions
          .map((dim) =>
            renderDimensionCard(dim, report, download, ctcDownloading, modelPolicy),
          )
          .join("")}
      </div>
      ${renderCoachSection(report)}
      ${problemsBlock}
      ${renderTranscriptSection(report)}
      ${caveats}
      <details class="speech-analysis-technical">
        <summary>${escapeHtml(t("tools.speechAnalysis.technicalDetails"))}</summary>
        ${renderMetricsTable(report)}
        ${renderLetterMismatchTechnical(report)}
        ${renderTranscriptSection(report)}
      </details>
      <div class="speech-analysis-actions">
        <button type="button" class="btn secondary" data-copy-report>${escapeHtml(t("tools.speechAnalysis.copyMd"))}</button>
        <button type="button" class="btn secondary" data-export-report>${escapeHtml(t("tools.speechAnalysis.exportMd"))}</button>
        ${
          report.articulation.missingCtcDownload && !ctcDownloading
            ? `<button type="button" class="btn secondary" data-run-analysis>${escapeHtml(t("tools.speechAnalysis.retryAfterDownload"))}</button>`
            : ""
        }
      </div>
      ${
        audioPath
          ? `<audio class="speech-analysis-audio" data-speech-analysis-audio preload="metadata" src="${escapeHtml(convertFileSrc(audioPath))}"></audio>`
          : ""
      }
    </div>`;
}

function renderProcessingMain(
  job: AnalysisJob,
  visiblePercent: number | null,
): string {
  const stage = stageLabel(job.phase);
  const percent = visiblePercent ?? job.percent ?? 0;
  return `
    <div class="speech-analysis-processing">
      <div class="voice-files-progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${percent}">
        <div class="voice-files-progress-fill" style="width: ${percent}%"></div>
      </div>
      <p class="speech-analysis-stage">${escapeHtml(stage)}</p>
      <p class="voice-files-stage-hint">${escapeHtml(t("tools.speechAnalysis.processingHint"))}</p>
    </div>`;
}

let fragmentStopHandler: (() => void) | null = null;

function renderAdvancedModelControls(options: {
  modelPolicy: SpeechAnalysisModelPolicy;
  speechModels: SpeechModelStatusDto[];
  manualVariant: ManualVariant | null;
  enableLlmCoach: boolean;
}): string {
  const manualSelect =
    options.modelPolicy === "manual" && options.speechModels.length > 0
      ? `<label class="speech-analysis-advanced-label">
            ${escapeHtml(t("tools.speechAnalysis.advanced.manualVariant"))}
            <select data-speech-analysis-manual-variant class="speech-analysis-advanced-select">
              ${options.speechModels
                .map((entry) => {
                  const selected =
                    options.manualVariant != null &&
                    entry.family === options.manualVariant.family &&
                    entry.quant === options.manualVariant.quant;
                  return `<option value="${escapeHtml(variantKey({ family: entry.family, quant: entry.quant }))}"${selected ? " selected" : ""}>${escapeHtml(modelOptionLabel(entry))}</option>`;
                })
                .join("")}
            </select>
          </label>`
      : "";
  return `
        <details class="speech-analysis-advanced">
          <summary>${escapeHtml(t("tools.speechAnalysis.advanced.title"))}</summary>
          <label class="speech-analysis-advanced-label">
            ${escapeHtml(t("tools.speechAnalysis.advanced.modelPolicy"))}
            <select data-speech-analysis-model-policy class="speech-analysis-advanced-select">
              <option value="auto"${options.modelPolicy === "auto" ? " selected" : ""}>${escapeHtml(t("tools.speechAnalysis.advanced.policyAuto"))}</option>
              <option value="follow_global"${options.modelPolicy === "follow_global" ? " selected" : ""}>${escapeHtml(t("tools.speechAnalysis.advanced.policyFollowGlobal"))}</option>
              <option value="manual"${options.modelPolicy === "manual" ? " selected" : ""}>${escapeHtml(t("tools.speechAnalysis.advanced.policyManual"))}</option>
            </select>
          </label>
          ${manualSelect}
          <label class="speech-analysis-advanced-label speech-analysis-advanced-check">
            <input type="checkbox" data-speech-analysis-llm-coach${options.enableLlmCoach ? " checked" : ""} />
            ${escapeHtml(t("tools.speechAnalysis.advanced.llmCoach"))}
          </label>
          <p class="muted speech-analysis-advanced-hint">${escapeHtml(t("tools.speechAnalysis.advanced.hint"))}</p>
        </details>`;
}

function render(host: HTMLElement, jobs: AnalysisJob[], selectedId: string | null, options: {
  visiblePercent: number | null;
  download: DownloadState | null;
  ctcDownloading: boolean;
  modelPolicy: SpeechAnalysisModelPolicy;
  speechModels: SpeechModelStatusDto[];
  manualVariant: ManualVariant | null;
  backfilling: boolean;
  enableLlmCoach: boolean;
  planPreview: SpeechAnalysisPlanDto | null;
}): void {
  const selected = jobs.find((job) => job.id === selectedId) ?? jobs[0] ?? null;
  const processing = jobs.some((job) => job.status === "processing");
  const listRows =
    jobs.length === 0
      ? `<li class="voice-files-browser-empty">${escapeHtml(t("tools.speechAnalysis.noJobs"))}</li>`
      : jobs
            .map((job) => {
              const active = job.id === selected?.id;
              let status = t("tools.speechAnalysis.status.pending");
              if (job.status === "processing") {
                const stage = stageLabel(job.phase);
                status =
                  job.percent != null ? `${stage} ${job.percent}%` : stage;
              } else if (job.status === "done") {
                status = t("tools.speechAnalysis.status.done");
              } else if (job.status === "error") {
                status = t("tools.speechAnalysis.status.error");
              }
              return `
              <li class="voice-files-browser-row${active ? " voice-files-browser-row--selected" : ""}">
                <button type="button" class="voice-files-browser-main" data-select-job="${escapeHtml(job.id)}">
                  <span class="voice-files-browser-name">${escapeHtml(job.fileName)}</span>
                  <span class="voice-files-browser-status">${escapeHtml(status)}</span>
                </button>
              </li>`;
            })
            .join("");

  let main = "";
  if (selected?.status === "error" && selected.errorKey) {
    main = `<p class="error">${escapeHtml(formatToolErrorForResultField(selected.errorKey, "tools.speechAnalysis.readFailed"))}</p>`;
  } else if (selected?.status === "processing") {
    main = renderProcessingMain(selected, options.visiblePercent);
  } else if (selected?.status === "done" && selected.report) {
    main = renderVisualReport(
      selected.report,
      selected.path,
      options.download,
      options.ctcDownloading,
      options.modelPolicy,
    );
  } else if (selected) {
    main = `<p class="voice-files-hint">${escapeHtml(t("tools.speechAnalysis.readyToAnalyze"))}</p>`;
  } else {
    main = `<p class="voice-files-hint">${escapeHtml(t("tools.speechAnalysis.noJobs"))}</p>`;
  }

  const canAnalyze =
    Boolean(selected) && selected.status !== "processing" && !processing;

  host.innerHTML = `
    <main class="voice-files-tool voice-files-tool--with-browser">
      <aside class="voice-files-sidebar" aria-label="${escapeHtml(t("tools.speechAnalysis.sidebarTitle"))}">
        <div class="voice-files-sidebar-head">
          <span class="voice-files-sidebar-title">${escapeHtml(t("tools.speechAnalysis.sidebarTitle"))}</span>
        </div>
        <div
          class="voice-files-dropzone voice-files-dropzone--sidebar${processing ? " voice-files-dropzone--busy" : ""}"
          data-speech-analysis-dropzone
          tabindex="0"
          role="region"
          aria-label="${escapeHtml(t("tools.speechAnalysis.pickFiles"))}"
        >
          <button type="button" class="btn btn-secondary btn-compact" data-pick-files ${processing ? "disabled" : ""}>
            ${escapeHtml(t("tools.speechAnalysis.pickFiles"))}
          </button>
        </div>
        <ul class="voice-files-browser" role="list">${listRows}</ul>
        ${renderPlanBanner(options.planPreview, options.modelPolicy, options.speechModels)}
        ${renderAdvancedModelControls(options)}
        ${
          options.backfilling
            ? `<p class="speech-analysis-backfill-hint muted">${escapeHtml(t("tools.speechAnalysis.models.backfill"))}</p>`
            : ""
        }
        <div class="speech-analysis-sidebar-foot">
          <button type="button" class="btn btn-primary btn-compact" data-run-analysis ${canAnalyze ? "" : "disabled"}>
            ${escapeHtml(t("tools.speechAnalysis.analyze"))}
          </button>
        </div>
      </aside>
      <div class="voice-files-main">${main}</div>
    </main>`;
}

export function createSpeechAnalysisController(host: HTMLElement) {
  let jobs: AnalysisJob[] = [];
  let selectedId: string | null = null;
  let unlistenProgress: UnlistenFn | null = null;
  let unlistenDownload: UnlistenFn | null = null;
  let ctcDownloading = false;
  let download: DownloadState | null = null;
  let modelPolicy = loadModelPolicy();
  let manualVariant = loadManualVariant();
  let speechModels: SpeechModelStatusDto[] = [];
  let backfilling = false;
  let enableLlmCoach = loadLlmCoachEnabled();
  let planPreview: SpeechAnalysisPlanDto | null = null;
  const decodeProgress = new ToolDecodeProgressSmoother();

  const refreshPlanPreview = () => {
    if (modelPolicy !== "auto") {
      planPreview = null;
      paint();
      return;
    }
    void speechAnalysisResolvePlan(
      analysisOptions(modelPolicy, manualVariant, enableLlmCoach),
    ).then((plan) => {
      planPreview = plan;
      paint();
    });
  };

  void listSpeechAnalysisModels("ru").then((models) => {
    speechModels = models;
    if (modelPolicy === "manual" && !manualVariant && models.length > 0) {
      manualVariant = { family: models[0].family, quant: models[0].quant };
      saveManualVariant(manualVariant);
    }
    paint();
    refreshPlanPreview();
  });

  const paint = () => {
    const selected = jobs.find((job) => job.id === selectedId);
    const backendPercent =
      selected?.status === "processing" ? (selected.percent ?? null) : null;
    const visiblePercent = decodeProgress.displayPercent(
      selected?.phase ?? null,
      backendPercent,
    );
    render(host, jobs, selectedId, {
      visiblePercent,
      download,
      ctcDownloading,
      modelPolicy,
      speechModels,
      manualVariant,
      backfilling,
      enableLlmCoach,
      planPreview,
    });
  };

  decodeProgress.bindRepaint(paint);

  const backfillJobs = async () => {
    const targets = jobs.filter(
      (job) =>
        job.status === "done" &&
        job.report != null &&
        jobNeedsMetricBackfill(job.report),
    );
    if (targets.length === 0) {
      return;
    }
    backfilling = true;
    paint();
    for (const job of targets) {
      if (!job.report) {
        continue;
      }
      job.status = "processing";
      job.phase = "analyzing";
      job.percent = null;
      paint();
      try {
        const report = await runToolWithSttLanguageRecovery(() =>
          analyzeSpeechAnalysisFile(
            job.path,
            analysisOptions(modelPolicy, manualVariant, enableLlmCoach, {
              fillGapsOnly: true,
              cache: buildSpeechAnalysisCache(job.report!),
            }),
          ),
        );
        job.report = report;
        job.pathKey = report.pathKey;
        job.status = "done";
        job.phase = "done";
        job.percent = 100;
        job.errorKey = null;
      } catch (error) {
        job.status = "error";
        job.errorKey =
          typeof error === "string"
            ? error
            : "tools.speechAnalysis.readFailed";
      }
    }
    backfilling = false;
    decodeProgress.stop();
    paint();
  };

  const runJob = async (job: AnalysisJob) => {
    job.status = "processing";
    job.phase = "decoding";
    job.percent = 0;
    decodeProgress.reset();
    if (modelPolicy === "auto") {
      refreshPlanPreview();
    }
    paint();
    try {
      const report = await runToolWithSttLanguageRecovery(() =>
        analyzeSpeechAnalysisFile(
          job.path,
          analysisOptions(modelPolicy, manualVariant, enableLlmCoach),
        ),
      );
      job.report = report;
      job.pathKey = report.pathKey;
      job.status = "done";
      job.phase = "done";
      job.percent = 100;
      job.errorKey = null;
      if (jobNeedsMetricBackfill(report)) {
        await backfillJobs();
      }
    } catch (error) {
      job.status = "error";
      job.errorKey =
        typeof error === "string" ? error : "tools.speechAnalysis.readFailed";
    }
    decodeProgress.stop();
    paint();
  };

  const enqueue = (paths: string[]) => {
    for (const path of paths) {
      const fileName = path.split(/[/\\]/).pop() ?? path;
      const id = `${path}-${Date.now()}`;
      jobs.unshift({
        id,
        path,
        pathKey: null,
        fileName,
        status: "pending",
        phase: null,
        percent: null,
        report: null,
        errorKey: null,
      });
      selectedId = id;
    }
    paint();
    for (const path of paths) {
      void speechAnalysisLoadDiskCache(path).then((cached) => {
        if (!cached) {
          return;
        }
        const job = jobs.find(
          (entry) => pathsMatch(entry.path, path) || pathsMatch(entry.path, cached.pathKey),
        );
        if (!job || job.status === "processing") {
          return;
        }
        job.report = cached;
        job.pathKey = cached.pathKey;
        job.status = "done";
        job.phase = "done";
        job.percent = 100;
        paint();
      });
    }
  };

  const startCtcDownload = async (family: LocalSttFamily, quant: LocalSttQuant) => {
    ctcDownloading = true;
    download = {
      family,
      quant,
      progress: { downloaded: 0, total: null, percent: null },
      error: null,
    };
    paint();
    try {
      await downloadLocalSttVariant(family, quant);
      ctcDownloading = false;
      download = { family, quant, progress: null, error: null };
      await backfillJobs();
    } catch (error) {
      ctcDownloading = false;
      download = {
        family,
        quant,
        progress: null,
        error:
          typeof error === "string"
            ? error
            : t("tools.speechAnalysis.downloadFailed"),
      };
    }
    paint();
  };

  host.addEventListener("change", (event) => {
    const coachCheck = (event.target as HTMLElement).closest<HTMLInputElement>(
      "[data-speech-analysis-llm-coach]",
    );
    if (coachCheck) {
      enableLlmCoach = coachCheck.checked;
      saveLlmCoachEnabled(enableLlmCoach);
      paint();
      return;
    }
    const select = (event.target as HTMLElement).closest<HTMLSelectElement>(
      "[data-speech-analysis-model-policy]",
    );
    if (select) {
      const value = select.value;
      if (value === "auto" || value === "follow_global" || value === "manual") {
        modelPolicy = value;
        saveModelPolicy(modelPolicy);
        if (modelPolicy === "manual" && !manualVariant && speechModels.length > 0) {
          manualVariant = { family: speechModels[0].family, quant: speechModels[0].quant };
          saveManualVariant(manualVariant);
        }
        refreshPlanPreview();
      }
      return;
    }
    const manualSelect = (event.target as HTMLElement).closest<HTMLSelectElement>(
      "[data-speech-analysis-manual-variant]",
    );
    if (!manualSelect) {
      return;
    }
    const [family, quant] = manualSelect.value.split(":") as [LocalSttFamily, LocalSttQuant];
    if (family && quant) {
      manualVariant = { family, quant };
      saveManualVariant(manualVariant);
      refreshPlanPreview();
    }
  });

  host.addEventListener("click", async (event) => {
    const target = event.target as HTMLElement;
    const playBtn = target.closest<HTMLElement>("[data-play-start]");
    if (playBtn?.dataset.playStart != null) {
      const audio = host.querySelector<HTMLAudioElement>("[data-speech-analysis-audio]");
      if (audio) {
        fragmentStopHandler?.();
        fragmentStopHandler = null;
        const startSec = Number(playBtn.dataset.playStart) / 1000;
        const endSec = Number(playBtn.dataset.playEnd ?? playBtn.dataset.playStart) / 1000;
        const onTime = () => {
          if (audio.currentTime >= endSec) {
            audio.pause();
            audio.removeEventListener("timeupdate", onTime);
            fragmentStopHandler = null;
          }
        };
        fragmentStopHandler = () => audio.removeEventListener("timeupdate", onTime);
        audio.currentTime = Math.max(0, startSec);
        void audio.play();
        audio.addEventListener("timeupdate", onTime);
      }
      return;
    }
    if (target.closest("[data-pick-files]")) {
      const picked = await pickVoiceFiles();
      if (picked.length > 0) {
        enqueue(picked);
      }
      return;
    }
    const selectBtn = target.closest<HTMLElement>("[data-select-job]");
    if (selectBtn?.dataset.selectJob) {
      selectedId = selectBtn.dataset.selectJob;
      paint();
      return;
    }
    if (target.closest("[data-download-ctc-model]")) {
      const job = jobs.find((entry) => entry.id === selectedId);
      const hint = job?.report?.articulation.missingCtcDownload;
      if (hint && !ctcDownloading) {
        void startCtcDownload(
          normalizeLocalSttFamily(hint.family),
          hint.quant as LocalSttQuant,
        );
      }
      return;
    }
    if (target.closest("[data-run-analysis]")) {
      const job = jobs.find((entry) => entry.id === selectedId);
      if (job && job.status !== "processing") {
        void runJob(job);
      }
      return;
    }
    if (target.closest("[data-copy-report]")) {
      const job = jobs.find((entry) => entry.id === selectedId);
      if (job?.report) {
        void copyTextToClipboard(job.report.transcript);
      }
      return;
    }
    if (target.closest("[data-export-report]")) {
      const job = jobs.find((entry) => entry.id === selectedId);
      if (!job?.report) {
        return;
      }
      const savePath = await pickSpeechAnalysisSavePath(
        `${job.fileName.replace(/\.[^.]+$/, "")}-analysis.md`,
        "md",
      );
      if (savePath) {
        await exportSpeechAnalysisReport(savePath, job.report, "md");
      }
    }
  });

  host.addEventListener("dragover", (event) => {
    const zone = (event.target as HTMLElement).closest("[data-speech-analysis-dropzone]");
    if (!zone) {
      return;
    }
    event.preventDefault();
    zone.classList.add("voice-files-dropzone--hover");
  });
  host.addEventListener("dragleave", (event) => {
    const zone = (event.target as HTMLElement).closest("[data-speech-analysis-dropzone]");
    zone?.classList.remove("voice-files-dropzone--hover");
  });
  host.addEventListener("drop", (event) => {
    const zone = (event.target as HTMLElement).closest("[data-speech-analysis-dropzone]");
    if (!zone) {
      return;
    }
    event.preventDefault();
    zone.classList.remove("voice-files-dropzone--hover");
    const transfer = event.dataTransfer;
    if (!transfer?.files?.length) {
      return;
    }
    const paths = Array.from(transfer.files)
      .map((file) => (file as File & { path?: string }).path)
      .filter((path): path is string => Boolean(path));
    if (paths.length > 0) {
      enqueue(paths);
    }
  });

  void listen<SpeechAnalysisProgressPayload>(EVENTS.speechAnalysisProgress, (event) => {
    const payload = event.payload;
    const job = jobs.find(
      (entry) =>
        pathsMatch(entry.path, payload.path) ||
        (entry.pathKey != null && pathsMatch(entry.pathKey, payload.path)),
    );
    if (!job) {
      return;
    }
    job.phase = payload.phase;
    if (payload.percent != null) {
      job.percent = payload.percent;
    }
    decodeProgress.sync(job.phase, job.percent);
    paint();
  }).then((fn) => {
    unlistenProgress = fn;
  });

  void listen<WhisperModelDownloadProgress>(
    EVENTS.whisperModelDownloadProgress,
    (event) => {
      if (!ctcDownloading || !download) {
        return;
      }
      download = { ...download, progress: event.payload };
      patchModelDownloadProgressDom(host, event.payload, `[${CTC_DOWNLOAD_ATTR}]`);
      paint();
    },
  ).then((fn) => {
    unlistenDownload = fn;
  });

  paint();

  return {
    enqueue,
    destroy: () => {
      decodeProgress.dispose();
      void unlistenProgress?.();
      void unlistenDownload?.();
    },
  };
}
