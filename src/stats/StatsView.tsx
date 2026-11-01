// Reading statistics: headline numbers, a calendar of the year and the year in review.

import { useEffect, useMemo, useState } from "react";
import { api, errorText } from "../api";
import { formatDate, formatDuration, language, t } from "../i18n";
import { Icon } from "../icons";
import type { ReadingReport } from "../types";
import { LEVELS, levelOf, localDay, weeksOf } from "./calendar";

function Tile(props: { label: string; value: string; hero?: boolean }) {
  return (
    <div className={`stat-tile${props.hero ? " hero" : ""}`}>
      <div className="stat-label">{props.label}</div>
      <div className="stat-value">{props.value}</div>
    </div>
  );
}

const hours = (seconds: number) => (seconds > 0 ? formatDuration(seconds) : "0");

/** One square per day, shaded by reading time. */
function Calendar({ year, report }: { year: number; report: ReadingReport }) {
  const [hover, setHover] = useState<{ day: string; left: number; top: number } | null>(null);
  const weeks = useMemo(() => weeksOf(year), [year]);
  const seconds = useMemo(() => new Map(report.days.map((d) => [d.day, d.seconds])), [report]);
  const monthName = (m: number) => new Date(year, m, 1).toLocaleDateString(language(), { month: "short" });
  const describe = (day: string) => {
    const s = seconds.get(day) ?? 0;
    return `${new Date(`${day}T00:00`).toLocaleDateString(language(), { dateStyle: "medium" })} · ${s > 0 ? formatDuration(s) : t("stats.noReading")}`;
  };
  return (
    <div className="calendar-wrap">
      <div className="calendar" role="img" aria-label={t("stats.calendarLabel", { year, days: report.days.length })} onMouseLeave={() => setHover(null)}>
        {weeks.map((w, x) => (
          <div key={x} className="calendar-week">
            <span className="calendar-month">{w.month !== null ? monthName(w.month) : ""}</span>
            {w.days.map((day, y) =>
              day ? (
                <span
                  key={y}
                  className={`calendar-day level-${levelOf(seconds.get(day) ?? 0)}`}
                  onMouseEnter={(e) => {
                    const box = e.currentTarget.closest(".calendar-wrap")!.getBoundingClientRect();
                    const cell = e.currentTarget.getBoundingClientRect();
                    setHover({ day, left: cell.left - box.left + cell.width / 2, top: cell.top - box.top });
                  }}
                />
              ) : (
                <span key={y} className="calendar-day blank" />
              ),
            )}
          </div>
        ))}
      </div>
      {hover && (
        <div className="calendar-tip" style={{ left: hover.left, top: hover.top }}>
          {describe(hover.day)}
        </div>
      )}
      <div className="calendar-legend muted small">
        <span>{t("stats.less")}</span>
        {[0, ...LEVELS.map((_, i) => i + 1)].map((l) => (
          <span key={l} className={`calendar-day level-${l}`} title={l ? t("stats.fromMinutes", { n: LEVELS[l - 1] }) : t("stats.noReading")} />
        ))}
        <span>{t("stats.more")}</span>
      </div>
    </div>
  );
}

/** The same numbers as the calendar, for reading rather than looking. */
function MonthTable({ year, report }: { year: number; report: ReadingReport }) {
  const months = useMemo(() => {
    const totals = Array.from({ length: 12 }, () => ({ seconds: 0, days: 0 }));
    for (const d of report.days) {
      const m = totals[Number(d.day.slice(5, 7)) - 1];
      m.seconds += d.seconds;
      m.days += 1;
    }
    return totals;
  }, [report]);
  return (
    <details className="stats-table">
      <summary className="muted small">{t("stats.byMonth")}</summary>
      <table>
        <thead>
          <tr>
            <th>{t("stats.month")}</th>
            <th>{t("stats.time")}</th>
            <th>{t("stats.activeDays")}</th>
          </tr>
        </thead>
        <tbody>
          {months.map((m, i) => (
            <tr key={i}>
              <td>{new Date(year, i, 1).toLocaleDateString(language(), { month: "long" })}</td>
              <td>{hours(m.seconds)}</td>
              <td>{m.days}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </details>
  );
}

export default function StatsView(props: { onBack: () => void; onOpen: (bookId: string) => void }) {
  const [year, setYear] = useState(() => new Date().getFullYear());
  const [report, setReport] = useState<ReadingReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api.readingReport(localDay(), year).then(
      (r) => alive && setReport(r),
      (e) => alive && setError(errorText(e)),
    );
    return () => {
      alive = false;
    };
  }, [year]);

  const years = report ? [...new Set([new Date().getFullYear(), ...report.years])].sort((a, b) => b - a) : [year];
  const top = report?.books[0]?.seconds ?? 0;
  return (
    <div className="review-view">
      <header className="review-head">
        <button onClick={props.onBack}>
          <Icon name="arrowLeft" size={15} /> {t("reader.shelf")}
        </button>
        <span className="review-title">{t("stats.title")}</span>
        <span className="spacer" />
        {years.length > 1 && (
          <select value={year} aria-label={t("stats.year")} onChange={(e) => setYear(Number(e.target.value))}>
            {years.map((y) => (
              <option key={y} value={y}>
                {y}
              </option>
            ))}
          </select>
        )}
      </header>
      {error && <div className="center-msg error">{error}</div>}
      {report && (
        <div className="stats-page">
          <div className="stat-row">
            <Tile hero label={t("stats.streak")} value={t("stats.days", { n: report.streak })} />
            <Tile label={t("stats.today")} value={hours(report.today)} />
            <Tile label={t("stats.week")} value={hours(report.last7Days)} />
            <Tile label={t("stats.month30")} value={hours(report.last30Days)} />
            <Tile label={t("stats.lifetime")} value={hours(report.lifetimeSeconds)} />
          </div>

          <section>
            <h3>{t("stats.calendar", { year })}</h3>
            <Calendar year={year} report={report} />
            <MonthTable year={year} report={report} />
            {report.years.length === 0 && <p className="muted small">{t("stats.startsNow")}</p>}
          </section>

          <section>
            <h3>{t("stats.yearReview", { year })}</h3>
            <div className="stat-row thirds">
              <Tile label={t("stats.yearTime")} value={hours(report.yearSeconds)} />
              <Tile label={t("stats.activeDays")} value={String(report.days.length)} />
              <Tile label={t("stats.longest")} value={t("stats.days", { n: report.longestStreak })} />
              <Tile label={t("stats.finishedCount")} value={String(report.finished.length)} />
              <Tile label={t("stats.highlights")} value={String(report.highlights)} />
              <Tile label={t("stats.reviews")} value={String(report.cardsReviewed)} />
            </div>
          </section>

          {report.books.length > 0 && (
            <section>
              <h3>{t("stats.topBooks")}</h3>
              <div className="stat-bars">
                {report.books.map((b) => (
                  <button key={b.bookId} className="stat-bar" onClick={() => props.onOpen(b.bookId)} title={b.title}>
                    <span className="ellipsis">{b.title}</span>
                    <span className="stat-track">
                      <span style={{ width: `${(b.seconds / top) * 100}%` }} />
                    </span>
                    <span className="stat-amount">{formatDuration(b.seconds)}</span>
                  </button>
                ))}
              </div>
            </section>
          )}

          {report.finished.length > 0 && (
            <section>
              <h3>{t("stats.finished")}</h3>
              {report.finished.map((b) => (
                <button key={b.id} className="stat-finished" onClick={() => props.onOpen(b.id)}>
                  <span className="ellipsis">{b.title}</span>
                  <span className="muted small">{formatDate(b.finished)}</span>
                </button>
              ))}
            </section>
          )}
        </div>
      )}
    </div>
  );
}
