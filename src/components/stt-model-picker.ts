import {
  isWhisperSttFamily,
  normalizeLocalSttFamily,
  type LocalSttFamily,
  type LocalSttFamilyInfo,
  type LocalSttQuant,
  type LocalSttVariantInfo,
  type WhisperModelDownloadProgress,
} from "../api";
import { t } from "../i18n";
import type { MessageKey } from "../i18n/locales/en";
import {
  CircuitBoard,
  Crosshair,
  Gauge,
  Globe,
  HardDrive,
  MemoryStick,
  Zap,
  type IconNode,
} from "lucide";
import { lucideIcon } from "./lucide-icon";

const FAMILY_NAME_KEYS: Record<LocalSttFamily, MessageKey> = {
  whisper_base: "settings.sttFamilyWhisperBase",
  whisper_small: "settings.sttFamilyWhisperSmall",
  whisper_medium: "settings.sttFamilyWhisperMedium",
  whisper_large_v3_turbo: "settings.sttFamilyWhisperLargeV3Turbo",
  whisper_large_v3: "settings.sttFamilyWhisperLargeV3",
  parakeet_tdt_0_6b_v3: "settings.sttFamilyParakeet",
  qwen3_asr_0_6b: "settings.sttFamilyQwen3Asr06b",
  qwen3_asr_1_7b: "settings.sttFamilyQwen3Asr17b",
};

const WHISPER_QUANT_LABEL_KEYS: Record<LocalSttQuant, MessageKey> = {
  legacy: "settings.sttQuantLegacy",
  q4_0: "settings.sttQuantQ4",
  q5: "settings.sttQuantQ5",
  q8_0: "settings.sttQuantQ8",
  int8: "settings.sttQuantInt8",
  fp16: "settings.sttQuantFp16",
  fp32: "settings.sttQuantFp32",
};

const SHERPA_BUILD_LABEL_KEYS: Partial<Record<LocalSttQuant, MessageKey>> = {
  int8: "settings.sttSherpaBuildInt8",
  fp16: "settings.sttSherpaBuildFp16",
  fp32: "settings.sttSherpaBuildFp32",
};

function variantOptionLabel(family: LocalSttFamily, quant: LocalSttQuant): MessageKey {
  if (!isWhisperSttFamily(family)) {
    return SHERPA_BUILD_LABEL_KEYS[quant] ?? "settings.sttSherpaBuildInt8";
  }
  return WHISPER_QUANT_LABEL_KEYS[quant];
}

const FALLBACK_FAMILIES: LocalSttFamily[] = [
  "whisper_base",
  "whisper_small",
  "whisper_medium",
  "whisper_large_v3_turbo",
  "whisper_large_v3",
  "parakeet_tdt_0_6b_v3",
  "qwen3_asr_0_6b",
  "qwen3_asr_1_7b",
];

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function tierLabel(tier: number): string {
  const clamped = Math.max(1, Math.min(5, Math.round(tier)));
  return t("settings.sttSpecTierFormat", { tier: String(clamped) });
}

function clampTier(tier: number): number {
  return Math.max(1, Math.min(5, Math.round(tier)));
}

function specIcon(node: IconNode): string {
  return `<span class="stt-spec-icon" aria-hidden="true">${lucideIcon(node)}</span>`;
}

function diskSizeParts(spec: LocalSttVariantInfo): { label: string; value: string } {
  if (spec.disk_bytes !== null && spec.disk_bytes > 0) {
    const mb = spec.disk_bytes / (1024 * 1024);
    return {
      label: t("settings.sttSpecDiskLabel"),
      value: t("settings.sttSpecSizeMbValue", { size: mb.toFixed(0) }),
    };
  }
  return {
    label: t("settings.sttSpecDownloadLabel"),
    value: t("settings.sttSpecSizeMbValue", { size: String(spec.download_size_mb) }),
  };
}

function vramValue(spec: LocalSttVariantInfo): string {
  if (spec.vram_mb === null || spec.vram_mb <= 0) {
    return t("settings.sttSpecVramDash");
  }
  return t("settings.sttSpecVramValueShort", {
    size: String(Math.round(spec.vram_mb / 1024)),
  });
}

function parseFeatures(featuresText: string): {
  langValue: string;
  langSub: string;
  tagline: string;
} {
  const parts = featuresText.split(" · ").map((part) => part.trim()).filter(Boolean);
  const head = parts[0] ?? featuresText;
  const tagline = parts.slice(1).join(" · ").trim();
  const countMatch = head.match(/~?\d+/);
  if (countMatch && /язык|language/i.test(head)) {
    return {
      langValue: countMatch[0],
      langSub: t("settings.sttSpecLanguagesLabel"),
      tagline: tagline || head,
    };
  }
  return {
    langValue: head,
    langSub: "",
    tagline: tagline || head,
  };
}

function renderTierSegments(tier: number, tone: "speed" | "accuracy"): string {
  const on = clampTier(tier);
  return Array.from({ length: 5 }, (_, index) => {
    const lit = index < on;
    return `<span class="stt-tier-seg ${lit ? `is-on is-${tone}` : ""}"></span>`;
  }).join("");
}

export function readLocalSttQuantFromForm(form: HTMLFormElement): LocalSttQuant {
  const select = form.querySelector<HTMLSelectElement>('select[name="local_stt_quant"]');
  if (select && !select.disabled) {
    return (select.value || "legacy") as LocalSttQuant;
  }
  const hidden = form.querySelector<HTMLInputElement>(
    'input[type="hidden"][name="local_stt_quant"]',
  );
  if (hidden?.value) {
    return hidden.value as LocalSttQuant;
  }
  return "legacy";
}

export function quantsForFamilyFromCatalog(
  families: LocalSttFamilyInfo[],
  family: LocalSttFamily,
): LocalSttQuant[] {
  const normalized = normalizeLocalSttFamily(family);
  const entry = families.find(
    (item) => normalizeLocalSttFamily(item.family) === normalized,
  );
  if (entry && entry.quants.length > 0) {
    return entry.quants;
  }
  if (isWhisperSttFamily(normalized)) {
    return ["legacy", "q4_0", "q5", "q8_0"];
  }
  if (normalized === "parakeet_tdt_0_6b_v3") {
    return ["int8", "fp16", "fp32"];
  }
  return ["int8"];
}

export function renderSttVariantSpec(
  spec: LocalSttVariantInfo | null,
  variantDownloaded = false,
): string {
  if (!spec) {
    return `<div class="stt-variant-spec stt-variant-spec--empty" data-stt-variant-spec></div>`;
  }

  const featuresKey = spec.features_i18n_key as MessageKey;
  const ready = spec.exists || variantDownloaded;
  const disk = diskSizeParts(spec);
  const { langValue, langSub, tagline } = parseFeatures(t(featuresKey));
  const ramGb = String(Math.round(spec.ram_mb / 1024));
  const speedTier = clampTier(spec.speed_tier);
  const accuracyTier = clampTier(spec.accuracy_tier);

  return `
    <div class="stt-variant-spec" data-stt-variant-spec>
      <div class="stt-spec-top">
        <div class="stt-spec-cell stt-spec-disk">
          ${specIcon(HardDrive)}
          <div class="stt-spec-cell-body">
            <span class="stt-spec-label">${escapeHtml(disk.label)}</span>
            <span class="stt-spec-value">${escapeHtml(disk.value)}</span>
          </div>
        </div>
        <div class="stt-spec-mem-cols">
          <div class="stt-spec-cell stt-spec-ram">
            ${specIcon(MemoryStick)}
            <div class="stt-spec-cell-body">
              <span class="stt-spec-label">${escapeHtml(t("settings.sttSpecRamLabel"))}</span>
              <span class="stt-spec-value">${escapeHtml(t("settings.sttSpecRamValue", { size: ramGb }))}</span>
            </div>
          </div>
          <div class="stt-spec-cell stt-spec-vram">
            ${specIcon(CircuitBoard)}
            <div class="stt-spec-cell-body">
              <span class="stt-spec-label">${escapeHtml(t("settings.sttSpecVramGpuLabel"))}</span>
              <span class="stt-spec-value">${escapeHtml(vramValue(spec))}</span>
            </div>
          </div>
        </div>
      </div>
      <div class="stt-spec-bottom">
        <div class="stt-spec-metric stt-spec-speed">
          ${specIcon(Gauge)}
          <div class="stt-spec-metric-body">
            <span class="stt-spec-label">${escapeHtml(t("settings.sttSpecSpeed"))}</span>
            <span class="stt-spec-metric-value">${escapeHtml(tierLabel(speedTier))}</span>
            <div class="stt-tier-bar">${renderTierSegments(speedTier, "speed")}</div>
          </div>
        </div>
        <div class="stt-spec-metric stt-spec-accuracy">
          ${specIcon(Crosshair)}
          <div class="stt-spec-metric-body">
            <span class="stt-spec-label">${escapeHtml(t("settings.sttSpecAccuracy"))}</span>
            <span class="stt-spec-metric-value">${escapeHtml(tierLabel(accuracyTier))}</span>
            <div class="stt-tier-bar">${renderTierSegments(accuracyTier, "accuracy")}</div>
          </div>
        </div>
        <div class="stt-spec-metric stt-spec-lang">
          ${specIcon(Globe)}
          <div class="stt-spec-metric-body stt-spec-lang-body">
            <span class="stt-spec-value stt-spec-lang-value">${escapeHtml(langValue)}</span>
            ${
              langSub
                ? `<span class="stt-spec-lang-sub">${escapeHtml(langSub)}</span>`
                : ""
            }
          </div>
        </div>
        <div class="stt-spec-metric stt-spec-tagline">
          ${specIcon(Zap)}
          <p class="stt-spec-tagline-text">${escapeHtml(tagline)}</p>
        </div>
      </div>
      ${
        ready
          ? ""
          : `<p class="field-hint stt-spec-not-downloaded">${escapeHtml(t("settings.sttVariantNotDownloaded"))}</p>`
      }
    </div>
  `;
}

export function renderSttModelPicker(
  family: LocalSttFamily,
  quant: LocalSttQuant,
  families: LocalSttFamilyInfo[],
  spec: LocalSttVariantInfo | null,
  whisperModelDownload: WhisperModelDownloadProgress | null,
  downloadProgressPercent: number | null,
  variantDownloaded = false,
): string {
  const normalizedFamily = normalizeLocalSttFamily(family);
  const catalogFamilies =
    families.length > 0
      ? families.map((item) => normalizeLocalSttFamily(item.family))
      : FALLBACK_FAMILIES;
  const whisperFamilies = catalogFamilies.filter((f) => isWhisperSttFamily(f));
  const sherpaFamilies = catalogFamilies.filter((f) => !isWhisperSttFamily(f));
  const quants = quantsForFamilyFromCatalog(families, normalizedFamily);
  const effectiveQuant = quants.includes(quant) ? quant : (quants[0] ?? "legacy");
  const isSherpaFamily = !isWhisperSttFamily(normalizedFamily);
  const showQuant = quants.length > 1;
  const variantFieldLabel = isSherpaFamily
    ? t("settings.sttModelSherpaBuild")
    : t("settings.sttModelQuant");

  const familyOptions = (list: LocalSttFamily[]) =>
    list
      .map((item) => {
        const key = FAMILY_NAME_KEYS[item];
        return `<option value="${item}" ${item === normalizedFamily ? "selected" : ""}>${escapeHtml(t(key))}</option>`;
      })
      .join("");

  const quantOptions = quants
    .map((item) => {
      const key = variantOptionLabel(normalizedFamily, item);
      return `<option value="${item}" ${item === effectiveQuant ? "selected" : ""}>${escapeHtml(t(key))}</option>`;
    })
    .join("");

  const downloadBlock = renderSttDownloadAction(
    spec,
    whisperModelDownload,
    downloadProgressPercent,
    variantDownloaded,
  );

  return `
    <div class="stt-model-picker" data-stt-model-picker>
      <div class="stt-model-picker-row">
        <label class="field">
          <span>${escapeHtml(t("settings.sttModelFamily"))}</span>
          <select name="local_stt_family" data-stt-family-select>
            ${
              whisperFamilies.length > 0
                ? `<optgroup label="${escapeHtml(t("settings.sttGroupWhisper"))}">${familyOptions(whisperFamilies)}</optgroup>`
                : ""
            }
            ${
              sherpaFamilies.length > 0
                ? `<optgroup label="${escapeHtml(t("settings.sttGroupSherpa"))}">${familyOptions(sherpaFamilies)}</optgroup>`
                : ""
            }
          </select>
        </label>
        <label class="field ${showQuant ? "" : "hidden"}" data-stt-quant-field>
          <span data-stt-variant-field-label>${escapeHtml(variantFieldLabel)}</span>
          <select name="local_stt_quant" data-stt-quant-select ${showQuant ? "" : "disabled"}>
            ${quantOptions}
          </select>
        </label>
      </div>
      ${
        showQuant
          ? ""
          : `<input type="hidden" name="local_stt_quant" value="${escapeHtml(effectiveQuant)}" />`
      }
      ${renderSttVariantSpec(spec, variantDownloaded)}
      ${downloadBlock}
    </div>
  `;
}

function renderSttDownloadAction(
  spec: LocalSttVariantInfo | null,
  whisperModelDownload: WhisperModelDownloadProgress | null,
  downloadProgressPercent: number | null,
  variantDownloaded = false,
): string {
  const downloadingModel = whisperModelDownload !== null;
  if (downloadingModel) {
    const hint =
      whisperModelDownload.percent !== null && whisperModelDownload.percent >= 0
        ? t("settings.downloadingWhisper", {
            percent: Math.round(whisperModelDownload.percent),
          })
        : t("settings.downloadingWhisperUnknown", {
            downloaded: (whisperModelDownload.downloaded / (1024 * 1024)).toFixed(1),
          });
    return `
      <div class="whisper-download" data-whisper-download>
        <div class="field-row">
          <span class="field-hint" data-whisper-download-hint>${escapeHtml(hint)}</span>
        </div>
        <div
          class="download-progress"
          data-whisper-download-progress
          role="progressbar"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow="${downloadProgressPercent ?? 0}"
        >
          <div
            class="download-progress-bar ${
              downloadProgressPercent === null ? "is-indeterminate" : ""
            }"
            data-whisper-download-bar
            style="${
              downloadProgressPercent === null ? "" : `width: ${downloadProgressPercent}%;`
            }"
          ></div>
        </div>
      </div>
    `;
  }

  if (spec?.exists || variantDownloaded) {
    return "";
  }

  return `
    <div class="field-row model-action">
      <button type="button" class="btn-secondary" data-download-whisper-model>
        ${escapeHtml(t("settings.downloadWhisperModel"))}
      </button>
    </div>
  `;
}
