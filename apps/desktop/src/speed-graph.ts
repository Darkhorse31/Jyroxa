// High-DPI theme-aware smooth canvas speed graph with Catmull-Rom spline and gradient area

import { fmtSpeed } from "./formatters";

export class SpeedGraph {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private samples: number[] = [];
  private maxPoints = 90;
  private resizeObserver: ResizeObserver | null = null;
  private currentSpeed = 0;
  private peakSpeed = 0;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d", { alpha: true })!;

    this.setupHighDpi();

    if (window.ResizeObserver) {
      this.resizeObserver = new ResizeObserver(() => {
        this.setupHighDpi();
        this.draw();
      });
      if (this.canvas.parentElement) {
        this.resizeObserver.observe(this.canvas.parentElement);
      }
    }
  }

  private setupHighDpi() {
    const rect = this.canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const displayWidth = Math.max(200, Math.floor(rect.width || 480));
    const displayHeight = Math.max(50, Math.floor(rect.height || 74));

    if (
      this.canvas.width !== displayWidth * dpr ||
      this.canvas.height !== displayHeight * dpr
    ) {
      this.canvas.width = displayWidth * dpr;
      this.canvas.height = displayHeight * dpr;
      this.canvas.style.width = `${displayWidth}px`;
      this.canvas.style.height = `${displayHeight}px`;
    }

    this.ctx.resetTransform?.();
    this.ctx.scale(dpr, dpr);
  }

  push(totalBps: number) {
    this.currentSpeed = totalBps;
    if (totalBps > this.peakSpeed) {
      this.peakSpeed = totalBps;
    }
    this.samples.push(totalBps);
    if (this.samples.length > this.maxPoints) {
      this.samples.shift();
    }
    this.draw();
  }

  draw() {
    const rect = this.canvas.getBoundingClientRect();
    const w = rect.width;
    const h = rect.height;
    if (w <= 0 || h <= 0) return;

    const style = getComputedStyle(document.body);
    const lineColor = style.getPropertyValue("--graph-line").trim() || "#2DD4BF";
    const fillColor = style.getPropertyValue("--graph-fill").trim() || "rgba(45, 212, 191, 0.18)";
    const gridColor = style.getPropertyValue("--graph-grid").trim() || "rgba(148, 163, 184, 0.15)";
    const textColor = style.getPropertyValue("--text-muted").trim() || "#64748b";

    this.ctx.clearRect(0, 0, w, h);

    const padTop = 16;
    const padBottom = 8;
    const plotH = Math.max(10, h - padTop - padBottom);

    // Calculate max scale based on peak or current sample max
    const sampleMax = Math.max(...this.samples, 0);
    const maxVal = Math.max(sampleMax, 1024 * 100); // at least 100 KB/s range

    // Draw horizontal grid lines
    this.ctx.strokeStyle = gridColor;
    this.ctx.lineWidth = 1;
    this.ctx.setLineDash([3, 4]);

    const gridSteps = [0.25, 0.5, 0.75, 1.0];
    for (const step of gridSteps) {
      const y = padTop + plotH * (1 - step);
      this.ctx.beginPath();
      this.ctx.moveTo(0, y);
      this.ctx.lineTo(w, y);
      this.ctx.stroke();
    }
    this.ctx.setLineDash([]);

    // Draw grid peak text label
    this.ctx.fillStyle = textColor;
    this.ctx.font = '10px "Segoe UI", system-ui, sans-serif';
    this.ctx.textAlign = "right";
    this.ctx.textBaseline = "middle";
    this.ctx.fillText(fmtSpeed(maxVal), w - 8, padTop - 6);

    // Draw current speed text badge on top left
    this.ctx.textAlign = "left";
    this.ctx.fillStyle = lineColor;
    this.ctx.font = '600 11px "Segoe UI", system-ui, sans-serif';
    this.ctx.fillText(`${fmtSpeed(this.currentSpeed)}`, 8, padTop - 6);

    if (this.samples.length < 2) return;

    // Calculate coordinates
    const points: { x: number; y: number }[] = [];
    const stepX = w / (this.maxPoints - 1);
    const startOffset = (this.maxPoints - this.samples.length) * stepX;

    for (let i = 0; i < this.samples.length; i++) {
      const x = startOffset + i * stepX;
      const norm = Math.min(1, Math.max(0, this.samples[i] / maxVal));
      const y = padTop + plotH * (1 - norm);
      points.push({ x, y });
    }

    // Build smooth Catmull-Rom curve
    this.ctx.beginPath();
    this.ctx.moveTo(points[0].x, points[0].y);

    for (let i = 0; i < points.length - 1; i++) {
      const p0 = points[Math.max(0, i - 1)];
      const p1 = points[i];
      const p2 = points[i + 1];
      const p3 = points[Math.min(points.length - 1, i + 2)];

      const cp1x = p1.x + (p2.x - p0.x) / 6;
      const cp1y = p1.y + (p2.y - p0.y) / 6;

      const cp2x = p2.x - (p3.x - p1.x) / 6;
      const cp2y = p2.y - (p3.y - p1.y) / 6;

      this.ctx.bezierCurveTo(cp1x, cp1y, cp2x, cp2y, p2.x, p2.y);
    }

    // Stroke line
    this.ctx.strokeStyle = lineColor;
    this.ctx.lineWidth = 2.2;
    this.ctx.lineCap = "round";
    this.ctx.lineJoin = "round";
    this.ctx.stroke();

    // Fill area below curve
    const lastPt = points[points.length - 1];
    const firstPt = points[0];

    this.ctx.lineTo(lastPt.x, h);
    this.ctx.lineTo(firstPt.x, h);
    this.ctx.closePath();

    const gradient = this.ctx.createLinearGradient(0, padTop, 0, h);
    gradient.addColorStop(0, fillColor);
    gradient.addColorStop(1, "rgba(0, 0, 0, 0)");

    this.ctx.fillStyle = gradient;
    this.ctx.fill();

    // Highlight current dot at the tip
    this.ctx.fillStyle = lineColor;
    this.ctx.beginPath();
    this.ctx.arc(lastPt.x, lastPt.y, 3.5, 0, Math.PI * 2);
    this.ctx.fill();

    this.ctx.strokeStyle = "#ffffff";
    this.ctx.lineWidth = 1.5;
    this.ctx.stroke();
  }

  destroy() {
    if (this.resizeObserver) {
      this.resizeObserver.disconnect();
      this.resizeObserver = null;
    }
  }
}
