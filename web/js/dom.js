// Scroll a table row fully into view inside a scroll container with a
// sticky <thead>. scrollIntoView({block:'nearest'}) can't be used: it
// aligns rows to the scrollport edge, leaving them hidden under the
// sticky header, and considers partially-visible rows close enough.

/** @param {HTMLElement?} container @param {HTMLElement?} row */
export function ensureRowVisible(container, row) {
  if (!container || !row) return
  const headH = container.querySelector('thead')?.offsetHeight ?? 0
  const cr = container.getBoundingClientRect()
  const rr = row.getBoundingClientRect()
  // clientHeight excludes a horizontal scrollbar, which cr.bottom (the
  // border-box edge) would let cover the bottom row.
  const top = cr.top + container.clientTop + headH
  const bottom = cr.top + container.clientTop + container.clientHeight
  if (rr.top < top) {
    container.scrollTop += rr.top - top
  } else if (rr.bottom > bottom) {
    container.scrollTop += rr.bottom - bottom
  }
}

// PgUp/PgDn jump: one screenful of rows less one row of overlap, so from
// the bottom visible row a jump lands on the top visible row (and vice
// versa) without scrolling, and repeats page with continuity.
/** @param {HTMLElement?} container */
export function pageJump(container) {
  const row = container?.querySelector('tbody tr')
  if (!row || !row.offsetHeight) return 20
  const headH = container.querySelector('thead')?.offsetHeight ?? 0
  return Math.max(1, Math.floor((container.clientHeight - headH) / row.offsetHeight) - 1)
}

/** Format a signed amount string into debit/credit columns. */
export function fmtSigned(amount) {
  if (amount.startsWith('-')) return { debit: '', credit: amount.slice(1) }
  if (parseFloat(amount) === 0) return { debit: '', credit: '' }
  return { debit: amount, credit: '' }
}
