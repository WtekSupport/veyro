import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import {
  type ActivityLogEntry,
  type AppStats,
  type DiagnosticsSnapshot,
  EVENTS,
  getAppStats,
  setResourceStatsEnabled,
} from "../api";
import type { InjectionMode } from "../api";
import type { SettingsFormValues } from "./settings";
import { llmModelPlateName, localSttModelPlateName } from "./model-labels";
import { renderActivityLog } from "./status";
import { lucideIcon } from "./lucide-icon";
import { Activity, Cpu, MemoryStick } from "lucide";
import { t, vadEngineDiagLabel, vadEngineShortLabel, whisperBackendLabel } from "../i18n";
import { bindVadGraphResize, resizeVadGraphCanvas } from "./vad-level-graph";
import { refreshVadGraphsFromSnapshot } from "./vad-monitor";

let statsUnlisten: UnlistenFn | null = null;
let resizeTeardown: (() => void) | null = null;
let cachedAppStats: AppStats = {
  cpu_percent: 0,
  memory_mb: 0,
  process_count: 0,
};

type StatusPlate = {
  label: string;
  value: string;
  active: boolean;
  title?: string;
};

export function renderStatusDashboard(
  values: SettingsFormValues,
  diagnostics: DiagnosticsSnapshot | null,
  activityLog: ActivityLogEntry[],
): string {
  const pushToTalk = values.push_to_talk;
  const vadEngine = diagnostics?.vad_engine ?? values.vad_engine;
  const metrics = formatMetricsDisplay(cachedAppStats);

  return `
    <div class="status-dashboard" data-status-dashboard>
      <div class="status-feature-plates" role="list" aria-label="${escapeHtml(t("status.featuresAria"))}">
        ${renderFeaturePlates(values, diagnostics, vadEngine)}
      </div>

      <div class="status-metrics-row">
        <div class="status-metric" title="${escapeHtml(t("status.cpuHint"))}">
          ${lucideIcon(Cpu)}
          <span class="status-metric-value" data-status-cpu>${escapeHtml(metrics.cpu)}</span>
        </div>
        <div class="status-metric" title="${escapeHtml(t("status.ramHint"))}">
          ${lucideIcon(MemoryStick)}
          <span class="status-metric-value" data-status-ram>${escapeHtml(metrics.ram)}</span>
        </div>
        <div class="status-metric status-metric--compact" title="${escapeHtml(t("status.processesHint"))}">
          ${lucideIcon(Activity)}
          <span class="status-metric-value" data-status-processes>${escapeHtml(metrics.processes)}</span>
        </div>
      </div>

      <section class="status-capture-section" data-status-capture-section>
        <div class="status-capture-head">
          <div class="status-capture-head-leading">
            <span class="status-capture-title">${escapeHtml(t("status.captureGraph"))}</span>
            ${renderVadEngineChip(vadEngine, diagnostics, pushToTalk)}
          </div>
          ${
            pushToTalk
              ? `<span class="status-capture-ptt-note">${escapeHtml(t("status.pttNoVad"))}</span>`
              : `<span class="status-capture-speech" data-vad-speech-indicator hidden>${escapeHtml(t("settings.vadSpeechDetected"))}</span>`
          }
          <span class="status-capture-level" data-status-level>0%</span>
        </div>
        <div class="status-capture-graph-wrap" data-vad-graph-wrap data-vad-graph-root>
          <canvas class="status-capture-graph" data-vad-graph aria-label="${escapeHtml(t("status.captureGraph"))}"></canvas>
        </div>
      </section>

      <details class="status-log-details">
        <summary>${escapeHtml(t("status.logExpand"))}</summary>
        ${renderActivityLog(activityLog)}
      </details>
    </div>
  `;
}

function renderFeaturePlates(
  values: SettingsFormValues,
  diagnostics: DiagnosticsSnapshot | null,
  vadEngine: string,
): string {
  const plates: StatusPlate[] = [];

  if (values.push_to_talk) {
    plates.push({
      label: t("status.platePtt"),
      value: values.global_hotkey || "—",
      active: Boolean(values.global_hotkey),
      title: values.ptt_hold ? t("settings.pttHold") : t("status.pttToggleHint", { hotkey: values.global_hotkey }),
    });
  } else {
    plates.push({
      label: t("status.plateMode"),
      value: t("settings.modeContinuous"),
      active: true,
    });
    plates.push({
      label: t("status.plateVad"),
      value: vadEngineShortLabel(vadEngine),
      active: true,
      title: t("settings.vadActiveChipTitle", { engine: vadEngineShortLabel(vadEngine) }),
    });
  }

  const sttLocal = values.transcription_provider === "local";
  const sttLoaded = sttLocal
    ? Boolean(diagnostics?.local_stt_loaded ?? diagnostics?.whisper_loaded)
    : Boolean(values.has_api_key);
  const sttTitle =
    sttLocal && diagnostics?.whisper_local_compiled
      ? whisperBackendLabel(diagnostics.whisper_backend)
      : undefined;
  plates.push({
    label: t("status.plateStt"),
    value: sttLocal ? localSttModelPlateName(values.local_stt_model) : t("status.plateCloudStt"),
    active: sttLoaded,
    title: sttTitle,
  });

  const rewriteLocal = values.text_rewrite_provider === "local";
  const llmLoaded = rewriteLocal
    ? Boolean(diagnostics?.llm_loaded)
    : Boolean(values.has_api_key);
  plates.push({
    label: t("status.plateLlm"),
    value: rewriteLocal ? llmModelPlateName(values.local_llm_model) : t("status.plateCloudLlm"),
    active: llmLoaded,
  });

  const inject = injectionPlateValue(values.injection_mode, diagnostics);
  plates.push({
    label: t("status.plateInject"),
    value: inject.value,
    active: inject.active,
    title: inject.title,
  });

  return plates
    .map(
      (plate) => `
        <article class="status-plate${plate.active ? " is-active" : " is-inactive"}" role="listitem" title="${escapeHtml(plate.title ?? plate.value)}">
          <span class="status-plate-label">${escapeHtml(plate.label)}</span>
          <span class="status-plate-value">${escapeHtml(plate.value)}</span>
        </article>
      `,
    )
    .join("");
}

function captureVadLabel(
  engine: string,
  diagnostics: DiagnosticsSnapshot | null,
): string {
  if (diagnostics?.vad_engine) {
    return vadEngineDiagLabel({
      vad_engine: diagnostics.vad_engine,
      vad_silero_runtime_ok: diagnostics.vad_silero_runtime_ok,
    });
  }
  return vadEngineShortLabel(engine);
}

function renderVadEngineChip(
  engine: string,
  diagnostics: DiagnosticsSnapshot | null,
  pushToTalk: boolean,
): string {
  const short = captureVadLabel(engine, diagnostics);
  const title = pushToTalk
    ? t("status.vadChipPttTitle", { engine: short })
    : t("settings.vadActiveChipTitle", { engine: short });
  const pttClass = pushToTalk ? " status-capture-vad-chip--ptt" : "";
  return `<span class="vad-active-chip status-capture-vad-chip${pttClass}" title="${escapeHtml(title)}" aria-label="${escapeHtml(title)}">${escapeHtml(short)}</span>`;
}

function injectionModeLabel(mode: InjectionMode): string {
  switch (mode) {
    case "paste":
      return t("settings.injectionPaste");
    case "keyboard":
      return t("settings.injectionKeyboard");
    default:
      return t("settings.injectionAuto");
  }
}

function injectionBackendLabel(backend: string): string {
  const key = `status.injectBackend.${backend}` as Parameters<typeof t>[0];
  const translated = t(key);
  return translated === key ? backend : translated;
}

function injectionPlateValue(
  mode: InjectionMode,
  diagnostics: DiagnosticsSnapshot | null,
): { value: string; active: boolean; title?: string } {
  const modeLabel = injectionModeLabel(mode);
  if (!diagnostics?.injection_available) {
    return {
      value: `${modeLabel} · ${t("status.plateInjectNa")}`,
      active: false,
      title: diagnostics?.injection_backend
        ? injectionBackendLabel(diagnostics.injection_backend)
        : undefined,
    };
  }
  const backendLabel = injectionBackendLabel(diagnostics.injection_backend);
  return {
    value: `${modeLabel} · ${backendLabel}`,
    active: true,
    title: diagnostics.injection_backend,
  };
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function formatCpu(value: number): string {
  return `${Math.round(value)}%`;
}

function formatRam(mb: number): string {
  if (mb >= 1024) {
    return `${(mb / 1024).toFixed(1)}G`;
  }
  return `${Math.round(mb)}M`;
}

function formatMetricsDisplay(stats: AppStats): {
  cpu: string;
  ram: string;
  processes: string;
} {
  return {
    cpu: formatCpu(stats.cpu_percent),
    ram: formatRam(stats.memory_mb),
    processes: String(stats.process_count),
  };
}

function updateMetricsDom(stats: AppStats): void {
  cachedAppStats = stats;
  const root = document.querySelector<HTMLElement>("[data-status-dashboard]");
  if (!root) {
    return;
  }

  const display = formatMetricsDisplay(stats);
  root.querySelector<HTMLElement>("[data-status-cpu]")!.textContent = display.cpu;
  root.querySelector<HTMLElement>("[data-status-ram]")!.textContent = display.ram;
  root.querySelector<HTMLElement>("[data-status-processes]")!.textContent = display.processes;
}

function handleAppStats(event: { payload: AppStats }): void {
  updateMetricsDom(event.payload);
}

async function refreshAppStatsNow(): Promise<void> {
  try {
    updateMetricsDom(await getAppStats());
  } catch {
    updateMetricsDom(cachedAppStats);
  }
}

export async function syncStatusDashboardLifecycle(active: boolean): Promise<void> {
  await setResourceStatsEnabled(active);

  if (active) {
    const root = document.querySelector<HTMLElement>("[data-status-dashboard]");
    if (root) {
      updateMetricsDom(cachedAppStats);
      void refreshAppStatsNow();
      resizeTeardown?.();
      resizeTeardown = bindVadGraphResize(root);
      root.querySelectorAll<HTMLCanvasElement>("[data-vad-graph]").forEach(resizeVadGraphCanvas);
      void refreshVadGraphsFromSnapshot();
    }
    if (!statsUnlisten) {
      statsUnlisten = await listen<AppStats>(EVENTS.appStats, handleAppStats);
    }
    void refreshAppStatsNow();
  } else {
    if (statsUnlisten) {
      await statsUnlisten();
      statsUnlisten = null;
    }
    resizeTeardown?.();
    resizeTeardown = null;
  }
}
