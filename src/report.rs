//! Report computation: balance sheet and income statement as generic
//! row tables the frontend renders directly (and typst will consume for
//! PDF export later). Layout decisions mirror the user's GnuCash report
//! options — see doc/decisions.md ("reports architecture").

use crate::api::{ApiError, ApiResult, AppState};
use crate::money;
use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use sqlx::{Row as _, SqlitePool};
use std::collections::HashMap;

/// Levels of subaccounts shown; deeper accounts roll up into their
/// ancestor at this depth. (GnuCash option, hardcoded per user prefs.)
const LEVELS: i64 = 3;
const CAD: &str = "CURRENCY:CAD";

#[derive(Serialize)]
pub(crate) struct ReportRow {
    kind: &'static str, // section | account | subtotal | total | net | blank
    depth: i64,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    account_id: Option<String>,
    /// CAD amount, formatted; None = blank cell
    #[serde(skip_serializing_if = "Option::is_none")]
    amount: Option<String>,
    /// Original-commodity annotation for foreign-currency accounts
    #[serde(skip_serializing_if = "Option::is_none")]
    foreign: Option<String>,
    /// Which of the `cols` amount columns the figure sits in (staircase)
    col: i64,
}

#[derive(Serialize)]
pub(crate) struct ReportOut {
    kind: &'static str,
    currency: &'static str,
    cols: i64,
    rows: Vec<ReportRow>,
}

struct Acct {
    id: String,
    name: String,
    kind: String,
    commodity_id: Option<String>,
    code: Option<String>,
    /// balance in own commodity (exact rational)
    own: (i128, i128),
    /// balance converted to CAD cents (rounded once per account)
    own_cents: i128,
}

struct Book {
    accounts: HashMap<String, Acct>,
    children: HashMap<Option<String>, Vec<String>>,
    /// own + all descendants, CAD cents
    totals: HashMap<String, i128>,
}

/// Load accounts with balances over the given date window, converted to
/// CAD at the price nearest on-or-before `to` (falling back to the
/// earliest price after, then 1:1).
async fn load_book(pool: &SqlitePool, from: Option<&str>, to: &str) -> Result<Book, ApiError> {
    let rows = sqlx::query(
        "SELECT id, name, kind, commodity_id, parent_id, code FROM accounts WHERE kind != 'ROOT'",
    )
    .fetch_all(pool)
    .await?;

    let splits = sqlx::query(
        "SELECT s.account_id, s.quantity_num, s.quantity_denom
         FROM splits s JOIN transactions t ON t.id = s.tx_id
         WHERE t.date_posted <= ? AND (? IS NULL OR t.date_posted >= ?)",
    )
    .bind(to)
    .bind(from)
    .bind(from)
    .fetch_all(pool)
    .await?;
    let mut balances: HashMap<String, (i128, i128)> = HashMap::new();
    for s in &splits {
        let acct: String = s.get("account_id");
        let n: i64 = s.get("quantity_num");
        let d: i64 = s.get("quantity_denom");
        let e = balances.entry(acct).or_insert((0, 1));
        *e = money::add(*e, (n as i128, d as i128));
    }

    // One price per foreign commodity, nearest the report end date.
    let mut prices: HashMap<String, (i128, i128)> = HashMap::new();
    let price_rows = sqlx::query(
        "SELECT commodity_id, value_num, value_denom FROM prices p
         WHERE currency_id = ? AND id = (
           SELECT id FROM prices q
           WHERE q.commodity_id = p.commodity_id AND q.currency_id = ?
           ORDER BY (q.date > ?), CASE WHEN q.date > ? THEN q.date ELSE '' END,
                    CASE WHEN q.date <= ? THEN q.date ELSE '' END DESC
           LIMIT 1)",
    )
    .bind(CAD)
    .bind(CAD)
    .bind(to)
    .bind(to)
    .bind(to)
    .fetch_all(pool)
    .await?;
    for p in &price_rows {
        let n: i64 = p.get("value_num");
        let d: i64 = p.get("value_denom");
        prices.insert(p.get("commodity_id"), (n as i128, d as i128));
    }

    let mut accounts = HashMap::new();
    let mut children: HashMap<Option<String>, Vec<String>> = HashMap::new();
    let ids: std::collections::HashSet<String> = rows.iter().map(|r| r.get("id")).collect();
    for r in rows {
        let id: String = r.get("id");
        let commodity_id: Option<String> = r.get("commodity_id");
        let own = balances.get(&id).copied().unwrap_or((0, 1));
        let price = match commodity_id.as_deref() {
            Some(c) if c != CAD => prices.get(c).copied().unwrap_or((1, 1)),
            _ => (1, 1),
        };
        let parent_id: Option<String> =
            r.get::<Option<String>, _>("parent_id").filter(|p| ids.contains(p));
        children.entry(parent_id.clone()).or_default().push(id.clone());
        accounts.insert(
            id.clone(),
            Acct {
                id,
                name: r.get("name"),
                kind: r.get("kind"),
                commodity_id,
                code: r.get("code"),
                own,
                own_cents: to_cents(own, price),
            },
        );
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| {
            let (aa, bb) = (&accounts[a], &accounts[b]);
            (&aa.code, &aa.name).cmp(&(&bb.code, &bb.name))
        });
    }

    // Post-order totals (own + descendants).
    let mut totals = HashMap::new();
    fn walk(
        id: &str,
        accounts: &HashMap<String, Acct>,
        children: &HashMap<Option<String>, Vec<String>>,
        totals: &mut HashMap<String, i128>,
    ) -> i128 {
        let mut t = accounts[id].own_cents;
        for c in children.get(&Some(id.to_string())).cloned().unwrap_or_default() {
            t += walk(&c, accounts, children, totals);
        }
        totals.insert(id.to_string(), t);
        t
    }
    for id in children.get(&None).cloned().unwrap_or_default() {
        walk(&id, &accounts, &children, &mut totals);
    }
    Ok(Book { accounts, children, totals })
}

/// bal * price, rounded (half away from zero) to cents.
fn to_cents(bal: (i128, i128), price: (i128, i128)) -> i128 {
    let num = bal.0 * price.0 * 100;
    let den = bal.1 * price.1;
    if den == 0 {
        return 0;
    }
    if (num >= 0) == (den > 0) { (num + den / 2) / den } else { (num - den / 2) / den }
}

fn fmt(cents: i128) -> String {
    money::format(cents, 100)
}

/// Emit one section (Assets / Liabilities / Expenses …): all top-level
/// accounts whose kind is in `kinds`, three levels deep, followed by a
/// grand-total row. `sign` = -1 for credit-normal sections so figures
/// read positive. Returns the (signed) section total in cents.
#[allow(clippy::too_many_arguments)]
fn emit_section(
    book: &Book,
    rows: &mut Vec<ReportRow>,
    label: &str,
    total_label: &str,
    kinds: &[&str],
    sign: i128,
    subtotals: bool,
) -> i128 {
    rows.push(ReportRow {
        kind: "section",
        depth: 0,
        label: label.into(),
        account_id: None,
        amount: None,
        foreign: None,
        col: 0,
    });
    let mut total = 0i128;
    for id in book.children.get(&None).cloned().unwrap_or_default() {
        if kinds.contains(&book.accounts[&id].kind.as_str()) {
            total += emit_account(book, rows, &id, 1, sign, subtotals);
        }
    }
    rows.push(ReportRow {
        kind: "total",
        depth: 0,
        label: total_label.into(),
        account_id: None,
        amount: Some(fmt(sign * total)),
        foreign: None,
        col: LEVELS - 1,
    });
    rows.push(blank());
    sign * total
}

fn emit_account(
    book: &Book,
    rows: &mut Vec<ReportRow>,
    id: &str,
    depth: i64,
    sign: i128,
    subtotals: bool,
) -> i128 {
    let a = &book.accounts[id];
    let total = book.totals[id];
    // "Include accounts with zero total balances" is off.
    if total == 0 && a.own_cents == 0 {
        return 0;
    }
    let kids: Vec<String> = book
        .children
        .get(&Some(id.to_string()))
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|c| book.totals[c] != 0 || book.accounts[c].own_cents != 0)
        .collect();
    let label = match &a.code {
        Some(c) if !c.is_empty() => format!("{} {}", c, a.name),
        _ => a.name.clone(),
    };
    let rolled_up = depth >= LEVELS || kids.is_empty();
    // Rolled-up rows carry own+descendants; open parents show only their
    // own balance ("Parent account balances: Account Balance").
    let shown = if rolled_up { total } else { a.own_cents };
    let foreign = match (&a.commodity_id, rolled_up && kids.is_empty()) {
        (Some(c), true) if c != CAD => {
            Some(format!("{} {}", c.split(':').next_back().unwrap_or(c), {
                let (n, d) = a.own;
                money::format(n, d)
            }))
        }
        _ => None,
    };
    rows.push(ReportRow {
        kind: "account",
        depth,
        label,
        account_id: Some(a.id.clone()),
        // "Omit zero balance figures" is on.
        amount: (shown != 0).then(|| fmt(sign * shown)),
        foreign,
        col: (LEVELS - depth).max(0),
    });
    if !rolled_up {
        for c in &kids {
            emit_account(book, rows, c, depth + 1, sign, subtotals);
        }
        if subtotals {
            rows.push(ReportRow {
                kind: "subtotal",
                depth,
                label: format!("Total {}", a.name),
                account_id: Some(a.id.clone()),
                amount: Some(fmt(sign * total)),
                foreign: None,
                col: (LEVELS - depth).max(0),
            });
        }
    }
    total
}

fn blank() -> ReportRow {
    ReportRow {
        kind: "blank",
        depth: 0,
        label: String::new(),
        account_id: None,
        amount: None,
        foreign: None,
        col: 0,
    }
}

fn kinds_total(book: &Book, kinds: &[&str]) -> i128 {
    book.accounts
        .values()
        .filter(|a| kinds.contains(&a.kind.as_str()))
        .map(|a| a.own_cents)
        .sum()
}

const ASSET_KINDS: &[&str] = &["ASSET", "BANK", "CASH", "RECEIVABLE"];
const LIABILITY_KINDS: &[&str] = &["LIABILITY", "CREDIT", "PAYABLE"];

#[derive(Deserialize)]
pub(crate) struct BalanceSheetParams {
    date: String,
}

pub(crate) async fn balance_sheet(
    State(st): State<AppState>,
    Query(p): Query<BalanceSheetParams>,
) -> ApiResult<ReportOut> {
    Ok(Json(balance_sheet_data(&st, &p).await?))
}

async fn balance_sheet_data(st: &AppState, p: &BalanceSheetParams) -> Result<ReportOut, ApiError> {
    let book = load_book(&st.pool, None, &p.date).await?;
    let mut rows = Vec::new();

    let assets = emit_section(&book, &mut rows, "Assets", "Total Assets", ASSET_KINDS, 1, true);
    let liab = emit_section(
        &book,
        &mut rows,
        "Liabilities",
        "Total Liabilities",
        LIABILITY_KINDS,
        -1,
        true,
    );

    // Equity: booked equity accounts plus computed retained earnings
    // (all income+expense to date) and trading gains/losses.
    rows.push(ReportRow {
        kind: "section",
        depth: 0,
        label: "Equity".into(),
        account_id: None,
        amount: None,
        foreign: None,
        col: 0,
    });
    let mut equity = 0i128;
    for id in book.children.get(&None).cloned().unwrap_or_default() {
        if book.accounts[&id].kind == "EQUITY" {
            equity += emit_account(&book, &mut rows, &id, 1, -1, true);
        }
    }
    let mut total_equity = -equity;
    let retained = -kinds_total(&book, &["INCOME", "EXPENSE"]);
    total_equity += retained;
    rows.push(ReportRow {
        kind: "total",
        depth: 0,
        label: "Retained Earnings".into(),
        account_id: None,
        amount: Some(fmt(retained)),
        foreign: None,
        col: LEVELS - 1,
    });
    let trading = -kinds_total(&book, &["TRADING"]);
    if trading != 0 {
        total_equity += trading;
        rows.push(ReportRow {
            kind: "total",
            depth: 0,
            label: (if trading >= 0 { "Trading Gains" } else { "Trading Losses" }).into(),
            account_id: None,
            amount: Some(fmt(trading.abs())),
            foreign: None,
            col: LEVELS - 1,
        });
    }
    rows.push(ReportRow {
        kind: "total",
        depth: 0,
        label: "Total Equity".into(),
        account_id: None,
        amount: Some(fmt(total_equity)),
        foreign: None,
        col: LEVELS - 1,
    });
    rows.push(blank());
    rows.push(ReportRow {
        kind: "total",
        depth: 0,
        label: "Total Liabilities & Equity".into(),
        account_id: None,
        amount: Some(fmt(liab + total_equity)),
        foreign: None,
        col: LEVELS - 1,
    });
    let _ = assets;

    Ok(ReportOut { kind: "balance-sheet", currency: "CAD", cols: LEVELS, rows })
}

#[derive(Deserialize)]
pub(crate) struct IncomeStatementParams {
    from: String,
    to: String,
}

// ---------- filtered account report ----------

#[derive(Serialize)]
struct AcctRow {
    tx_id: String,
    date: String,
    num: Option<String>,
    description: Option<String>,
    memo: Option<String>,
    reconcile_state: String,
    amount: String,
    running: String,
}

#[derive(Serialize)]
struct AcctGroup {
    account_id: String,
    account_name: String,
    commodity: String,
    rows: Vec<AcctRow>,
    total: String,
}

#[derive(Serialize)]
pub(crate) struct AccountReportOut {
    kind: &'static str,
    groups: Vec<AcctGroup>,
    /// Present only when every group shares one commodity.
    grand_total: Option<String>,
    grand_currency: Option<String>,
    count: usize,
}

/// Query params (all optional except `accounts`):
/// accounts=id,id — roots; subaccounts=1|0 (default 1); from/to — dates;
/// dir=all|in|out (debit/credit); reconciled=all|y|c|n|ny (ny = n or c);
/// filter=substring over description/memo/num (case-insensitive).
async fn account_report_data(
    st: &AppState,
    q: &HashMap<String, String>,
) -> Result<AccountReportOut, ApiError> {
    use axum::http::StatusCode;
    let opt = |k: &str| q.get(k).map(String::as_str).filter(|s| !s.is_empty());
    let roots: Vec<&str> = opt("accounts").map(|s| s.split(',').collect()).unwrap_or_default();
    if roots.is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "select at least one account".into()));
    }
    let include_sub = opt("subaccounts") != Some("0");
    let (from, to) = (opt("from"), opt("to"));
    let dir = opt("dir").unwrap_or("all");
    let reconciled = opt("reconciled").unwrap_or("all");
    let needle = opt("filter").map(str::to_lowercase);

    let accts = sqlx::query(
        "SELECT id, name, parent_id, commodity_id, code FROM accounts WHERE kind != 'ROOT'",
    )
    .fetch_all(&st.pool)
    .await?;
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut info: HashMap<String, (String, Option<String>, Option<String>)> = HashMap::new();
    for r in &accts {
        let id: String = r.get("id");
        if let Some(p) = r.get::<Option<String>, _>("parent_id") {
            children.entry(p).or_default().push(id.clone());
        }
        info.insert(id, (r.get("name"), r.get("commodity_id"), r.get("code")));
    }
    // Expand the selection to subtrees.
    let mut selected: Vec<String> = Vec::new();
    let mut queue: Vec<String> = roots.iter().map(|s| s.to_string()).collect();
    while let Some(id) = queue.pop() {
        if !info.contains_key(&id) || selected.contains(&id) {
            continue;
        }
        if include_sub {
            queue.extend(children.get(&id).cloned().unwrap_or_default());
        }
        selected.push(id);
    }
    selected.sort_by(|a, b| {
        let (an, _, ac) = &info[a];
        let (bn, _, bc) = &info[b];
        (ac, an).cmp(&(bc, bn))
    });

    let splits = sqlx::query(
        "SELECT s.account_id, s.memo, s.reconcile_state, s.quantity_num, s.quantity_denom,
                t.id tx_id, t.date_posted, t.date_entered, t.num, t.description
         FROM splits s JOIN transactions t ON t.id = s.tx_id",
    )
    .fetch_all(&st.pool)
    .await?;

    let mut groups = Vec::new();
    let mut count = 0;
    for acct_id in &selected {
        let (name, commodity, code) = info[acct_id].clone();
        let mut matched: Vec<(&sqlx::sqlite::SqliteRow, i64, i64)> = Vec::new();
        for s in &splits {
            if &s.get::<String, _>("account_id") != acct_id {
                continue;
            }
            let date: String = s.get("date_posted");
            if from.is_some_and(|f| date.as_str() < f) || to.is_some_and(|t| date.as_str() > t) {
                continue;
            }
            let qn: i64 = s.get("quantity_num");
            match dir {
                "in" if qn <= 0 => continue,
                "out" if qn >= 0 => continue,
                _ => {}
            }
            let state: String = s.get("reconcile_state");
            match reconciled {
                "all" => {}
                "ny" if state == "y" => continue,
                "y" | "c" | "n" if state != reconciled => continue,
                _ => {}
            }
            if let Some(needle) = &needle {
                let hay = format!(
                    "{} {} {}",
                    s.get::<Option<String>, _>("description").unwrap_or_default(),
                    s.get::<Option<String>, _>("memo").unwrap_or_default(),
                    s.get::<Option<String>, _>("num").unwrap_or_default(),
                )
                .to_lowercase();
                if !hay.contains(needle.as_str()) {
                    continue;
                }
            }
            matched.push((s, qn, s.get::<i64, _>("quantity_denom")));
        }
        if matched.is_empty() {
            continue;
        }
        matched.sort_by(|a, b| {
            let key = |r: &sqlx::sqlite::SqliteRow| {
                (
                    r.get::<String, _>("date_posted"),
                    r.get::<Option<String>, _>("date_entered"),
                    r.get::<String, _>("tx_id"),
                )
            };
            key(a.0).cmp(&key(b.0))
        });
        let mut running = (0i128, 1i128);
        let mut rows = Vec::new();
        for (s, qn, qd) in matched {
            running = money::add(running, (qn as i128, qd as i128));
            rows.push(AcctRow {
                tx_id: s.get("tx_id"),
                date: s.get("date_posted"),
                num: s.get("num"),
                description: s.get("description"),
                memo: s.get("memo"),
                reconcile_state: s.get("reconcile_state"),
                amount: money::format(qn as i128, qd as i128),
                running: money::format(running.0, running.1),
            });
        }
        count += rows.len();
        let label = match &code {
            Some(c) if !c.is_empty() => format!("{c} {name}"),
            _ => name.clone(),
        };
        groups.push(AcctGroup {
            account_id: acct_id.clone(),
            account_name: label,
            commodity: commodity
                .as_deref()
                .and_then(|c| c.split(':').next_back())
                .unwrap_or("?")
                .to_string(),
            total: money::format(running.0, running.1),
            rows,
        });
    }

    // Grand total only if the groups agree on a commodity.
    let grand = groups
        .first()
        .map(|g| g.commodity.clone())
        .filter(|c| groups.iter().all(|g| &g.commodity == c))
        .map(|c| {
            let mut t = (0i128, 1i128);
            for g in &groups {
                t = money::add(t, parse_amount(&g.total));
            }
            (money::format(t.0, t.1), c)
        });
    let (grand_total, grand_currency) = match grand {
        Some((t, c)) => (Some(t), Some(c)),
        None => (None, None),
    };

    Ok(AccountReportOut { kind: "account", groups, grand_total, grand_currency, count })
}

/// Parse a formatted decimal amount back to an exact rational.
fn parse_amount(s: &str) -> (i128, i128) {
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let denom = 10i128.pow(frac.len() as u32);
    let neg = int.starts_with('-');
    let int: i128 = int.trim_start_matches('-').parse().unwrap_or(0);
    let frac: i128 = if frac.is_empty() { 0 } else { frac.parse().unwrap_or(0) };
    let mag = int * denom + frac;
    (if neg { -mag } else { mag }, denom)
}

pub(crate) async fn account_report(
    State(st): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<AccountReportOut> {
    Ok(Json(account_report_data(&st, &q).await?))
}

/// Human summary of the active filters, for report subtitles.
fn account_report_subtitle(q: &HashMap<String, String>) -> String {
    let opt = |k: &str| q.get(k).map(String::as_str).filter(|s| !s.is_empty());
    let mut parts = Vec::new();
    parts.push(match (opt("from"), opt("to")) {
        (Some(f), Some(t)) => format!("{f} to {t}"),
        (Some(f), None) => format!("from {f}"),
        (None, Some(t)) => format!("through {t}"),
        (None, None) => "all dates".into(),
    });
    match opt("dir") {
        Some("in") => parts.push("debits only".into()),
        Some("out") => parts.push("credits only".into()),
        _ => {}
    }
    match opt("reconciled") {
        Some("y") => parts.push("reconciled only".into()),
        Some("c") => parts.push("cleared only".into()),
        Some("n") => parts.push("new only".into()),
        Some("ny") => parts.push("unreconciled only".into()),
        _ => {}
    }
    if let Some(f) = opt("filter") {
        parts.push(format!("matching \u{201c}{f}\u{201d}"));
    }
    parts.join(" · ")
}

// ---------- PDF export (typst) ----------

/// Escape text for use inside typst markup.
fn typst_esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '#' | '*' | '_' | '`' | '$' | '[' | ']' | '<' | '>' | '@' | '~') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Render a report as typst markup: staircase table, GnuCash-flavoured.
fn to_typst(rep: &ReportOut, title: &str, period: &str) -> String {
    let mut cells = String::new();
    let ncols = rep.cols as usize + 1;
    for r in &rep.rows {
        match r.kind {
            "blank" => {
                cells.push_str(&format!("table.cell(colspan: {ncols}, inset: (y: 6pt))[],\n"));
            }
            "section" => {
                cells.push_str(&format!(
                    "table.cell(colspan: {ncols}, fill: luma(235))[*{}*],\n",
                    typst_esc(&r.label)
                ));
            }
            _ => {
                let bold = matches!(r.kind, "total" | "net");
                let label = typst_esc(&r.label);
                let label = if bold { format!("*{label}*") } else { label };
                cells.push_str(&format!("[#h({}pt){label}],\n", r.depth * 14));
                for c in 0..rep.cols {
                    if c == r.col && r.amount.is_some() {
                        let amt = typst_esc(r.amount.as_deref().unwrap_or(""));
                        let amt = if bold { format!("*{amt}*") } else { amt };
                        let mut content = amt;
                        if let Some(f) = &r.foreign {
                            content.push_str(&format!(
                                "#linebreak()#text(size: 7.5pt, fill: luma(110))[{}]",
                                typst_esc(f)
                            ));
                        }
                        if matches!(r.kind, "subtotal" | "total" | "net") {
                            cells.push_str(&format!(
                                "table.cell(stroke: (top: 0.5pt + luma(60)))[{content}],\n"
                            ));
                        } else {
                            cells.push_str(&format!("[{content}],\n"));
                        }
                    } else {
                        cells.push_str("[],\n");
                    }
                }
            }
        }
    }
    let amount_cols = "8.5em, ".repeat(rep.cols as usize);
    let aligns = ", right".repeat(rep.cols as usize);
    format!(
        r#"#set page(paper: "us-letter", margin: (x: 2.2cm, y: 2.2cm))
#set text(size: 10pt)
#text(size: 14pt, weight: "bold")[{title}]
#h(0.8em)
#text(size: 10pt, fill: luma(100))[{period}]
#v(0.6em)
#table(
  columns: (1fr, {amount_cols}),
  align: (left{aligns}),
  stroke: none,
  inset: (x: 4pt, y: 2.5pt),
{cells})
#v(1em)
#text(size: 8pt, fill: luma(120))[All amounts in {currency}. Generated by kachet.]
"#,
        title = typst_esc(title),
        period = typst_esc(period),
        currency = rep.currency,
    )
}

/// Render an account report as typst markup: one transaction table per
/// account group, with per-group totals and an optional grand total.
fn account_to_typst(rep: &AccountReportOut, title: &str, subtitle: &str) -> String {
    let mut body = String::new();
    for g in &rep.groups {
        body.push_str(&format!(
            "#v(0.8em)\n#text(size: 11pt, weight: \"bold\")[{} #text(weight: \"regular\", fill: luma(100), size: 9pt)[({})]]\n#v(0.3em)\n",
            typst_esc(&g.account_name),
            typst_esc(&g.commodity),
        ));
        let mut cells = String::from(
            "table.header([*Date*], [*Num*], [*Description*], [*Memo*], [*R*], [*Amount*], [*Running*]),\n",
        );
        for r in &g.rows {
            cells.push_str(&format!(
                "[{}], [{}], [{}], [{}], [{}], [{}], [{}],\n",
                typst_esc(&r.date),
                typst_esc(r.num.as_deref().unwrap_or("")),
                typst_esc(r.description.as_deref().unwrap_or("")),
                typst_esc(r.memo.as_deref().unwrap_or("")),
                typst_esc(&r.reconcile_state),
                typst_esc(&r.amount),
                typst_esc(&r.running),
            ));
        }
        cells.push_str(&format!(
            "table.cell(colspan: 5)[*Total {}*], table.cell(stroke: (top: 0.5pt + luma(60)), colspan: 2)[*{}*],\n",
            typst_esc(&g.account_name),
            typst_esc(&g.total),
        ));
        body.push_str(&format!(
            "#table(\n  columns: (auto, auto, 1fr, 1fr, auto, 7em, 7em),\n  align: (left, left, left, left, center, right, right),\n  stroke: none,\n  inset: (x: 4pt, y: 2.5pt),\n  fill: (_, y) => if y == 0 {{ luma(235) }},\n{cells})\n",
        ));
    }
    if let (Some(t), Some(c)) = (&rep.grand_total, &rep.grand_currency) {
        if rep.groups.len() > 1 {
            body.push_str(&format!(
                "#v(0.6em)\n#align(right)[#text(weight: \"bold\")[Grand total: {} {}]]\n",
                typst_esc(t),
                typst_esc(c),
            ));
        }
    }
    format!(
        r#"#set page(paper: "us-letter", margin: (x: 2.2cm, y: 2.2cm))
#set text(size: 9.5pt)
#text(size: 14pt, weight: "bold")[{title}]
#h(0.8em)
#text(size: 10pt, fill: luma(100))[{subtitle}]
{body}
#v(1em)
#text(size: 8pt, fill: luma(120))[{count} entries. Generated by kachet.]
"#,
        title = typst_esc(title),
        subtitle = typst_esc(subtitle),
        count = rep.count,
    )
}

pub(crate) async fn report_pdf(
    State(st): State<AppState>,
    axum::extract::Path(kind): axum::extract::Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<axum::response::Response, ApiError> {
    use axum::http::{StatusCode, header};
    use axum::response::IntoResponse;
    let get = |k: &str| {
        q.get(k).cloned().ok_or_else(|| {
            ApiError(StatusCode::BAD_REQUEST, format!("missing query parameter '{k}'"))
        })
    };
    if kind == "account" {
        let rep = account_report_data(&st, &q).await?;
        let title = q.get("title").cloned().unwrap_or_else(|| "Account Report".to_string());
        let subtitle = account_report_subtitle(&q);
        let pdf = crate::pdf::compile(account_to_typst(&rep, &title, &subtitle))
            .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("typst: {e}")))?;
        let filename = format!("{title}.pdf").replace(['/', '\\', '"'], "-");
        return Ok((
            [
                (header::CONTENT_TYPE, "application/pdf".to_string()),
                (header::CONTENT_DISPOSITION, format!("inline; filename=\"{filename}\"")),
            ],
            pdf,
        )
            .into_response());
    }
    let (rep, default_title, period) = match kind.as_str() {
        "balance-sheet" => {
            let p = BalanceSheetParams { date: get("date")? };
            let period = p.date.clone();
            (balance_sheet_data(&st, &p).await?, "Balance Sheet", period)
        }
        "income-statement" => {
            let p = IncomeStatementParams { from: get("from")?, to: get("to")? };
            let period = format!("{} to {}", p.from, p.to);
            (income_statement_data(&st, &p).await?, "Income Statement", period)
        }
        other => {
            return Err(ApiError(StatusCode::NOT_FOUND, format!("unknown report kind '{other}'")));
        }
    };
    let title = q.get("title").cloned().unwrap_or_else(|| default_title.to_string());
    let source = to_typst(&rep, &title, &period);
    let pdf = crate::pdf::compile(source)
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("typst: {e}")))?;
    let filename = format!("{} {}.pdf", title, period).replace(['/', '\\', '"'], "-");
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (header::CONTENT_DISPOSITION, format!("inline; filename=\"{filename}\"")),
        ],
        pdf,
    )
        .into_response())
}

pub(crate) async fn income_statement(
    State(st): State<AppState>,
    Query(p): Query<IncomeStatementParams>,
) -> ApiResult<ReportOut> {
    Ok(Json(income_statement_data(&st, &p).await?))
}

async fn income_statement_data(
    st: &AppState,
    p: &IncomeStatementParams,
) -> Result<ReportOut, ApiError> {
    let book = load_book(&st.pool, Some(&p.from), &p.to).await?;
    let mut rows = Vec::new();

    let revenue =
        emit_section(&book, &mut rows, "Revenues", "Total Revenue", &["INCOME"], -1, false);
    let trading_raw = kinds_total(&book, &["TRADING"]);
    if trading_raw != 0 {
        emit_section(
            &book,
            &mut rows,
            "Trading",
            "Total Trading",
            &["TRADING"],
            -1,
            false,
        );
    }
    let expenses =
        emit_section(&book, &mut rows, "Expenses", "Total Expenses", &["EXPENSE"], 1, false);

    let net = revenue - trading_raw - expenses;
    rows.push(ReportRow {
        kind: "net",
        depth: 0,
        label: (if net >= 0 { "Net income for Period" } else { "Net loss for Period" }).into(),
        account_id: None,
        amount: Some(fmt(net.abs())),
        foreign: None,
        col: LEVELS - 1,
    });

    Ok(ReportOut { kind: "income-statement", currency: "CAD", cols: LEVELS, rows })
}
