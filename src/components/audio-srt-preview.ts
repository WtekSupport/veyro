import { convertFileSrc } from "@tauri-apps/api/core";
import {
  formatPreviewClock,
  formatSrtClock,
  parseSrt,
  type ParsedSrtCue,
} from "../lib/parse-srt";
import { t } from "../i18n";

export interface AudioSrtPreviewUpdate {
  audioPath: string | null;
  srtText: string;
  globalOffsetMs: number;
  visible: boolean;
}

export interface AudioSrtPreviewController {
  update: (state: AudioSrtPreviewUpdate) => void;
  destroy: () => void;
}

function escapeAttr(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll('"', "&quot;");
}

export function createAudioSrtPreview(): AudioSrtPreviewController & {
  element: HTMLElement;
} {
  const root = document.createElement("section");
  root.className = "audio-srt-preview";
  root.setAttribute("aria-label", t("tools.audioSrt.previewTitle"));

  root.innerHTML = `
    <div class="audio-srt-preview-stage" aria-live="polite">
      <div class="audio-srt-preview-subtitle-slot">
        <p class="audio-srt-preview-subtitle" data-subtitle-text></p>
      </div>
      <p class="audio-srt-preview-subtitle-meta" data-subtitle-meta></p>
    </div>
    <div class="audio-srt-timeline" data-timeline role="slider" tabindex="0" aria-label="${escapeAttr(t("tools.audioSrt.previewTimeline"))}">
      <div class="audio-srt-timeline-track" data-track></div>
      <div class="audio-srt-timeline-playhead" data-playhead></div>
    </div>
    <div class="audio-srt-preview-controls">
      <button type="button" class="btn btn-secondary btn-compact audio-srt-preview-play" data-play aria-label="${escapeAttr(t("tools.audioSrt.previewPlay"))}">▶</button>
      <input type="range" class="audio-srt-preview-seek" data-seek min="0" max="1000" value="0" step="1" />
      <span class="audio-srt-preview-time" data-time>00:00.00 / 00:00.00</span>
    </div>
    <audio class="audio-srt-preview-audio" data-audio preload="metadata"></audio>
  `;

  const subtitleEl = root.querySelector<HTMLElement>("[data-subtitle-text]")!;
  const subtitleMetaEl = root.querySelector<HTMLElement>("[data-subtitle-meta]")!;
  const trackEl = root.querySelector<HTMLElement>("[data-track]")!;
  const timelineEl = root.querySelector<HTMLElement>("[data-timeline]")!;
  const playheadEl = root.querySelector<HTMLElement>("[data-playhead]")!;
  const playBtn = root.querySelector<HTMLButtonElement>("[data-play]")!;
  const seekEl = root.querySelector<HTMLInputElement>("[data-seek]")!;
  const timeEl = root.querySelector<HTMLElement>("[data-time]")!;
  const audio = root.querySelector<HTMLAudioElement>("[data-audio]")!;

  let cues: ParsedSrtCue[] = [];
  let loadedPath: string | null = null;
  let durationMs = 0;
  let rafId = 0;

  const currentMs = (): number => audio.currentTime * 1000;

  const activeCue = (timeMs: number): ParsedSrtCue | null => {
    for (const cue of cues) {
      if (timeMs >= cue.startMs && timeMs < cue.endMs) {
        return cue;
      }
    }
    return null;
  };

  const renderTimeline = (): void => {
    trackEl.replaceChildren();
    if (durationMs <= 0) {
      return;
    }
    for (const cue of cues) {
      const left = (cue.startMs / durationMs) * 100;
      const width = Math.max(((cue.endMs - cue.startMs) / durationMs) * 100, 0.4);
      const block = document.createElement("button");
      block.type = "button";
      block.className = "audio-srt-timeline-cue";
      block.style.left = `${left}%`;
      block.style.width = `${width}%`;
      block.title = `${formatSrtClock(cue.startMs)} --> ${formatSrtClock(cue.endMs)}`;
      block.dataset.cueStart = String(cue.startMs);
      block.addEventListener("click", (event) => {
        event.stopPropagation();
        seekToMs(Number(block.dataset.cueStart ?? 0));
      });
      trackEl.appendChild(block);
    }
  };

  const highlightActiveCue = (timeMs: number): void => {
    const blocks = trackEl.querySelectorAll<HTMLElement>(".audio-srt-timeline-cue");
    blocks.forEach((block) => {
      const start = Number(block.dataset.cueStart ?? 0);
      const cue = cues.find((item) => item.startMs === start);
      const active =
        cue !== undefined && timeMs >= cue.startMs && timeMs < cue.endMs;
      block.classList.toggle("audio-srt-timeline-cue--active", active);
    });
  };

  const updateSubtitle = (timeMs: number): void => {
    const cue = activeCue(timeMs);
    if (!cue) {
      subtitleEl.textContent = t("tools.audioSrt.previewNoSubtitle");
      subtitleEl.classList.add("audio-srt-preview-subtitle--empty");
      subtitleMetaEl.textContent = "";
      subtitleMetaEl.classList.add("audio-srt-preview-subtitle-meta--empty");
      return;
    }
    subtitleEl.classList.remove("audio-srt-preview-subtitle--empty");
    subtitleEl.innerHTML = cue.lines.map((line) => escapeHtml(line)).join("<br />");
    subtitleMetaEl.classList.remove("audio-srt-preview-subtitle-meta--empty");
    subtitleMetaEl.textContent = `#${cue.index} · ${formatSrtClock(cue.startMs)} --> ${formatSrtClock(cue.endMs)}`;
  };

  const escapeHtml = (value: string): string =>
    value
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;");

  const syncChrome = (): void => {
    const timeMs = currentMs();
    durationMs = Number.isFinite(audio.duration) ? audio.duration * 1000 : durationMs;
    const pct =
      durationMs > 0 ? Math.min(1000, Math.round((timeMs / durationMs) * 1000)) : 0;
    seekEl.value = String(pct);
    playheadEl.style.left = `${(pct / 1000) * 100}%`;
    timeEl.textContent = `${formatPreviewClock(timeMs)} / ${formatPreviewClock(durationMs)}`;
    highlightActiveCue(timeMs);
    updateSubtitle(timeMs);
  };

  const tick = (): void => {
    syncChrome();
    if (!audio.paused) {
      rafId = requestAnimationFrame(tick);
    }
  };

  const seekToMs = (ms: number): void => {
    if (!Number.isFinite(audio.duration) || audio.duration <= 0) {
      return;
    }
    audio.currentTime = Math.min(Math.max(0, ms / 1000), audio.duration);
    syncChrome();
  };

  playBtn.addEventListener("click", () => {
    if (audio.paused) {
      void audio.play();
      cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(tick);
      playBtn.textContent = "⏸";
      playBtn.setAttribute("aria-label", t("tools.audioSrt.previewPause"));
    } else {
      audio.pause();
      playBtn.textContent = "▶";
      playBtn.setAttribute("aria-label", t("tools.audioSrt.previewPlay"));
    }
  });

  seekEl.addEventListener("input", () => {
    if (durationMs <= 0) {
      return;
    }
    const ratio = Number(seekEl.value) / 1000;
    audio.currentTime = (durationMs * ratio) / 1000;
    syncChrome();
  });

  timelineEl.addEventListener("click", (event) => {
    if (durationMs <= 0) {
      return;
    }
    const rect = timelineEl.getBoundingClientRect();
    const ratio = (event.clientX - rect.left) / rect.width;
    seekToMs(durationMs * Math.max(0, Math.min(1, ratio)));
  });

  timelineEl.addEventListener("keydown", (event) => {
    if (durationMs <= 0) {
      return;
    }
    const step = event.shiftKey ? 5000 : 500;
    if (event.key === "ArrowRight") {
      event.preventDefault();
      seekToMs(currentMs() + step);
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      seekToMs(currentMs() - step);
    }
  });

  audio.addEventListener("loadedmetadata", () => {
    durationMs = audio.duration * 1000;
    renderTimeline();
    syncChrome();
  });

  audio.addEventListener("timeupdate", () => {
    if (audio.paused) {
      syncChrome();
    }
  });

  audio.addEventListener("ended", () => {
    playBtn.textContent = "▶";
    playBtn.setAttribute("aria-label", t("tools.audioSrt.previewPlay"));
    cancelAnimationFrame(rafId);
    syncChrome();
  });

  const update = (state: AudioSrtPreviewUpdate): void => {
    root.hidden = !state.visible;
    if (!state.visible) {
      audio.pause();
      return;
    }

    // SRT from the tool already includes global offset in timestamps — do not add twice.
    void state.globalOffsetMs;
    cues = parseSrt(state.srtText, 0);

    if (state.audioPath && state.audioPath !== loadedPath) {
      loadedPath = state.audioPath;
      audio.pause();
      audio.src = convertFileSrc(state.audioPath);
      audio.load();
      playBtn.textContent = "▶";
    }

    if (Number.isFinite(audio.duration) && audio.duration > 0) {
      durationMs = audio.duration * 1000;
      renderTimeline();
    }

    syncChrome();
  };

  const destroy = (): void => {
    cancelAnimationFrame(rafId);
    audio.pause();
    audio.removeAttribute("src");
    root.remove();
  };

  return {
    element: root,
    update,
    destroy,
  };
}

export function mountAudioSrtPreview(
  slot: HTMLElement,
  controller: AudioSrtPreviewController & { element: HTMLElement },
): void {
  if (controller.element.parentElement !== slot) {
    slot.replaceChildren(controller.element);
  }
}
