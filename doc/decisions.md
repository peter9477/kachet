# Design decisions

## 2026-07-07 — Account report + settings storage

**Account report** (kind `account`): multi-account transaction listing —
roots expand to subtrees (toggleable), date range, debit/credit filter,
reconcile-state filter (any/y/c/n/unreconciled), case-insensitive
substring filter over description/memo/num. Grouped per account with
running totals and subtotals; grand total only when all groups share a
commodity (amounts are in each account's own commodity, deliberately
unconverted — statements should show booked figures). Covers receivables
statements, HST-quarter revenue, and payroll/source-deduction summaries
without dedicated report kinds. Same JSON→Vue and JSON→typst-PDF paths
as the other reports.

**UI state** persists server-side under the settings key `ui_state`
(one key, one JSON value — the state is a single logical value that must
change atomically): open tabs (registers with their jump stacks and
per-account selected rows, reports with their live params, settings
tab), active tab, sidebar collapse. Saved with an 800ms debounce plus a
`keepalive` flush on unload, restored on boot before first render; a
session therefore survives shutdowns and moves across browsers/machines.
Register selection is remembered by transaction id with an index
fallback, and re-scrolled into view on restore. Saved report *configs*
remain a separate concern in `report_configs` (named, reusable
documents); open report tabs reference them by id. Legacy localStorage
tab state is read as a one-time fallback.

**Settings** live server-side in a `settings` table: one row per key,
value JSON-encoded. Chosen over a single JSON blob because per-key
writes stay atomic (no read-modify-write races), individual settings are
inspectable/queryable in SQL, and future migrations touch one key at a
time. Client loads them before first render (`web/js/settings.js`,
reactive) and edits them in a Settings tab (Ctrl+O launcher). First
setting: `fiscal_year_end_month` (default 7) — the fiscal-period picker
and income-statement defaults now follow it; a December year end
degenerates correctly to calendar years.

## 2026-07-07 — PDF export via embedded typst

`GET /api/reports/{kind}/pdf?…&title=…` compiles the same report data
the live view renders into a PDF, using the typst compiler as a Rust
crate (typst 0.15 + typst-pdf + typst-assets bundled fonts — no external
toolchain). `src/pdf.rs` is a minimal typst `World`: one in-memory
source, embedded fonts, no filesystem/package access; `report.rs`
generates the typst markup (`to_typst`), so live view and PDF can never
disagree. UI: "PDF" button / Ctrl+P in the report toolbar, opens in a
new tab (browser viewer doubles as print preview). The interim
print-stylesheet idea is superseded. Note: user-supplied text is escaped
(`typst_esc`) before embedding in markup.

Rationale log for decisions that aren't obvious from the code. Newest first.
Add an entry whenever we settle something that a future reader might want to
relitigate — what we chose, what we rejected, and why.

## 2026-07-07 — Retained earnings: kachet deliberately differs from GnuCash

kachet's balance sheet computes retained earnings as −Σ(all income and
expense split balances to date), always over **every** account. A
2026-07 discrepancy against GnuCash was investigated and resolved:
GnuCash saved-report configs store a fixed account list, and the user's
balance sheet config predated a later-created expense account — its
retained earnings silently omitted that account (by exactly its
balance), printing a sheet that violated A = L + E. GnuCash's own
`balance-sheet.scm` confirms RE is a plain per-commodity balance sum, so
account selection is the only way it can drift. kachet reports
deliberately have no account selection: they always cover the whole
book, so this failure mode can't occur. (Ruled out along the way, for
posterity: closing entries — none flagged in the book; price-source
revaluation — the book's USD accounts all net to zero, so prices cannot
move RE.)

## 2026-07-07 — FX rates from the Bank of Canada

`POST /api/prices/fetch-boc` (UI: "Fetch FX rates…" in the Ctrl+O
launcher) fills missing daily rates from the BoC Valet API, from each
foreign currency's first use in the book through today. Existing price
rows are never touched, so manual prices win. Convention verified
against the imported GnuCash book: the pre-2017 entries match BoC's
legacy **closing** rate (IEXE0102), not the noon rate; from 2017 on, the
single daily indicative rate (`FX<CUR>CAD`, published ~16:30 ET)
replaced both and matches the book exactly. Rows are tagged
`source='bank-of-canada'`. Non-USD currencies only have the modern
series wired (legacy per-currency closing codes can be added if ever
needed).

Terms of use (bankofcanada.ca/terms, checked 2026-07-07): content is
freely usable with attribution (the `source` tag + UI label; add a
"Source: Bank of Canada" footnote if rates ever appear on shared/
exported reports) and without circumventing API rate limits (we make
1-2 bulk range requests per explicit button press — no polling). Rates
are "indicative only", same convention GnuCash used and CRA accepts.

## 2026-07-07 — Reports v1 implementation notes

Implements the architecture below. Specifics worth knowing:

- `src/report.rs` returns a generic row table (`section | account |
  subtotal | total | net | blank` rows with a `col` staircase index);
  the frontend renders it dumbly, and the future typst PDF template will
  consume the same JSON.
- CAD conversion happens once per account: the balance in the account's
  commodity times the price nearest the report end date (on-or-before
  preferred, else earliest after, else 1:1), rounded half-away to cents.
  Foreign accounts also carry their original-commodity figure for display.
- Balance sheet retained earnings = −Σ(income+expense to date); trading
  accounts appear as "Trading Gains/Losses". Income statement nets
  revenue − expenses − trading over the period.
- Report kinds hardcode the user's GnuCash display options (3 levels,
  omit zero, parent rows show own balance; subtotals on the balance
  sheet only).
- Saved configs live in `report_configs` (name, kind, params JSON) with
  CRUD at `/api/report-configs`; "Save as new" = copy. The sidebar lists
  them; Ctrl+O opens a keyboard launcher (new report / saved reports).
- Fiscal periods (`web/js/fiscal.js`): FY ends Jul 31; FY *N* ends in
  calendar year *N*; quarters are Aug–Oct, Nov–Jan, Feb–Apr, May–Jul.

## 2026-07-06 — Tabs sidebar, mouse parity, and reports architecture

**Tabs.** Navigation is a GnuCash-style list of open tabs in a left sidebar:
the Accounts tree is a permanent first tab; registers (and reports, once they
exist) open as closable tabs. All tabs stay mounted (`v-show`, not `v-if`) so
each keeps its scroll/selection state; an inactive tab ignores keys via an
`active` prop and refreshes its data when reactivated. Register tabs keep the
old jump/back stack internally (Ctrl+J pushes, Esc pops; Esc at the bottom
switches to Accounts, leaving the tab open). Open tabs persist in
localStorage across reloads. Fixing GnuCash's fixed-width uncollapsible tab
list was an explicit goal: the sidebar collapses to a thin clickable strip,
toggled by Ctrl+B or mouse; Ctrl+[ / Ctrl+] cycle tabs (Ctrl+Tab and
Ctrl+PgUp/PgDn are browser-reserved).

**Mouse parity (new convention).** Keyboard-first stands, but no feature may
be keyboard-*only*: everything needs a mouse path too. Known debt: register
new/insert/duplicate/jump have no mouse affordance yet (needs a small
toolbar or context menu).

**Reports.** Three report kinds to start, all hardcoded to the user's
GnuCash configurations (3 levels of subaccounts, omit zero balances, CAD
overall with foreign currencies shown; balance sheet shows parent subtotals,
income statement doesn't): balance sheet, income statement, and a filtered
account report (date range/direction/sign/reconciled/content filters) that
covers receivables statements, HST-return revenue, and payroll source
deductions without dedicated report types. Architecture decided:

- **Live view:** server computes the report as JSON (`/api/reports/...`,
  exact string amounts); Vue renders it. Keeps account-name hyperlinks into
  registers, live refresh, and app-consistent styling.
- **Saved report configs** (name + kind + parameters, e.g. "Balance Sheet
  2024") are stored server-side and can be copied/edited/deleted, GnuCash
  "Save Config" style; multiple years stay open as tabs.
- **Export:** typst compiled server-side (typst is a Rust crate — no
  external toolchain) renders PDF from the same JSON. Typst's *HTML* export
  was rejected for the live view: still experimental/feature-flagged as of
  typst 0.15 (2026-06). Interim until the typst path lands: print stylesheet
  + browser print-to-PDF, which may prove sufficient permanently.

## 2026-07-06 — Keyboard shortcuts: Ctrl-chords, not bare letters

**Decision.** Register commands are bound to Ctrl+key chords. Bare-letter
shortcuts (the earlier `n`/`e`/`c`/`j`/`d`) are removed as a general rule:
a stray keypress in a bookkeeping app must not open editors or delete
transactions. Non-letter keys with low accident cost stay: arrows/PgUp/PgDn/
Home/End for movement, `[` `]` `{` `}` for date jumps, Space for split
expansion, Enter for edit, Esc/Backspace for back. The account tree keeps
type-to-filter (letters there are input, not commands) with Insert/F2/Delete
plus chord alternates. Single letters may selectively return later if chords
prove sufficient protection elsewhere; assume not.

**Why Ctrl (the literal Control key, on every OS):**

- Browsers reserve tab/window shortcuts at a level pages cannot intercept —
  `preventDefault()` never sees them. In Chrome and Firefox on Windows/Linux
  that is **Ctrl+N, Ctrl+T, Ctrl+W** (and Shift variants, Ctrl+Tab, Ctrl+F4).
  On macOS the same set lives on **Cmd**, plus OS-level Cmd+Q/H/M/Tab. This is
  why "new" is Ctrl+Enter, not Ctrl+N.
- Every *other* Ctrl+letter is interceptable in both browsers on all three
  OSes. Their defaults (Ctrl+D bookmark, Ctrl+S save page, Ctrl+J downloads…)
  suppress cleanly and cost nothing inside kachet.
- On macOS the browser's own shortcuts are on Cmd, so Ctrl+letter is nearly
  all free — the same physical binding works everywhere. Mac-specific keys to
  avoid: Ctrl+Space (input source switch), Ctrl+arrows (Mission Control).
  Ctrl+N/E/A/K are Emacs cursor bindings inside focused text fields on macOS,
  so chords are only claimed in list/register mode, never while an input has
  focus (editor mode handles its own keys).
- **Alt was rejected:** on macOS Option+letter types special characters (must
  match `e.code`, since `e.key` becomes "ß" etc.), on Windows Firefox a bare
  Alt press focuses the menu bar, and Linux window managers grab Alt combos
  unpredictably. (Alt+S survives as a legacy alternate for add-split-line.)
- Caveat: Firefox users can set `permissions.default.shortcuts=2` to forbid
  pages from overriding any browser shortcut. Non-default; not designed for.

**The register keymap:**

| Chord | Action |
|---|---|
| Ctrl+Enter | new entry, dated today |
| Ctrl+I | insert — new entry dated same as selected entry; in editor mode, add split line |
| Ctrl+D | duplicate selected entry (today's date, Num cleared, reconcile reset) |
| Ctrl+E (or Enter) | edit selected entry |
| Ctrl+S (or Space) | toggle split expansion |
| Ctrl+J | jump to other account |
| Delete (⌘⌫ on Mac) | delete, with confirm |

Implementation notes: match `e.code` (`'KeyD'`), not `e.key`, so chords
survive macOS character composition; require `!e.altKey && !e.metaKey` so
Ctrl-chords don't swallow OS combos. Hint labels are platform-switched in
`web/js/keys.js` (`⌃D` vs `Ctrl+D`).

**Considered, deferred:** Ctrl+C/V/X as copy-entry / paste-as-new / cut. They
are interceptable, but the etiquette is to pass them through whenever text is
selected or an input is focused; do that check if/when implemented.

Sources: [overriding browser shortcuts from JS](https://www.robin-drexler.com/2015/07/07/overriding-default-browser-shortcuts),
[Mozilla bug 380637](https://bugzilla.mozilla.org/show_bug.cgi?id=380637),
[Ctrl+W not overridable in Firefox](https://support.mozilla.org/en-US/questions/1318792).
