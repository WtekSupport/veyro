/** Shared mic level history graph (VAD threshold line + level trace). */

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
  ctx.clearRect(0, 0, width, height);

  ctx.fillStyle = "rgba(255, 255, 255, 0.04)";
  ctx.fillRect(0, height * (1 - threshold / 100), width, height * (threshold / 100));

  const points = history.length > 0 ? history : Array.from({ length: 2 }, () => currentLevel);

  ctx.strokeStyle = "rgba(96, 165, 250, 0.9)";
  ctx.lineWidth = Math.max(1, Math.round((window.devicePixelRatio || 1) * 1.5));
  ctx.beginPath();

  points.forEach((level, index) => {
    const x = (index / Math.max(points.length - 1, 1)) * (width - 8) + 4;
    const y = height - (Math.max(0, Math.min(100, level)) / 100) * (height - 8) - 4;
    if (index === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  });
  ctx.stroke();

  ctx.strokeStyle = "rgba(250, 204, 21, 0.85)";
  ctx.setLineDash([4 * (window.devicePixelRatio || 1), 4 * (window.devicePixelRatio || 1)]);
  ctx.beginPath();
  const thresholdY = height - (threshold / 100) * (height - 8) - 4;
  ctx.moveTo(0, thresholdY);
  ctx.lineTo(width, thresholdY);
  ctx.stroke();
  ctx.setLineDash([]);
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
