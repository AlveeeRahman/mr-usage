// In-session history as a 2px line with the latest reading marked in the
// accent colour. Hover or focus moves a readout to the nearest reading.
import type { Point } from "./ipc";

const SVG = "http://www.w3.org/2000/svg";
const W = 280;
const H = 40;
const PAD = 4;

function el<K extends keyof SVGElementTagNameMap>(name: K, attrs: Record<string, string>): SVGElementTagNameMap[K] {
  const node = document.createElementNS(SVG, name);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  return node;
}

export function sparkline(points: Point[], describe: (p: Point) => string, readout: HTMLElement): SVGSVGElement | null {
  const data = points.filter((p): p is { t: number; remaining: number } => p.remaining !== null);
  if (data.length < 2) return null;

  const t0 = data[0]!.t;
  const t1 = data[data.length - 1]!.t;
  const values = data.map((p) => p.remaining);
  const lo = Math.min(...values);
  const hi = Math.max(...values);
  const x = (t: number) => PAD + ((t - t0) / Math.max(t1 - t0, 1)) * (W - 2 * PAD);
  const y = (v: number) => (hi === lo ? H / 2 : PAD + (1 - (v - lo) / (hi - lo)) * (H - 2 * PAD));

  // Stretched to the card width; strokes keep their width via non-scaling-stroke.
  const svg = el("svg", { viewBox: `0 0 ${W} ${H}`, preserveAspectRatio: "none", class: "sparkline", role: "img", tabindex: "0" });
  svg.setAttribute("aria-label", `Today: ${data.length} readings, from ${describe(data[0]!)} to ${describe(data[data.length - 1]!)}`);
  svg.append(el("path", { class: "spark-line", d: data.map((p, i) => `${i ? "L" : "M"}${x(p.t).toFixed(1)},${y(p.remaining).toFixed(1)}`).join("") }));
  const cross = el("line", { class: "spark-cross", y1: "0", y2: String(H), visibility: "hidden" });
  const last = data[data.length - 1]!;
  // A zero-length round-capped stroke stays circular when the SVG is stretched.
  const dot = el("path", { class: "spark-dot", d: `M${x(last.t).toFixed(1)},${y(last.remaining).toFixed(1)}h0` });
  svg.append(cross, dot);

  const show = (p: (typeof data)[number]) => {
    cross.setAttribute("x1", x(p.t).toFixed(1));
    cross.setAttribute("x2", x(p.t).toFixed(1));
    cross.setAttribute("visibility", "visible");
    readout.textContent = describe(p);
  };
  const reset = () => {
    cross.setAttribute("visibility", "hidden");
    readout.textContent = `Latest: ${describe(last)}`;
  };
  svg.addEventListener("pointermove", (ev) => {
    const box = svg.getBoundingClientRect();
    const vx = ((ev.clientX - box.left) / box.width) * W;
    const t = t0 + ((vx - PAD) / (W - 2 * PAD)) * (t1 - t0);
    let best = data[0]!;
    for (const p of data) if (Math.abs(p.t - t) < Math.abs(best.t - t)) best = p;
    show(best);
  });
  svg.addEventListener("pointerleave", reset);
  svg.addEventListener("focus", () => show(last));
  svg.addEventListener("blur", reset);
  reset();
  return svg;
}

/** Remaining-of-limit bar. The fill carries severity; the track is a lighter step. */
export function meter(fraction: number, severity: "ok" | "low" | "critical"): SVGSVGElement {
  const f = Math.min(Math.max(fraction, 0), 1);
  const svg = el("svg", { viewBox: "0 0 100 8", preserveAspectRatio: "none", class: "meter", "aria-hidden": "true" });
  svg.append(el("rect", { class: "meter-track", x: "0", y: "0", width: "100", height: "8", rx: "4" }));
  if (f > 0) {
    svg.append(el("rect", { class: `meter-fill ${severity}`, x: "0", y: "0", width: (Math.max(f * 100, 2)).toFixed(2), height: "8", rx: "4" }));
  }
  return svg;
}
