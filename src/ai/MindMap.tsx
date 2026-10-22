// Mind map rendering: an SVG tree (root on the left), a preview for the AI panel's mind-map tab,
// and a full-window viewer with panning, zooming, collapsing and export.

import { useEffect, useMemo, useRef, useState } from "react";
import { api, errorText } from "../api";
import { t } from "../i18n";
import { Icon } from "../icons";
import { isDark } from "../settings";
import { CITE_W, FONT_FAMILY, allIds, fontOf, layout, parseOutline, toMarkdown, type MindNode, type Placed } from "./mindTree";

const BRANCH_COLORS = ["#4f86ec", "#e5833a", "#2f9e5b", "#d2457a", "#8a6bd6", "#159ea3", "#c49a1c", "#e05252"];
const MARGIN = 24;

interface Theme {
  fg: string;
  bg: string;
  muted: string;
  accent: string;
}

function readTheme(): Theme {
  const s = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => s.getPropertyValue(name).trim() || fallback;
  return { fg: v("--fg", "#222"), bg: v("--bg", "#fff"), muted: v("--muted", "#888"), accent: v("--accent", "#2f6fde") };
}

const colorOf = (p: Placed, theme: Theme) => (p.branch < 0 ? theme.accent : BRANCH_COLORS[p.branch % BRANCH_COLORS.length]);

/** Where an edge leaves a node (right side) and enters it (left side). */
function anchors(p: Placed) {
  const y = p.depth >= 2 ? p.y + p.h - 1 : p.y + p.h / 2;
  return { inX: p.x, outX: p.x + p.w, y };
}

function Svg(props: {
  root: MindNode;
  collapsed: Set<string>;
  theme: Theme;
  onToggle?: (id: string) => void;
  svgRef?: React.Ref<SVGSVGElement>;
  className?: string;
  style?: React.CSSProperties;
}) {
  const { theme } = props;
  const { nodes, width, height } = useMemo(() => layout(props.root, props.collapsed), [props.root, props.collapsed]);
  const W = width + MARGIN * 2;
  const H = height + MARGIN * 2;
  return (
    <svg
      ref={props.svgRef}
      className={props.className}
      style={props.style}
      xmlns="http://www.w3.org/2000/svg"
      width={W}
      height={H}
      viewBox={`0 0 ${W} ${H}`}
      fontFamily={FONT_FAMILY}
    >
      <g transform={`translate(${MARGIN} ${MARGIN})`}>
        {nodes.map((p) => {
          if (!p.parent) return null;
          const from = anchors(p.parent);
          const to = anchors(p);
          const mid = (from.outX + to.inX) / 2;
          return (
            <path
              key={`e${p.node.id}`}
              d={`M${from.outX} ${from.y} C${mid} ${from.y} ${mid} ${to.y} ${to.inX} ${to.y}`}
              fill="none"
              stroke={colorOf(p, theme)}
              strokeWidth={p.depth === 1 ? 2.2 : 1.5}
              strokeOpacity={0.85}
            />
          );
        })}
        {nodes.map((p) => (
          <NodeView key={p.node.id} p={p} theme={theme} onToggle={props.onToggle} />
        ))}
      </g>
    </svg>
  );
}

function NodeView(props: { p: Placed; theme: Theme; onToggle?: (id: string) => void }) {
  const { p, theme } = props;
  const color = colorOf(p, theme);
  const f = fontOf(p.depth);
  const lineH = p.depth === 0 ? 24 : p.depth === 1 ? 20 : 19;
  const padX = p.depth === 0 ? 16 : p.depth === 1 ? 12 : 4;
  const padY = p.depth === 0 ? 10 : p.depth === 1 ? 7 : 4;
  const toggle = p.node.children.length && props.onToggle ? () => props.onToggle!(p.node.id) : undefined;
  // The root sits on the accent colour, which is light in dark themes.
  const textColor = p.depth === 0 ? (isDark(theme.bg) ? theme.bg : "#ffffff") : theme.fg;
  return (
    <g transform={`translate(${p.x} ${p.y})`} className={toggle ? "mm-node clickable" : "mm-node"} onClick={toggle}>
      {p.depth === 0 && <rect width={p.w} height={p.h} rx={10} fill={theme.accent} />}
      {p.depth === 1 && <rect width={p.w} height={p.h} rx={8} fill={color} fillOpacity={0.14} stroke={color} strokeWidth={1.5} />}
      {p.depth >= 2 && <line x1={0} x2={p.w} y1={p.h - 1} y2={p.h - 1} stroke={color} strokeWidth={1.5} strokeOpacity={0.85} />}
      <text fontSize={f.size} fontWeight={f.weight} fill={textColor}>
        {p.lines.map((line, i) => (
          <tspan key={i} x={padX} y={padY + lineH * i + lineH * 0.72}>
            {line}
          </tspan>
        ))}
      </text>
      {p.node.cites.map((n, i) => {
        const cx = padX + p.lastW + i * CITE_W + CITE_W / 2 + 2;
        const cy = padY + lineH * (p.lines.length - 1) + lineH * 0.45;
        return (
          <g key={n} className="mm-cite">
            <circle cx={cx} cy={cy} r={7.5} fill={theme.accent} fillOpacity={0.18} />
            <text x={cx} y={cy + 3.5} fontSize={10} fontWeight={600} textAnchor="middle" fill={theme.accent}>
              {n}
            </text>
          </g>
        );
      })}
      {p.hidden > 0 && (
        <g transform={`translate(${p.w + 14} ${p.depth >= 2 ? p.h - 1 : p.h / 2})`}>
          <circle r={10} fill={theme.bg} stroke={color} strokeWidth={1.5} />
          <text y={3.5} fontSize={10} fontWeight={600} textAnchor="middle" fill={color}>
            {p.hidden}
          </text>
        </g>
      )}
    </g>
  );
}

// --- export ------------------------------------------------------------------------------------

function svgText(svg: SVGSVGElement, theme: Theme): string {
  const clone = svg.cloneNode(true) as SVGSVGElement;
  clone.removeAttribute("class");
  clone.removeAttribute("style");
  const bg = document.createElementNS("http://www.w3.org/2000/svg", "rect");
  bg.setAttribute("width", "100%");
  bg.setAttribute("height", "100%");
  bg.setAttribute("fill", theme.bg);
  clone.insertBefore(bg, clone.firstChild);
  return '<?xml version="1.0" encoding="UTF-8"?>\n' + new XMLSerializer().serializeToString(clone);
}

const toBase64 = (bytes: Uint8Array) => {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
};

async function pngBase64(svg: SVGSVGElement, theme: Theme): Promise<string> {
  const text = svgText(svg, theme);
  const url = URL.createObjectURL(new Blob([text], { type: "image/svg+xml" }));
  try {
    const img = new Image();
    await new Promise((resolve, reject) => {
      img.onload = resolve;
      img.onerror = reject;
      img.src = url;
    });
    const scale = 2;
    const w = Number(svg.getAttribute("width"));
    const h = Number(svg.getAttribute("height"));
    const canvas = document.createElement("canvas");
    canvas.width = Math.min(16000, w * scale);
    canvas.height = Math.min(16000, h * scale);
    const c = canvas.getContext("2d")!;
    c.scale(canvas.width / w, canvas.height / h);
    c.drawImage(img, 0, 0, w, h);
    return canvas.toDataURL("image/png").split(",")[1];
  } finally {
    URL.revokeObjectURL(url);
  }
}

const fileSafe = (s: string) => s.replace(/[\\/:*?"<>|]+/g, " ").trim().slice(0, 80) || "mindmap";

/** Saves a drawn map through a save dialog; resolves to the path, or null if cancelled. */
export async function saveMindMap(svg: SVGSVGElement, title: string, kind: "png" | "svg"): Promise<string | null> {
  const theme = readTheme();
  const data = kind === "svg" ? toBase64(new TextEncoder().encode(svgText(svg, theme))) : await pngBase64(svg, theme);
  return api.saveExport(`${fileSafe(title)}.${kind}`, kind, data);
}

/** The outline as clean Markdown, for copying. */
export function outlineMarkdown(outline: string, title: string): string {
  const root = parseOutline(outline, title);
  return root ? toMarkdown(root) : outline;
}

// --- preview -----------------------------------------------------------------------------------

/** The whole map scaled to the panel width; clicking opens the viewer. */
export function MindMapPreview(props: {
  outline: string;
  title: string;
  streaming: boolean;
  svgRef?: React.Ref<SVGSVGElement>;
  onOpen: () => void;
}) {
  const root = useMemo(() => parseOutline(props.outline, props.title), [props.outline, props.title]);
  const theme = readTheme();
  const none = useMemo(() => new Set<string>(), []);
  if (!root) return null;
  return (
    <button className="mm-card" onClick={props.onOpen} title={t("mm.open")}>
      <Svg root={root} collapsed={none} theme={theme} className="mm-preview" svgRef={props.svgRef} />
      {!props.streaming && (
        <span className="mm-open">
          <Icon name="maximize" size={13} /> {t("mm.open")}
        </span>
      )}
    </button>
  );
}

// --- full-window viewer --------------------------------------------------------------------------

export function MindMapViewer(props: { outline: string; title: string; onClose: () => void }) {
  const root = useMemo(() => parseOutline(props.outline, props.title), [props.outline, props.title]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [view, setView] = useState({ x: 0, y: 0, k: 1 });
  const [menu, setMenu] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const stage = useRef<HTMLDivElement>(null);
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<{ x: number; y: number; vx: number; vy: number } | null>(null);
  const theme = readTheme();

  const fit = () => {
    const el = stage.current;
    const s = svg.current;
    if (!el || !s) return;
    const w = Number(s.getAttribute("width"));
    const h = Number(s.getAttribute("height"));
    const k = Math.min(1.4, el.clientWidth / w, el.clientHeight / h);
    setView({ k, x: (el.clientWidth - w * k) / 2, y: (el.clientHeight - h * k) / 2 });
  };
  // Fit once the first layout is on screen.
  useEffect(fit, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") props.onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [props.onClose]);

  useEffect(() => {
    if (!notice) return;
    const id = window.setTimeout(() => setNotice(null), 2500);
    return () => window.clearTimeout(id);
  }, [notice]);

  const zoomAt = (factor: number, cx?: number, cy?: number) =>
    setView((v) => {
      const el = stage.current!;
      const px = cx ?? el.clientWidth / 2;
      const py = cy ?? el.clientHeight / 2;
      const k = Math.max(0.15, Math.min(4, v.k * factor));
      return { k, x: px - ((px - v.x) * k) / v.k, y: py - ((py - v.y) * k) / v.k };
    });

  if (!root) return null;

  const toggle = (id: string) =>
    setCollapsed((c) => {
      const n = new Set(c);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const save = async (kind: "png" | "svg") => {
    setMenu(false);
    if (!svg.current) return;
    try {
      const path = await saveMindMap(svg.current, props.title, kind);
      if (path) setNotice(t("reader.exported", { path }));
    } catch (e) {
      setNotice(errorText(e));
    }
  };

  const copyOutline = () => {
    setMenu(false);
    navigator.clipboard.writeText(toMarkdown(root)).then(
      () => setNotice(t("common.copied")),
      () => setNotice(t("common.copyFailed")),
    );
  };

  return (
    <div className="mm-viewer" onClick={() => setMenu(false)}>
      <div className="mm-bar">
        <Icon name="mindmap" />
        <strong className="ellipsis">{props.title}</strong>
        <span className="spacer" />
        <div className="tool-group">
          <button className="icon-button" title={t("reader.zoomOut")} aria-label={t("reader.zoomOut")} onClick={() => zoomAt(1 / 1.2)}>
            <Icon name="minus" />
          </button>
          <button className="zoom" title={t("mm.fit")} onClick={fit}>
            {Math.round(view.k * 100)}%
          </button>
          <button className="icon-button" title={t("reader.zoomIn")} aria-label={t("reader.zoomIn")} onClick={() => zoomAt(1.2)}>
            <Icon name="plus" />
          </button>
        </div>
        <button onClick={() => setCollapsed(new Set())}>{t("side.expandAll")}</button>
        <button onClick={() => setCollapsed(new Set(root.children.flatMap((c) => allIds(c))))}>{t("mm.collapseToBranches")}</button>
        <div className="menu-anchor">
          <button
            className="primary"
            onClick={(e) => {
              e.stopPropagation();
              setMenu((m) => !m);
            }}
          >
            {t("mm.export")} <Icon name="chevronDown" size={14} />
          </button>
          {menu && (
            <div className="menu dropdown" onClick={(e) => e.stopPropagation()}>
              <button onClick={() => save("png")}>{t("mm.exportPng")}</button>
              <button onClick={() => save("svg")}>{t("mm.exportSvg")}</button>
              <button onClick={copyOutline}>{t("mm.copyOutline")}</button>
            </div>
          )}
        </div>
        <button className="icon-button" title={t("common.close")} aria-label={t("common.close")} onClick={props.onClose}>
          <Icon name="close" />
        </button>
      </div>
      <div
        ref={stage}
        className={`mm-stage${drag.current ? " dragging" : ""}`}
        onWheel={(e) => {
          if (e.ctrlKey || e.metaKey) {
            const r = stage.current!.getBoundingClientRect();
            zoomAt(Math.exp(-e.deltaY / 300), e.clientX - r.left, e.clientY - r.top);
          } else setView((v) => ({ ...v, x: v.x - e.deltaX, y: v.y - e.deltaY }));
        }}
        onPointerDown={(e) => {
          if ((e.target as Element).closest(".clickable")) return;
          drag.current = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y };
          try {
            (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
          } catch {
            // Dragging still works inside the stage without capture.
          }
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (d) setView((v) => ({ ...v, x: d.vx + e.clientX - d.x, y: d.vy + e.clientY - d.y }));
        }}
        onPointerUp={() => (drag.current = null)}
        onPointerCancel={() => (drag.current = null)}
      >
        <div className="mm-canvas" style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.k})` }}>
          <Svg
            root={root}
            collapsed={collapsed}
            theme={theme}
            svgRef={svg}
            onToggle={toggle}
          />
        </div>
        <div className="mm-hint muted small">{t("mm.hint")}</div>
      </div>
      {notice && <div className="toast">{notice}</div>}
    </div>
  );
}
