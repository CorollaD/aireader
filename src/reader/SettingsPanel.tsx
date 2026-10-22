// Reading appearance controls, shown in the reader's "Aa" popover and in the Settings dialog.

import type { UpdateSettings } from "../App";
import { t } from "../i18n";
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

function Choice<T extends string | number>(props: { value: T; options: [T, string][]; onChange: (v: T) => void }) {
  return (
    <div className="segmented">
      {props.options.map(([v, label]) => (
        <button key={v} className={props.value === v ? "on" : ""} onClick={() => props.onChange(v)}>
          {label}
        </button>
      ))}
    </div>
  );
}

/** The appearance sections; `text` adds typography, `paged` adds PDF/comic page options. */
export function ReadingSettings(props: { settings: Settings; update: UpdateSettings; text: boolean; paged: boolean }) {
  const { settings: s, update } = props;
  const custom = !BACKGROUNDS.some((b) => b.color === s.background);
  return (
    <>
      <div className="panel-section">
        <div className="panel-label">{t("rs.background")}</div>
        <div className="swatches">
          {BACKGROUNDS.map((b) => (
            <button
              key={b.color}
              className={`swatch${s.background === b.color ? " on" : ""}`}
              style={{ background: b.color, color: isDark(b.color) ? "#eee" : "#333" }}
              title={t(b.name)}
              onClick={() => update({ background: b.color })}
            >
              {t(b.name)}
            </button>
          ))}
          <label className={`swatch custom${custom ? " on" : ""}`} title={t("rs.customColor")}>
            <input type="color" value={s.background} onChange={(e) => update({ background: e.target.value })} />
            {t("rs.custom")}
          </label>
        </div>
      </div>

      <div className="panel-section">
        <div className="panel-label">{t("rs.textColor")}</div>
        <div className="row-gap">
          <div className="segmented">
            <button className={s.textColor === null ? "on" : ""} onClick={() => update({ textColor: null })}>
              {t("rs.auto")}
            </button>
            <button
              className={s.textColor !== null ? "on" : ""}
              onClick={() => update({ textColor: isDark(s.background) ? "#d8d8d8" : "#222222" })}
            >
              {t("rs.custom")}
            </button>
          </div>
          {s.textColor !== null && (
            <input type="color" value={s.textColor} onChange={(e) => update({ textColor: e.target.value })} />
          )}
        </div>
      </div>

      {props.text && (
        <>
          <div className="panel-section">
            <div className="panel-label">{t("rs.font")}</div>
            <div className="segmented">
              {(Object.keys(FONT_STACKS) as FontFamily[]).map((f) => (
                <button
                  key={f}
                  className={s.fontFamily === f ? "on" : ""}
                  style={{ fontFamily: FONT_STACKS[f].css }}
                  onClick={() => update({ fontFamily: f })}
                >
                  {t(FONT_STACKS[f].name)}
                </button>
              ))}
            </div>
          </div>
          <div className="panel-section">
            <Slider label={t("rs.fontSize")} value={s.fontSize} min={13} max={36} step={1} format={(v) => `${v}px`} onChange={(v) => update({ fontSize: v })} />
            <Slider label={t("rs.lineHeight")} value={s.lineHeight} min={1.3} max={2.6} step={0.1} format={(v) => v.toFixed(1)} onChange={(v) => update({ lineHeight: v })} />
            <Slider label={t("rs.paragraphSpacing")} value={s.paragraphSpacing} min={0} max={2} step={0.05} format={(v) => `${v.toFixed(2)}em`} onChange={(v) => update({ paragraphSpacing: v })} />
            <Slider label={t("rs.margin")} value={s.margin} min={0} max={120} step={4} format={(v) => `${v}px`} onChange={(v) => update({ margin: v })} />
            {s.flowMode === "scroll" && (
              <Slider label={t("rs.maxWidth")} value={s.maxWidth} min={480} max={1400} step={20} format={(v) => `${v}px`} onChange={(v) => update({ maxWidth: v })} />
            )}
            <div className="row-gap toggles">
              <Toggle label={t("rs.indent")} on={s.indent} onChange={(indent) => update({ indent })} />
              <Toggle label={t("rs.justify")} on={s.justify} onChange={(justify) => update({ justify })} />
            </div>
          </div>
          <div className="panel-section">
            <div className="panel-label">{t("rs.pageTurn")}</div>
            <div className="row-gap">
              <Choice
                value={s.flowMode}
                options={[
                  ["scroll", t("rs.scroll")],
                  ["paged", t("rs.paged")],
                ]}
                onChange={(flowMode) => update({ flowMode })}
              />
              {s.flowMode === "paged" && (
                <Choice
                  value={s.columns}
                  options={[
                    [1, t("rs.oneColumn")],
                    [2, t("rs.twoColumns")],
                  ]}
                  onChange={(columns) => update({ columns })}
                />
              )}
            </div>
          </div>
        </>
      )}

      {props.paged && (
        <div className="panel-section">
          {props.text && <div className="panel-heading">{t("settings.pagedBooks")}</div>}
          <div className="option-row">
            <span>{t("rs.pageLayout")}</span>
            <Choice
              value={s.pageLayout}
              options={[
                ["continuous", t("rs.continuous")],
                ["single", t("rs.single")],
              ]}
              onChange={(pageLayout) => update({ pageLayout })}
            />
          </div>
          <div className="option-row">
            <span>{t("rs.fit")}</span>
            <Choice
              value={s.pageFit}
              options={[
                ["width", t("rs.fitWidth")],
                ["page", t("rs.fitPage")],
              ]}
              onChange={(pageFit) => update({ pageFit })}
            />
          </div>
        </div>
      )}
    </>
  );
}

/** The reader's "Aa" popover. */
export default function SettingsPanel(props: {
  settings: Settings;
  update: UpdateSettings;
  textOptions: boolean;
  onMore: () => void;
}) {
  return (
    <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
      <ReadingSettings settings={props.settings} update={props.update} text={props.textOptions} paged={false} />
      <div className="panel-footer">
        <button className="link-button" onClick={props.onMore}>
          {t("rs.more")}
        </button>
      </div>
    </div>
  );
}
