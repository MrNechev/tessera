/** Lightweight timing counters, read by the autotest driver ("perf" step). */
export const perfStats = {
  draws: 0,
  drawMs: 0,
  maxDrawMs: 0,
  reset(): void {
    this.draws = 0;
    this.drawMs = 0;
    this.maxDrawMs = 0;
  },
};
