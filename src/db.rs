use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS commodities (
    id        TEXT PRIMARY KEY,           -- "CURRENCY:CAD"
    namespace TEXT NOT NULL,
    mnemonic  TEXT NOT NULL,
    fraction  INTEGER NOT NULL DEFAULT 100
);

CREATE TABLE IF NOT EXISTS accounts (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    kind         TEXT NOT NULL,           -- ASSET BANK LIABILITY INCOME EXPENSE EQUITY TRADING ROOT
    commodity_id TEXT REFERENCES commodities(id),
    parent_id    TEXT REFERENCES accounts(id),
    code         TEXT,
    description  TEXT,
    notes        TEXT,
    placeholder  INTEGER NOT NULL DEFAULT 0,
    hidden       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS transactions (
    id           TEXT PRIMARY KEY,
    currency_id  TEXT NOT NULL REFERENCES commodities(id),
    num          TEXT,
    date_posted  TEXT NOT NULL,           -- ISO date YYYY-MM-DD
    date_entered TEXT,
    description  TEXT
);

CREATE TABLE IF NOT EXISTS splits (
    id              TEXT PRIMARY KEY,
    tx_id           TEXT NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    account_id      TEXT NOT NULL REFERENCES accounts(id),
    memo            TEXT,
    action          TEXT,
    reconcile_state TEXT NOT NULL DEFAULT 'n',
    value_num       INTEGER NOT NULL,     -- in transaction currency
    value_denom     INTEGER NOT NULL,
    quantity_num    INTEGER NOT NULL,     -- in account commodity
    quantity_denom  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS prices (
    id           TEXT PRIMARY KEY,
    commodity_id TEXT NOT NULL REFERENCES commodities(id),
    currency_id  TEXT NOT NULL REFERENCES commodities(id),
    date         TEXT NOT NULL,
    source       TEXT,
    kind         TEXT,
    value_num    INTEGER NOT NULL,
    value_denom  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL                 -- JSON-encoded
);

CREATE TABLE IF NOT EXISTS report_configs (
    id     TEXT PRIMARY KEY,
    name   TEXT NOT NULL,
    kind   TEXT NOT NULL,               -- balance-sheet | income-statement
    params TEXT NOT NULL DEFAULT '{}'   -- JSON, shape depends on kind
);

CREATE INDEX IF NOT EXISTS idx_splits_account ON splits(account_id);
CREATE INDEX IF NOT EXISTS idx_splits_tx ON splits(tx_id);
CREATE INDEX IF NOT EXISTS idx_tx_date ON transactions(date_posted);
CREATE INDEX IF NOT EXISTS idx_prices_date ON prices(commodity_id, date);

-- Seed common currencies so a fresh database is usable before any import.
INSERT OR IGNORE INTO commodities (id, namespace, mnemonic, fraction) VALUES
    ('CURRENCY:CAD', 'CURRENCY', 'CAD', 100),
    ('CURRENCY:USD', 'CURRENCY', 'USD', 100);
"#;

pub async fn open(path: &Path) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().connect_with(opts).await?;
    sqlx::raw_sql(SCHEMA).execute(&pool).await?;
    Ok(pool)
}
