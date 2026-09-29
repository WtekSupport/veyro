/** Shared mic level history graph (VAD threshold line + pointwise level trace). */

const BRASS = "255, 193, 69";
const GRID_LEVELS = [0, 25, 50, 75, 100] as const;

export function resizeVadGraphCanvas(canvas: HTMLCanvasElement): void {
  const wrap = canvas.closest<HTMLElement>("[data-vad-graph-wrap]");
  if (!wrap) {
    return;
  }
  const rect = wrap.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.floor(rect.width * dpr));
  const h = Math.max(1, Math.floor(rect.height * dpr));
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
    canvas.style.width = `${rect.width}px`;
    canvas.style.height = `${rect.height}px`;
  }
}

export function drawVadLevelGraph(
  canvas: HTMLCanvasElement,
  history: number[],
  threshold: number,
  currentLevel: number,
): void {
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    return;
  }

  const width = canvas.width;
  const height = canvas.height;
  const dpr = window.devicePixelRatio || 1;
  const padL = Math.round(34 * dpr);
  const padR = Math.round(10 * dpr);
  const padT = Math.round(8 * dpr);
  const padB = Math.round(8 * dpr);
  const plotW = Math.max(1, width - padL - padR);
  const plotH = Math.max(1, height - padT - padB);

  ctx.clearRect(0, 0, width, height);

  drawGrid(ctx, padL, padT, plotW, plotH, dpr, threshold);
  drawAxisLabels(ctx, padL, padT, plotH, dpr);

  const levels =
    history.length > 0
      ? history.map((level) => clampLevel(level))
      : [clampLevel(currentLevel), clampLevel(currentLevel)];

  drawPointwiseFill(ctx, levels, padL, padT, plotW, plotH, dpr);
  drawGlowTrace(ctx, levels, padL, padT, plotW, plotH, dpr);
  drawThresholdLine(ctx, threshold, padL, padT, plotW, plotH, dpr);
}

export function bindVadGraphResize(root: ParentNode): () => void {
  const canvases = root.querySelectorAll<HTMLCanvasElement>("[data-vad-graph]");
  const observers: ResizeObserver[] = [];

  canvases.forEach((canvas) => {
    const wrap = canvas.closest<HTMLElement>("[data-vad-graph-wrap]");
    if (!wrap) {
      return;
    }
    resizeVadGraphCanvas(canvas);
    const observer = new ResizeObserver(() => {
      resizeVadGraphCanvas(canvas);
    });
    observer.observe(wrap);
    observers.push(observer);
  });

  return () => {
    observers.forEach((observer) => observer.disconnect());
  };
}

function clampLevel(level: number): number {
  return Math.max(0, Math.min(100, level));
}

function levelToY(level: number, padT: number, plotH: number): number {
  return padT + plotH * (1 - clampLevel(level) / 100);
}

function sampleLevel(levels: number[], t: number): number {
  if (levels.length === 1) {
    return levels[0]!;
  }
  const scaled = Math.max(0, Math.min(1, t)) * (levels.length - 1);
  const i = Math.floor(scaled);
  const next = Math.min(levels.length - 1, i + 1);
  const frac = scaled - i;
  return levels[i]! * (1 - frac) + levels[next]! * frac;
}

function drawGrid(
  ctx: CanvasRenderingContext2D,
  padL: number,
  padT: number,
  plotW: number,
  plotH: number,
  dpr: number,
  threshold: number,
): void {
  const dash = Math.max(2, Math.round(3 * dpr));
  ctx.lineWidth = Math.max(1, Math.round(dpr * 0.75));
  ctx.setLineDash([dash, dash]);

  for (const level of GRID_LEVELS) {
    const y = levelToY(level, padT, plotH);
    const nearThreshold = Math.abs(level - threshold) < 6;
    ctx.strokeStyle = nearThreshold
      ? `rgba(${BRASS}, 0.55)`
      : `rgba(${BRASS}, 0.14)`;
    ctx.beginPath();
    ctx.moveTo(padL, y);
    ctx.lineTo(padL + plotW, y);
    ctx.stroke();
  }

  ctx.setLineDash([]);
}

function drawAxisLabels(
  ctx: CanvasRenderingContext2D,
  padL: number,
  padT: number,
  plotH: number,
  dpr: number,
): void {
  ctx.fillStyle = `rgba(${BRASS}, 0.55)`;
  ctx.font = `${Math.max(9, Math.round(10 * dpr))}px ui-sans-serif, system-ui, sans-serif`;
  ctx.textAlign = "right";
  ctx.textBaseline = "middle";

  for (const level of GRID_LEVELS) {
    const y = levelToY(level, padT, plotH);
    ctx.fillText(`${level}%`, padL - Math.round(6 * dpr), y);
  }
}

function drawPointwiseFill(
  ctx: CanvasRenderingContext2D,
  levels: number[],
  padL: number,
  padT: number,
  plotW: number,
  plotH: number,
  dpr: number,
): void {
  const barStep = Math.max(2, Math.round(2.5 * dpr));
  const bottom = padT + plotH;
  const barWidth = Math.max(1, Math.round(dpr));

  for (let x = 0; x < plotW; x += barStep) {
    const t = plotW <= 1 ? 1 : x / (plotW - 1);
    const level = sampleLevel(levels, t);
    if (level <= 0.4) {
      continue;
    }

    const top = levelToY(level, padT, plotH);
    const fade = ctx.createLinearGradient(0, top, 0, bottom);
    fade.addColorStop(0, `rgba(${BRASS}, 0.72)`);
    fade.addColorStop(0.35, `rgba(${BRASS}, 0.28)`);
    fade.addColorStop(1, `rgba(${BRASS}, 0.02)`);

    ctx.fillStyle = fade;
    ctx.fillRect(padL + x, top, barWidth, bottom - top);
  }
}

function drawGlowTrace(
  ctx: CanvasRenderingContext2D,
  levels: number[],
  padL: number,
  padT: number,
  plotW: number,
  plotH: number,
  dpr: number,
): void {
  const points = buildPlotPoints(levels, padL, padT, plotW, plotH);
  if (points.length === 0) {
    return;
  }

  ctx.save();
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  ctx.strokeStyle = `rgba(${BRASS}, 0.95)`;
  ctx.lineWidth = Math.max(1.5, Math.round(1.75 * dpr));
  ctx.shadowColor = `rgba(${BRASS}, 0.85)`;
  ctx.shadowBlur = Math.round(14 * dpr);
  strokeSmoothPath(ctx, points);
  ctx.stroke();

  // Second pass keeps the core brighter without extra bloom wash.
  ctx.shadowBlur = Math.round(4 * dpr);
  ctx.lineWidth = Math.max(1, Math.round(dpr));
  ctx.strokeStyle = `rgba(255, 232, 180, 0.95)`;
  strokeSmoothPath(ctx, points);
  ctx.stroke();
  ctx.restore();

  const tip = points[points.length - 1]!;
  ctx.save();
  ctx.shadowColor = `rgba(${BRASS}, 0.95)`;
  ctx.shadowBlur = Math.round(12 * dpr);
  ctx.fillStyle = `rgb(${BRASS})`;
  ctx.beginPath();
  ctx.arc(tip.x, tip.y, Math.max(2.5, 3 * dpr), 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
}

function drawThresholdLine(
  ctx: CanvasRenderingContext2D,
  threshold: number,
  padL: number,
  padT: number,
  plotW: number,
  plotH: number,
  dpr: number,
): void {
  const y = levelToY(threshold, padT, plotH);
  const dash = Math.max(3, Math.round(4 * dpr));
  ctx.save();
  ctx.setLineDash([dash, dash]);
  ctx.strokeStyle = `rgba(${BRASS}, 0.9)`;
  ctx.lineWidth = Math.max(1, Math.round(dpr));
  ctx.shadowColor = `rgba(${BRASS}, 0.45)`;
  ctx.shadowBlur = Math.round(6 * dpr);
  ctx.beginPath();
  ctx.moveTo(padL, y);
  ctx.lineTo(padL + plotW, y);
  ctx.stroke();
  ctx.restore();
}

function buildPlotPoints(
  levels: number[],
  padL: number,
  padT: number,
  plotW: number,
  plotH: number,
): Array<{ x: number; y: number }> {
  if (levels.length === 1) {
    const y = levelToY(levels[0]!, padT, plotH);
    return [
      { x: padL, y },
      { x: padL + plotW, y },
    ];
  }

  return levels.map((level, index) => ({
    x: padL + (index / (levels.length - 1)) * plotW,
    y: levelToY(level, padT, plotH),
  }));
}

function strokeSmoothPath(
  ctx: CanvasRenderingContext2D,
  points: Array<{ x: number; y: number }>,
): void {
  ctx.beginPath();
  if (points.length < 3) {
    points.forEach((point, index) => {
      if (index === 0) {
        ctx.moveTo(point.x, point.y);
      } else {
        ctx.lineTo(point.x, point.y);
      }
    });
    return;
  }

  ctx.moveTo(points[0]!.x, points[0]!.y);
  for (let i = 1; i < points.length - 1; i += 1) {
    const current = points[i]!;
    const next = points[i + 1]!;
    const midX = (current.x + next.x) / 2;
    const midY = (current.y + next.y) / 2;
    ctx.quadraticCurveTo(current.x, current.y, midX, midY);
  }
  const last = points[points.length - 1]!;
  const prev = points[points.length - 2]!;
  ctx.quadraticCurveTo(prev.x, prev.y, last.x, last.y);
}
