// The year as a grid of weeks for the reading calendar, and how a day's reading is shaded.

/** A date as `YYYY-MM-DD` in local time. */
export function localDay(d: Date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Minutes a day must reach for each shade; fixed, so years can be compared. */
export const LEVELS = [1, 15, 30, 60];

/** The shade of a day: 0 for no reading, up to `LEVELS.length`. */
export function levelOf(seconds: number): number {
  const minutes = seconds / 60;
  return LEVELS.reduce((level, from, i) => (minutes >= from ? i + 1 : level), 0);
}

export interface CalendarWeek {
  /** Monday to Sunday; `null` for days outside the year. */
  days: (string | null)[];
  /** The month (0–11) whose first day falls in this week. */
  month: number | null;
}

/** Weeks of `year`, each running Monday to Sunday. */
export function weeksOf(year: number): CalendarWeek[] {
  const weeks: CalendarWeek[] = [];
  let week: CalendarWeek = { days: new Array(7).fill(null), month: null };
  for (let d = new Date(year, 0, 1); d.getFullYear() === year; d.setDate(d.getDate() + 1)) {
    const weekday = (d.getDay() + 6) % 7;
    week.days[weekday] = localDay(d);
    if (d.getDate() === 1) week.month = d.getMonth();
    if (weekday === 6) {
      weeks.push(week);
      week = { days: new Array(7).fill(null), month: null };
    }
  }
  if (week.days.some(Boolean)) weeks.push(week);
  return weeks;
}
