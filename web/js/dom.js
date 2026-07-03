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
  if (rr.top < cr.top + headH) {
    container.scrollTop += rr.top - (cr.top + headH)
  } else if (rr.bottom > cr.bottom) {
    container.scrollTop += rr.bottom - cr.bottom
  }
}

/** Format a signed amount string into debit/credit columns. */
export function fmtSigned(amount) {
  if (amount.startsWith('-')) return { debit: '', credit: amount.slice(1) }
  if (parseFloat(amount) === 0) return { debit: '', credit: '' }
  return { debit: amount, credit: '' }
}
