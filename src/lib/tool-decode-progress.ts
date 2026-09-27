/** Smooth decode-phase progress so the bar advances visibly even when Rust emits bursts. */
export class ToolDecodeProgressSmoother {
  private shown = 0;
  private target = 0;
  private active = false;
  private raf = 0;
  private repaint: () => void = () => {};

  bindRepaint(repaint: () => void): void {
    this.repaint = repaint;
  }

  dispose(): void {
    this.stop();
  }

  reset(): void {
    this.stop();
    this.shown = 0;
    this.target = 0;
    this.active = true;
  }

  stop(): void {
    if (this.raf) {
      cancelAnimationFrame(this.raf);
      this.raf = 0;
    }
    this.active = false;
  }

  sync(phase: string | null, target: number | null): void {
    if (phase !== "decoding" || target === null) {
      this.stop();
      return;
    }
    this.active = true;
    this.target = target;
    this.scheduleTick();
  }

  /** Bar + label value while decoding; otherwise use backend percent. */
  displayPercent(phase: string | null, backendPercent: number | null): number | null {
    if (phase === "decoding" && this.active) {
      return this.shown;
    }
    return backendPercent;
  }

  private scheduleTick(): void {
    if (this.raf) {
      return;
    }
    const tick = (): void => {
      this.raf = 0;
      if (!this.active) {
        return;
      }
      if (this.shown < this.target) {
        this.shown = Math.min(this.target, this.shown + 2);
        this.repaint();
      }
      if (this.shown < this.target) {
        this.raf = requestAnimationFrame(tick);
      }
    };
    this.raf = requestAnimationFrame(tick);
  }
}
