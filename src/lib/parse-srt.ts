export interface ParsedSrtCue {
  index: number;
  startMs: number;
  endMs: number;
  lines: string[];
  /** Optional speaker id from diarization (`cueSpeakerIds` alignment). */
  speakerId?: number | null;
}

const TIMESTAMP =
  /(\d{2}):(\d{2}):(\d{2})[,.](\d{3})\s*-->\s*(\d{2}):(\d{2}):(\d{2})[,.](\d{3})/;

function timestampToMs(h: string, m: string, s: string, ms: string): number {
  return (
    Number(h) * 3_600_000 +
    Number(m) * 60_000 +
    Number(s) * 1_000 +
    Number(ms)
  );
}

export function parseSrt(text: string, globalOffsetMs = 0): ParsedSrtCue[] {
  const normalized = text.replace(/\uFEFF/g, "").replace(/\r\n/g, "\n").trim();
  if (!normalized) {
    return [];
  }

  const blocks = normalized.split(/\n\s*\n/);
  const cues: ParsedSrtCue[] = [];

  for (const block of blocks) {
    const lines = block.split("\n").map((line) => line.trimEnd());
    if (lines.length < 2) {
      continue;
    }

    let lineIndex = 0;
    if (/^\d+$/.test(lines[0] ?? "")) {
      lineIndex = 1;
    }

    const timingLine = lines[lineIndex];
    const match = timingLine?.match(TIMESTAMP);
    if (!match) {
      continue;
    }

    const startMs =
      timestampToMs(match[1], match[2], match[3], match[4]) + globalOffsetMs;
    const endMs =
      timestampToMs(match[5], match[6], match[7], match[8]) + globalOffsetMs;
    const textLines = lines.slice(lineIndex + 1).filter((line) => line.length > 0);
    if (textLines.length === 0) {
      continue;
    }

    cues.push({
      index: cues.length + 1,
      startMs: Math.max(0, startMs),
      endMs: Math.max(startMs, endMs),
      lines: textLines,
    });
  }

  return cues;
}

export function formatPreviewClock(ms: number): string {
  const value = Math.max(0, Math.floor(ms));
  const minutes = Math.floor(value / 60_000);
  const seconds = Math.floor((value % 60_000) / 1_000);
  const millis = value % 1_000;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}.${String(millis).padStart(3, "0").slice(0, 2)}`;
}

/** Same units as SRT timestamp lines (for preview vs textarea). */
export function formatSrtClock(ms: number): string {
  const value = Math.max(0, Math.floor(ms));
  const hours = Math.floor(value / 3_600_000);
  const minutes = Math.floor((value % 3_600_000) / 60_000);
  const seconds = Math.floor((value % 60_000) / 1_000);
  const millis = value % 1_000;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")},${String(millis).padStart(3, "0")}`;
}
