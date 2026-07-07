// Fiscal calendar helpers. The fiscal year-end month comes from settings
// (default July — the corp's year ends Jul 31; see CLAUDE.md). "FY2024"
// is the year ending in 2024. With a July year end, quarters are
// Q1 Aug-Oct, Q2 Nov-Jan, Q3 Feb-Apr, Q4 May-Jul.
import { settings } from './settings.js'

// Fiscal years end at this month's end.
const fyEndMonth = () => Number(settings.fiscal_year_end_month) || 7

const pad = (n) => String(n).padStart(2, '0')
const ymd = (y, m, d) => `${y}-${pad(m)}-${pad(d)}`
const lastDay = (y, m) => new Date(y, m, 0).getDate()

export function todayISO() {
  const d = new Date()
  return ymd(d.getFullYear(), d.getMonth() + 1, d.getDate())
}

/** FY label year containing the given ISO date. */
export function fiscalYearOf(iso = todayISO()) {
  const [y, m] = iso.split('-').map(Number)
  return m <= fyEndMonth() ? y : y + 1
}

/** {from, to} for fiscal year `y`. */
export function fiscalYear(y) {
  const m = fyEndMonth()
  // Start is the month after the end month; a December year end means a
  // calendar year (no year-1 rollback).
  const from = m === 12 ? ymd(y, 1, 1) : ymd(y - 1, m + 1, 1)
  return { from, to: ymd(y, m, lastDay(y, m)) }
}

function quarter(y, q) {
  let m = fyEndMonth() + 1 + (q - 1) * 3
  let fy = y - 1
  if (m > 12) {
    m -= 12
    fy += 1
  }
  let em = m + 2
  let ey = fy
  if (em > 12) {
    em -= 12
    ey += 1
  }
  return { from: ymd(fy, m, 1), to: ymd(ey, em, lastDay(ey, em)) }
}

/** Recent fiscal periods for the quick picker: years (with quarters for
 * the two most recent) going back `years` fiscal years. */
export function fiscalPeriods(years = 7, today = todayISO()) {
  const cur = fiscalYearOf(today)
  const out = []
  for (let i = 0; i < years; i++) {
    const y = cur - i
    out.push({ label: `FY${y}`, ...fiscalYear(y) })
    if (i < 2) {
      for (let q = 1; q <= 4; q++) out.push({ label: `FY${y} Q${q}`, ...quarter(y, q) })
    }
  }
  return out
}
