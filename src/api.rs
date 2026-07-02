use crate::money;
use anyhow::{Result, anyhow};
use axum::extract::{Path as AxPath, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::path::Path;
use tower_http::services::{ServeDir, ServeFile};

/// Frontend assets compiled into the binary. `npm run build` in web/
/// must run before `cargo build`.
#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist"]
struct Assets;

async fn embedded_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let file = Assets::get(path).or_else(|| Assets::get("index.html"));
    match file {
        Some(f) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], f.data).into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// Hash of the web assets' modification times ("webhash" technique):
/// lets clients detect that the server is running newer frontend code.
/// FNV-1a over the sorted mtimes — change detection, not integrity.
fn web_hash(static_dir: Option<&Path>) -> String {
    let mut mtimes: Vec<u64> = match static_dir {
        Some(dir) => std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .chain(std::fs::read_dir(dir.join("assets")).into_iter().flatten().flatten())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .filter_map(|m| m.modified().ok())
            .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .collect(),
        None => Assets::iter()
            .filter_map(|p| Assets::get(&p))
            .filter_map(|f| f.metadata.last_modified())
            .collect(),
    };
    mtimes.sort_unstable();
    let mut h: u64 = 0xcbf29ce484222325;
    for t in mtimes {
        for b in t.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
    static_dir: Option<std::path::PathBuf>,
}

pub fn router(pool: SqlitePool, static_dir: Option<&Path>) -> Router {
    let state = AppState {
        pool,
        static_dir: static_dir.map(|p| p.to_path_buf()),
    };
    Router::new()
        .route("/api/ws", get(ws_handler))
        .route("/api/commodities", get(list_commodities))
        .route("/api/accounts", get(list_accounts).post(create_account))
        .route(
            "/api/accounts/{id}",
            axum::routing::put(update_account).delete(delete_account),
        )
        .route("/api/accounts/{id}/register", get(register))
        .route("/api/transactions", axum::routing::post(create_tx))
        .route(
            "/api/transactions/{id}",
            axum::routing::put(update_tx).delete(delete_tx),
        )
        .with_state(state)
        .pipe(|r| match static_dir {
            Some(dir) => r.fallback_service(
                ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html"))),
            ),
            None => r.fallback(embedded_asset),
        })
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

/// Anyhow-compatible error type that renders as JSON.
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError(StatusCode::BAD_REQUEST, format!("{e:#}"))
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

// ---------- accounts ----------

#[derive(Serialize)]
struct AccountOut {
    id: String,
    name: String,
    kind: String,
    commodity_id: Option<String>,
    parent_id: Option<String>,
    code: Option<String>,
    description: Option<String>,
    placeholder: bool,
    hidden: bool,
    /// Balance in the account's own commodity, formatted ("1234.56")
    balance: String,
    tx_count: i64,
}

async fn list_accounts(State(st): State<AppState>) -> ApiResult<Vec<AccountOut>> {
    let rows = sqlx::query(
        "SELECT a.id, a.name, a.kind, a.commodity_id, a.parent_id, a.code, a.description,
                a.placeholder, a.hidden
         FROM accounts a WHERE a.kind != 'ROOT'",
    )
    .fetch_all(&st.pool)
    .await?;

    // Exact balances: fold split quantities per account in Rust.
    let splits = sqlx::query(
        "SELECT account_id, quantity_num, quantity_denom, COUNT(*) OVER (PARTITION BY account_id) c
         FROM splits",
    )
    .fetch_all(&st.pool)
    .await?;
    let mut balances: HashMap<String, (i128, i128)> = HashMap::new();
    let mut counts: HashMap<String, i64> = HashMap::new();
    for s in &splits {
        let acct: String = s.get("account_id");
        let n: i64 = s.get("quantity_num");
        let d: i64 = s.get("quantity_denom");
        let e = balances.entry(acct.clone()).or_insert((0, 1));
        *e = money::add(*e, (n as i128, d as i128));
        *counts.entry(acct).or_insert(0) += 1;
    }

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let id: String = r.get("id");
        let (bn, bd) = balances.get(&id).copied().unwrap_or((0, 100));
        out.push(AccountOut {
            balance: money::format(bn, bd),
            tx_count: counts.get(&id).copied().unwrap_or(0),
            id,
            name: r.get("name"),
            kind: r.get("kind"),
            commodity_id: r.get("commodity_id"),
            parent_id: r.get("parent_id"),
            code: r.get("code"),
            description: r.get("description"),
            placeholder: r.get::<i64, _>("placeholder") != 0,
            hidden: r.get::<i64, _>("hidden") != 0,
        });
    }
    // Sort by code then name for a stable, ledger-like ordering.
    out.sort_by(|a, b| (&a.code, &a.name).cmp(&(&b.code, &b.name)));
    Ok(Json(out))
}

// ---------- websocket (connection status now; server push later) ----------

async fn ws_handler(
    State(st): State<AppState>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, st))
}

async fn handle_socket(mut socket: axum::extract::ws::WebSocket, st: AppState) {
    use axum::extract::ws::Message;
    // Greet with the current web-asset hash so the client can detect that
    // the server is running newer frontend code than the loaded page.
    let hello = serde_json::json!({
        "type": "hello",
        "web_hash": web_hash(st.static_dir.as_deref()),
    });
    if socket.send(Message::Text(hello.to_string().into())).await.is_err() {
        return;
    }
    // Periodic pings keep intermediaries from idling the connection out and
    // let the client detect a dead server quickly. Future server-push
    // messages will be JSON Text frames with a {"type": ...} envelope.
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(_)) => {} // pongs / client chatter; nothing to do yet
                    _ => break,
                }
            }
        }
    }
}

// ---------- account CRUD ----------

const ACCOUNT_KINDS: &[&str] = &[
    "ASSET", "BANK", "CASH", "LIABILITY", "CREDIT", "INCOME", "EXPENSE", "EQUITY", "TRADING",
];

#[derive(Deserialize)]
struct AccountIn {
    name: String,
    kind: String,
    commodity_id: String,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    placeholder: bool,
    #[serde(default)]
    hidden: bool,
}

async fn validate_account(pool: &SqlitePool, body: &AccountIn, self_id: Option<&str>) -> Result<()> {
    if body.name.trim().is_empty() {
        return Err(anyhow!("account name is required"));
    }
    if !ACCOUNT_KINDS.contains(&body.kind.as_str()) {
        return Err(anyhow!("invalid account type '{}'", body.kind));
    }
    let cmdty = sqlx::query("SELECT id FROM commodities WHERE id = ?")
        .bind(&body.commodity_id)
        .fetch_optional(pool)
        .await?;
    if cmdty.is_none() {
        return Err(anyhow!("unknown commodity {}", body.commodity_id));
    }
    if let Some(pid) = &body.parent_id {
        // Walk up from the proposed parent; it must exist and not pass
        // through the account itself (cycle).
        let mut cur = Some(pid.clone());
        let mut hops = 0;
        while let Some(id) = cur {
            if Some(id.as_str()) == self_id {
                return Err(anyhow!("parent would create a cycle"));
            }
            let row = sqlx::query("SELECT parent_id FROM accounts WHERE id = ?")
                .bind(&id)
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| anyhow!("unknown parent account {id}"))?;
            cur = row.get("parent_id");
            hops += 1;
            if hops > 100 {
                return Err(anyhow!("account tree too deep"));
            }
        }
    }
    Ok(())
}

async fn create_account(
    State(st): State<AppState>,
    Json(body): Json<AccountIn>,
) -> ApiResult<serde_json::Value> {
    validate_account(&st.pool, &body, None).await?;
    let id = new_guid();
    sqlx::query(
        "INSERT INTO accounts (id, name, kind, commodity_id, parent_id, code, description, placeholder, hidden)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(&body.kind)
    .bind(&body.commodity_id)
    .bind(&body.parent_id)
    .bind(&body.code)
    .bind(&body.description)
    .bind(body.placeholder)
    .bind(body.hidden)
    .execute(&st.pool)
    .await?;
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn update_account(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(body): Json<AccountIn>,
) -> ApiResult<serde_json::Value> {
    validate_account(&st.pool, &body, Some(&id)).await?;
    // Changing commodity under existing splits would corrupt quantities.
    let has_splits: i64 = sqlx::query("SELECT COUNT(*) c FROM splits WHERE account_id = ?")
        .bind(&id)
        .fetch_one(&st.pool)
        .await?
        .get("c");
    if has_splits > 0 {
        let old: Option<String> = sqlx::query("SELECT commodity_id FROM accounts WHERE id = ?")
            .bind(&id)
            .fetch_optional(&st.pool)
            .await?
            .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no such account".into()))?
            .get("commodity_id");
        if old.as_deref() != Some(body.commodity_id.as_str()) {
            return Err(anyhow!("cannot change commodity of an account with transactions").into());
        }
    }
    let res = sqlx::query(
        "UPDATE accounts SET name = ?, kind = ?, commodity_id = ?, parent_id = ?,
                             code = ?, description = ?, placeholder = ?, hidden = ?
         WHERE id = ? AND kind != 'ROOT'",
    )
    .bind(body.name.trim())
    .bind(&body.kind)
    .bind(&body.commodity_id)
    .bind(&body.parent_id)
    .bind(&body.code)
    .bind(&body.description)
    .bind(body.placeholder)
    .bind(body.hidden)
    .bind(&id)
    .execute(&st.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "no such account".into()));
    }
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn delete_account(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> ApiResult<serde_json::Value> {
    let splits: i64 = sqlx::query("SELECT COUNT(*) c FROM splits WHERE account_id = ?")
        .bind(&id)
        .fetch_one(&st.pool)
        .await?
        .get("c");
    if splits > 0 {
        return Err(anyhow!("account has {splits} splits; move or delete them first").into());
    }
    let children: i64 = sqlx::query("SELECT COUNT(*) c FROM accounts WHERE parent_id = ?")
        .bind(&id)
        .fetch_one(&st.pool)
        .await?
        .get("c");
    if children > 0 {
        return Err(anyhow!("account has {children} child accounts; delete or reparent them first").into());
    }
    let res = sqlx::query("DELETE FROM accounts WHERE id = ? AND kind != 'ROOT'")
        .bind(&id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "no such account".into()));
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}

#[derive(Serialize)]
struct CommodityOut {
    id: String,
    namespace: String,
    mnemonic: String,
    fraction: i64,
}

async fn list_commodities(State(st): State<AppState>) -> ApiResult<Vec<CommodityOut>> {
    let rows = sqlx::query("SELECT id, namespace, mnemonic, fraction FROM commodities ORDER BY id")
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(
        rows.iter()
            .map(|r| CommodityOut {
                id: r.get("id"),
                namespace: r.get("namespace"),
                mnemonic: r.get("mnemonic"),
                fraction: r.get("fraction"),
            })
            .collect(),
    ))
}

// ---------- register ----------

#[derive(Serialize, Clone)]
struct SplitOut {
    id: String,
    account_id: String,
    account_name: String,
    memo: Option<String>,
    reconcile_state: String,
    value: String,    // in tx currency
    quantity: String, // in account commodity
}

#[derive(Serialize)]
struct EntryOut {
    tx_id: String,
    date_posted: String,
    num: Option<String>,
    description: Option<String>,
    currency_id: String,
    /// This account's split
    split_id: String,
    memo: Option<String>,
    reconcile_state: String,
    amount: String,  // this account's quantity, signed
    balance: String, // running balance in account commodity
    splits: Vec<SplitOut>,
}

#[derive(Serialize)]
struct RegisterOut {
    account_id: String,
    account_name: String,
    commodity_id: Option<String>,
    entries: Vec<EntryOut>,
}

async fn register(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> ApiResult<RegisterOut> {
    let acct = sqlx::query("SELECT name, commodity_id FROM accounts WHERE id = ?")
        .bind(&id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no such account".into()))?;

    // All transactions touching this account, ordered; an account can appear
    // in a transaction via multiple splits (rare) — one entry per split.
    let rows = sqlx::query(
        "SELECT t.id tx_id, t.date_posted, t.num, t.description, t.currency_id, t.date_entered,
                s.id split_id, s.memo, s.reconcile_state, s.quantity_num, s.quantity_denom
         FROM splits s JOIN transactions t ON t.id = s.tx_id
         WHERE s.account_id = ?
         ORDER BY t.date_posted, t.date_entered, t.id",
    )
    .bind(&id)
    .fetch_all(&st.pool)
    .await?;

    // Fetch all sibling splits for these transactions in one query.
    let all_splits = sqlx::query(
        "SELECT s.tx_id, s.id, s.account_id, a.name account_name, s.memo, s.reconcile_state,
                s.value_num, s.value_denom, s.quantity_num, s.quantity_denom
         FROM splits s
         JOIN transactions t ON t.id = s.tx_id
         JOIN accounts a ON a.id = s.account_id
         WHERE s.tx_id IN (SELECT tx_id FROM splits WHERE account_id = ?)
         ORDER BY s.rowid",
    )
    .bind(&id)
    .fetch_all(&st.pool)
    .await?;
    let mut by_tx: HashMap<String, Vec<SplitOut>> = HashMap::new();
    for s in &all_splits {
        by_tx.entry(s.get("tx_id")).or_default().push(SplitOut {
            id: s.get("id"),
            account_id: s.get("account_id"),
            account_name: s.get("account_name"),
            memo: s.get("memo"),
            reconcile_state: s.get("reconcile_state"),
            value: money::format(s.get::<i64, _>("value_num") as i128, s.get::<i64, _>("value_denom") as i128),
            quantity: money::format(s.get::<i64, _>("quantity_num") as i128, s.get::<i64, _>("quantity_denom") as i128),
        });
    }

    let mut entries = Vec::with_capacity(rows.len());
    let mut running: (i128, i128) = (0, 1);
    for r in rows {
        let n = r.get::<i64, _>("quantity_num") as i128;
        let d = r.get::<i64, _>("quantity_denom") as i128;
        running = money::add(running, (n, d));
        let tx_id: String = r.get("tx_id");
        entries.push(EntryOut {
            splits: by_tx.get(&tx_id).cloned().unwrap_or_default(),
            tx_id,
            date_posted: r.get("date_posted"),
            num: r.get("num"),
            description: r.get("description"),
            currency_id: r.get("currency_id"),
            split_id: r.get("split_id"),
            memo: r.get("memo"),
            reconcile_state: r.get("reconcile_state"),
            amount: money::format(n, d),
            balance: money::format(running.0, running.1),
        });
    }

    Ok(Json(RegisterOut {
        account_id: id,
        account_name: acct.get("name"),
        commodity_id: acct.get("commodity_id"),
        entries,
    }))
}

// ---------- transaction CRUD ----------

#[derive(Deserialize)]
struct SplitIn {
    account_id: String,
    #[serde(default)]
    memo: Option<String>,
    /// Decimal string in transaction currency, e.g. "-123.45"
    value: String,
    /// Decimal string in account commodity; defaults to value (same currency)
    #[serde(default)]
    quantity: Option<String>,
    #[serde(default)]
    reconcile_state: Option<String>,
}

#[derive(Deserialize)]
struct TxIn {
    currency_id: String,
    date_posted: String,
    #[serde(default)]
    num: Option<String>,
    #[serde(default)]
    description: Option<String>,
    splits: Vec<SplitIn>,
}

fn new_guid() -> String {
    // 32 hex chars, GnuCash-guid style, from OS randomness.
    let mut bytes = [0u8; 16];
    getrandom_fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn getrandom_fill(buf: &mut [u8]) {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .expect("urandom");
}

async fn validate_and_insert(pool: &SqlitePool, tx_id: &str, body: &TxIn) -> Result<()> {
    if body.splits.len() < 2 {
        return Err(anyhow!("a transaction needs at least 2 splits"));
    }
    if body.date_posted.len() != 10 {
        return Err(anyhow!("date_posted must be YYYY-MM-DD"));
    }
    let currency_fraction: i64 =
        sqlx::query("SELECT fraction FROM commodities WHERE id = ?")
            .bind(&body.currency_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| anyhow!("unknown currency {}", body.currency_id))?
            .get("fraction");

    // Parse and check the splits balance to zero in transaction currency.
    let mut parsed = Vec::new();
    let mut sum: (i128, i128) = (0, 1);
    for s in &body.splits {
        let acct = sqlx::query(
            "SELECT a.commodity_id, c.fraction FROM accounts a
             LEFT JOIN commodities c ON c.id = a.commodity_id WHERE a.id = ?",
        )
        .bind(&s.account_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow!("unknown account {}", s.account_id))?;
        let acct_fraction: i64 = acct.try_get("fraction").unwrap_or(100);

        let value = money::parse(&s.value, currency_fraction)?;
        let quantity = match &s.quantity {
            Some(q) => money::parse(q, acct_fraction)?,
            None => money::parse(&s.value, acct_fraction)?,
        };
        sum = money::add(sum, (value.0 as i128, value.1 as i128));
        parsed.push((s, value, quantity));
    }
    if sum.0 != 0 {
        return Err(anyhow!(
            "splits do not balance: total {}",
            money::format(sum.0, sum.1)
        ));
    }

    let mut dbtx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO transactions (id, currency_id, num, date_posted, date_entered, description)
         VALUES (?, ?, ?, ?, datetime('now'), ?)",
    )
    .bind(tx_id)
    .bind(&body.currency_id)
    .bind(&body.num)
    .bind(&body.date_posted)
    .bind(&body.description)
    .execute(&mut *dbtx)
    .await?;
    for (s, value, quantity) in &parsed {
        sqlx::query(
            "INSERT INTO splits (id, tx_id, account_id, memo, reconcile_state,
                                 value_num, value_denom, quantity_num, quantity_denom, action)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(new_guid())
        .bind(tx_id)
        .bind(&s.account_id)
        .bind(&s.memo)
        .bind(s.reconcile_state.as_deref().unwrap_or("n"))
        .bind(value.0)
        .bind(value.1)
        .bind(quantity.0)
        .bind(quantity.1)
        .execute(&mut *dbtx)
        .await?;
    }
    dbtx.commit().await?;
    Ok(())
}

async fn create_tx(State(st): State<AppState>, Json(body): Json<TxIn>) -> ApiResult<serde_json::Value> {
    let id = new_guid();
    validate_and_insert(&st.pool, &id, &body).await?;
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn update_tx(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(body): Json<TxIn>,
) -> ApiResult<serde_json::Value> {
    let existing = sqlx::query("SELECT id FROM transactions WHERE id = ?")
        .bind(&id)
        .fetch_optional(&st.pool)
        .await?;
    if existing.is_none() {
        return Err(ApiError(StatusCode::NOT_FOUND, "no such transaction".into()));
    }
    // Replace wholesale: delete (cascades to splits) then re-insert with same id.
    sqlx::query("DELETE FROM transactions WHERE id = ?")
        .bind(&id)
        .execute(&st.pool)
        .await?;
    validate_and_insert(&st.pool, &id, &body).await?;
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn delete_tx(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> ApiResult<serde_json::Value> {
    let res = sqlx::query("DELETE FROM transactions WHERE id = ?")
        .bind(&id)
        .execute(&st.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "no such transaction".into()));
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}
