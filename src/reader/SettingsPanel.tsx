// Reading appearance controls, shown in the reader's "Aa" popover and in the Settings dialog.

import { useEffect, useState, type ReactNode } from "react";
import type { UpdateSettings } from "../App";
import { api, errorText } from "../api";
import { t } from "../i18n";
import { BACKGROUNDS, FONT_STACKS, FONT_WEIGHTS, isDark, READING_THEMES, type FontFamily, type FontWeight, type Settings } from "../settings";
import type { MessageKey } from "../i18n";
import { BTN, BTN_GHOST } from "../tw";

const WEIGHT_NAMES: Record<FontWeight, MessageKey> = { 400: "rs.weightRegular", 500: "rs.weightMedium", 600: "rs.weightBold" };
import { Select } from "../Select";

function Slider(props: { label: string; value: number; min: number; max: number; step: number; format: (v: number) => string; onChange: (v: number) => void }) {
  return (
    <label className="my-2.5 grid grid-cols-[84px_1fr_52px] items-center gap-2.5 text-[13px]">
      <span>{props.label}</span>
      <input
        type="range"
        className="d-range d-range-primary d-range-xs"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        onChange={(e) => props.onChange(Number(e.target.value))}
      />
      <span className="text-right text-xs text-[var(--muted)] tabular-nums">{props.format(props.value)}</span>
    </label>
  );
}

/** Sections of the panel: a small heading over its controls. */
function Section(props: { label?: string; children: ReactNode }) {
  return (
    <section className="mb-4 last:mb-0">
      {props.label && <h3 className="mb-2 text-xs font-semibold text-[var(--muted)]">{props.label}</h3>}
      {props.children}
    </section>
  );
}

/** Picking (or adding) a font file of the user's own for book text. */
function CustomFont(props: { value: string | null; onChange: (name: string | null) => void }) {
  const [fonts, setFonts] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api
      .listFonts()
      .then(setFonts)
      .catch(() => {});
  }, []);
  const add = async () => {
    setError(null);
    try {
      const name = await api.importFont();
      if (!name) return;
      setFonts((f) => [...new Set([...f, name])].sort());
      props.onChange(name);
    } catch (e) {
      setError(errorText(e));
    }
  };
  const remove = async () => {
    if (!props.value) return;
    const name = props.value;
    props.onChange(null);
    await api.removeFont(name).catch(() => {});
    setFonts((f) => f.filter((x) => x !== name));
  };
  const label = (name: string) => name.replace(/\.[^.]+$/, "");
  return (
    <div className="mt-2 flex flex-wrap items-center gap-2">
      <Select
        full
        value={props.value ?? ""}
        label={t("rs.customFont")}
        options={[{ value: "", label: t("rs.customFontNone") }, ...fonts.map((f) => ({ value: f, label: label(f) }))]}
        onChange={(font) => props.onChange(font || null)}
      />
      <button className={BTN} onClick={add}>
        {t("rs.addFont")}
      </button>
      {props.value && (
        <button className={`${BTN_GHOST} text-[var(--danger)]`} onClick={remove} title={t("rs.removeFont")}>
          {t("common.delete")}
        </button>
      )}
      {error && <div className="w-full text-xs text-[var(--danger)]">{error}</div>}
    </div>
  );
}

function Toggle(props: { label: string; on: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="flex cursor-pointer items-center gap-2 text-[13px]">
      <input type="checkbox" className="d-toggle d-toggle-primary d-toggle-sm" checked={props.on} onChange={(e) => props.onChange(e.target.checked)} />
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
  const themeOn = (th: (typeof READING_THEMES)[number]) =>
    s.background === th.background && s.textColor === th.textColor && s.fontWeight === th.fontWeight && (!props.text || s.fontFamily === th.fontFamily);
  return (
    <>
      <Section label={t("rs.theme")}>
        <div className="grid grid-cols-3 gap-2">
          {READING_THEMES.map((th) => (
            <button
              key={th.name}
              className={`tw flex h-16 flex-col items-center justify-center gap-0.5 rounded-xl border border-black/10 shadow-sm transition ${
                themeOn(th) ? "ring-2 ring-[var(--accent)] ring-offset-2 ring-offset-[var(--surface)]" : "hover:-translate-y-px hover:shadow-md"
              }`}
              style={{
                background: th.background,
                color: th.textColor ?? (isDark(th.background) ? "#eee" : "#121212"),
                fontFamily: FONT_STACKS[th.fontFamily].css,
              }}
              onClick={() =>
                update({ background: th.background, textColor: th.textColor, fontWeight: th.fontWeight, ...(props.text ? { fontFamily: th.fontFamily } : {}) })
              }
            >
              <span className="text-[19px] leading-none" style={{ fontWeight: th.fontWeight === 400 ? 500 : 700 }}>
                Aa
              </span>
              <span className="text-[11px]" style={{ fontWeight: th.fontWeight === 600 ? 700 : 500 }}>
                {t(th.name)}
              </span>
            </button>
          ))}
        </div>
      </Section>

      <Section label={t("rs.background")}>
        <div className="grid grid-cols-3 gap-1.5">
          {BACKGROUNDS.map((b) => (
            <button
              key={b.color}
              className={`tw h-9 truncate rounded-lg border border-black/10 px-1.5 text-xs font-medium ${s.background === b.color ? "ring-2 ring-[var(--accent)] ring-offset-1 ring-offset-[var(--surface)]" : ""}`}
              style={{ background: b.color, color: isDark(b.color) ? "#eee" : "#333" }}
              title={t(b.name)}
              onClick={() => update({ background: b.color })}
            >
              {t(b.name)}
            </button>
          ))}
          <label
            className={`flex h-9 cursor-pointer items-center justify-center gap-1.5 rounded-lg border border-dashed border-[var(--line-strong)] text-xs font-medium text-[var(--muted)] ${
              custom ? "ring-2 ring-[var(--accent)] ring-offset-1 ring-offset-[var(--surface)]" : ""
            }`}
            title={t("rs.customColor")}
          >
            <input
              type="color"
              className="tw size-4 cursor-pointer overflow-hidden rounded"
              value={s.background}
              onChange={(e) => update({ background: e.target.value })}
            />
            {t("rs.custom")}
          </label>
        </div>
      </Section>

      <Section label={t("rs.textColor")}>
        <div className="flex items-center gap-2">
          <Choice
            value={s.textColor === null ? "auto" : "custom"}
            options={[
              ["auto", t("rs.auto")],
              ["custom", t("rs.custom")],
            ]}
            onChange={(v) => update({ textColor: v === "auto" ? null : isDark(s.background) ? "#d8d8d8" : "#222222" })}
          />
          {s.textColor !== null && (
            <input type="color" className="tw size-7 cursor-pointer rounded-md" value={s.textColor} onChange={(e) => update({ textColor: e.target.value })} />
          )}
        </div>
      </Section>

      {props.text && (
        <>
          <Section label={t("rs.font")}>
            <div className="segmented">
              {(Object.keys(FONT_STACKS) as FontFamily[]).map((f) => (
                <button key={f} className={s.fontFamily === f ? "on" : ""} style={{ fontFamily: FONT_STACKS[f].css }} onClick={() => update({ fontFamily: f })}>
                  {t(FONT_STACKS[f].name)}
                </button>
              ))}
            </div>
            <CustomFont value={s.customFont} onChange={(customFont) => update({ customFont })} />
          </Section>
          <Section label={t("rs.weight")}>
            <Choice
              value={s.fontWeight}
              options={FONT_WEIGHTS.map((w) => [w, t(WEIGHT_NAMES[w])] as [FontWeight, string])}
              onChange={(fontWeight) => update({ fontWeight })}
            />
          </Section>
          <Section>
            <Slider label={t("rs.fontSize")} value={s.fontSize} min={13} max={36} step={1} format={(v) => `${v}px`} onChange={(v) => update({ fontSize: v })} />
            <Slider
              label={t("rs.lineHeight")}
              value={s.lineHeight}
              min={1.3}
              max={2.6}
              step={0.1}
              format={(v) => v.toFixed(1)}
              onChange={(v) => update({ lineHeight: v })}
            />
            <Slider
              label={t("rs.paragraphSpacing")}
              value={s.paragraphSpacing}
              min={0}
              max={2}
              step={0.05}
              format={(v) => `${v.toFixed(2)}em`}
              onChange={(v) => update({ paragraphSpacing: v })}
            />
            <Slider label={t("rs.margin")} value={s.margin} min={0} max={120} step={4} format={(v) => `${v}px`} onChange={(v) => update({ margin: v })} />
            {s.flowMode === "scroll" && (
              <Slider
                label={t("rs.maxWidth")}
                value={s.maxWidth}
                min={480}
                max={1400}
                step={20}
                format={(v) => `${v}px`}
                onChange={(v) => update({ maxWidth: v })}
              />
            )}
            <div className="mt-3 flex gap-5">
              <Toggle label={t("rs.indent")} on={s.indent} onChange={(indent) => update({ indent })} />
              <Toggle label={t("rs.justify")} on={s.justify} onChange={(justify) => update({ justify })} />
            </div>
          </Section>
          <Section label={t("rs.pageTurn")}>
            <div className="flex flex-wrap gap-2">
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
          </Section>
        </>
      )}

      {props.paged && (
        <Section label={props.text ? t("settings.pagedBooks") : undefined}>
          <div className="my-2 flex items-center justify-between gap-3 text-[13px]">
            <span>{t("rs.pageLayout")}</span>
            <Choice
              value={s.pageLayout}
              options={[
                ["continuous", t("rs.continuous")],
                ["single", t("rs.single")],
                ["spread", t("rs.spread")],
              ]}
              onChange={(pageLayout) => update({ pageLayout })}
            />
          </div>
          <div className="my-2 flex items-center justify-between gap-3 text-[13px]">
            <span>{t("rs.fit")}</span>
            <Choice
              value={s.pageFit}
              options={[
                ["width", t("rs.fitWidth")],
                ["page", t("rs.fitPage")],
                ["height", t("rs.fitHeight")],
              ]}
              onChange={(pageFit) => update({ pageFit })}
            />
          </div>
          <Toggle label={t("rs.comicRtl")} on={s.comicRtl} onChange={(comicRtl) => update({ comicRtl })} />
        </Section>
      )}
    </>
  );
}

/** The reader's "Aa" popover. */
export default function SettingsPanel(props: { settings: Settings; update: UpdateSettings; textOptions: boolean; onMore: () => void }) {
  return (
    <div
      className="settings-panel absolute top-[calc(100%+8px)] right-0 z-[60] max-h-[calc(100vh-70px)] w-[352px] overflow-y-auto rounded-2xl border border-[var(--line)] bg-[var(--glass)] p-4 shadow-2xl backdrop-blur-xl backdrop-saturate-150"
      onClick={(e) => e.stopPropagation()}
    >
      <ReadingSettings settings={props.settings} update={props.update} text={props.textOptions} paged={false} />
      <div className="mt-3 flex justify-end border-t border-[var(--line)] pt-2.5">
        <button className="link-button" onClick={props.onMore}>
          {t("rs.more")}
        </button>
      </div>
    </div>
  );
}
