import type { UpdateSettings } from "../App";
import { BACKGROUNDS, FONT_STACKS, isDark, type FontFamily, type Settings } from "../settings";

function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  onChange: (v: number) => void;
}) {
  return (
    <label className="slider">
      <span>{props.label}</span>
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        onChange={(e) => props.onChange(Number(e.target.value))}
      />
      <span className="slider-value">{props.format(props.value)}</span>
    </label>
  );
}

function Toggle(props: { label: string; on: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={props.on} onChange={(e) => props.onChange(e.target.checked)} />
      {props.label}
    </label>
  );
}

export default function SettingsPanel(props: {
  settings: Settings;
  update: UpdateSettings;
  textOptions: boolean;
  onFocusMode: () => void;
  onFullscreen: () => void;
  onShortcuts: () => void;
}) {
  const { settings: s, update } = props;
  const custom = !BACKGROUNDS.some((b) => b.color === s.background);
  return (
    <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
      <div className="panel-section">
        <div className="panel-label">背景颜色</div>
        <div className="swatches">
          {BACKGROUNDS.map((b) => (
            <button
              key={b.color}
              className={`swatch${s.background === b.color ? " on" : ""}`}
              style={{ background: b.color, color: isDark(b.color) ? "#eee" : "#333" }}
              title={b.name}
              onClick={() => update({ background: b.color })}
            >
              {b.name}
            </button>
          ))}
          <label className={`swatch custom${custom ? " on" : ""}`} title="自定义颜色">
            <input type="color" value={s.background} onChange={(e) => update({ background: e.target.value })} />
            自定义
          </label>
        </div>
      </div>

      <div className="panel-section">
        <div className="panel-label">文字颜色</div>
        <div className="row-gap">
          <div className="segmented">
            <button className={s.textColor === null ? "on" : ""} onClick={() => update({ textColor: null })}>
              自动
            </button>
            <button
              className={s.textColor !== null ? "on" : ""}
              onClick={() => update({ textColor: isDark(s.background) ? "#d8d8d8" : "#222222" })}
            >
              自定义
            </button>
          </div>
          {s.textColor !== null && (
            <input type="color" value={s.textColor} onChange={(e) => update({ textColor: e.target.value })} />
          )}
        </div>
      </div>

      {props.textOptions && (
        <>
          <div className="panel-section">
            <div className="panel-label">字体</div>
            <div className="segmented">
              {(Object.keys(FONT_STACKS) as FontFamily[]).map((f) => (
                <button
                  key={f}
                  className={s.fontFamily === f ? "on" : ""}
                  style={{ fontFamily: FONT_STACKS[f].css }}
                  onClick={() => update({ fontFamily: f })}
                >
                  {FONT_STACKS[f].name}
                </button>
              ))}
            </div>
          </div>
          <div className="panel-section">
            <Slider label="字号" value={s.fontSize} min={13} max={36} step={1} format={(v) => `${v}px`} onChange={(v) => update({ fontSize: v })} />
            <Slider label="行距" value={s.lineHeight} min={1.3} max={2.6} step={0.1} format={(v) => v.toFixed(1)} onChange={(v) => update({ lineHeight: v })} />
            <Slider label="段距" value={s.paragraphSpacing} min={0} max={2} step={0.05} format={(v) => `${v.toFixed(2)}em`} onChange={(v) => update({ paragraphSpacing: v })} />
            <Slider label="页边距" value={s.margin} min={0} max={120} step={4} format={(v) => `${v}px`} onChange={(v) => update({ margin: v })} />
            {s.flowMode === "scroll" && (
              <Slider label="版心" value={s.maxWidth} min={480} max={1400} step={20} format={(v) => `${v}px`} onChange={(v) => update({ maxWidth: v })} />
            )}
            <div className="row-gap toggles">
              <Toggle label="首行缩进" on={s.indent} onChange={(indent) => update({ indent })} />
              <Toggle label="两端对齐" on={s.justify} onChange={(justify) => update({ justify })} />
            </div>
          </div>
          <div className="panel-section">
            <div className="panel-label">翻页方式</div>
            <div className="row-gap">
              <div className="segmented">
                <button className={s.flowMode === "scroll" ? "on" : ""} onClick={() => update({ flowMode: "scroll" })}>
                  滚动
                </button>
                <button className={s.flowMode === "paged" ? "on" : ""} onClick={() => update({ flowMode: "paged" })}>
                  分页
                </button>
              </div>
              {s.flowMode === "paged" && (
                <div className="segmented">
                  <button className={s.columns === 1 ? "on" : ""} onClick={() => update({ columns: 1 })}>
                    单栏
                  </button>
                  <button className={s.columns === 2 ? "on" : ""} onClick={() => update({ columns: 2 })}>
                    双栏
                  </button>
                </div>
              )}
            </div>
          </div>
        </>
      )}

      <div className="panel-section">
        <div className="panel-label">显示</div>
        <div className="row-gap">
          <button onClick={props.onFocusMode} title="F">
            专注模式
          </button>
          <button onClick={props.onFullscreen}>全屏</button>
          <button onClick={props.onShortcuts} title="?">
            快捷键
          </button>
        </div>
      </div>
    </div>
  );
}
