use crate::money::parse_gnc_fraction;
use anyhow::{Context, Result};
use quick_xml::Reader;
use quick_xml::events::Event;
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Default)]
pub struct ImportStats {
    pub commodities: usize,
    pub accounts: usize,
    pub transactions: usize,
    pub splits: usize,
    pub prices: usize,
}

#[derive(Default, Clone)]
struct CommodityRef {
    space: String,
    id: String,
}

impl CommodityRef {
    fn key(&self) -> String {
        format!("{}:{}", self.space, self.id)
    }
    fn is_currency(&self) -> bool {
        self.space == "CURRENCY" || self.space == "ISO4217"
    }
}

#[derive(Default)]
struct Account {
    id: String,
    name: String,
    kind: String,
    commodity: Option<CommodityRef>,
    parent: Option<String>,
    code: Option<String>,
    description: Option<String>,
    notes: Option<String>,
    placeholder: bool,
    hidden: bool,
}

#[derive(Default)]
struct Split {
    id: String,
    account: String,
    memo: Option<String>,
    action: Option<String>,
    reconcile: String,
    value: (i64, i64),
    quantity: (i64, i64),
}

#[derive(Default)]
struct Transaction {
    id: String,
    currency: Option<CommodityRef>,
    num: Option<String>,
    date_posted: Option<String>, // from ts:date, date part
    gdate_posted: Option<String>, // from date-posted slot, preferred
    date_entered: Option<String>,
    description: Option<String>,
    splits: Vec<Split>,
}

#[derive(Default)]
struct Price {
    id: String,
    commodity: Option<CommodityRef>,
    currency: Option<CommodityRef>,
    date: Option<String>,
    source: Option<String>,
    kind: Option<String>,
    value: (i64, i64),
}

#[derive(Default)]
struct Parsed {
    commodities: BTreeMap<String, (String, String, i64)>, // key -> (namespace, mnemonic, fraction)
    accounts: Vec<Account>,
    transactions: Vec<Transaction>,
    prices: Vec<Price>,
}

impl Parsed {
    fn note_commodity(&mut self, c: &CommodityRef, fraction: Option<i64>) {
        if c.space == "template" {
            return;
        }
        let entry = self
            .commodities
            .entry(c.key())
            .or_insert_with(|| (c.space.clone(), c.id.clone(), 100));
        if let Some(f) = fraction {
            entry.2 = f;
        }
    }
}

pub async fn import_file(pool: &SqlitePool, path: &Path) -> Result<ImportStats> {
    let raw = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let xml = if raw.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut out)?;
        out
    } else {
        raw
    };
    let parsed = parse(&xml)?;
    insert(pool, parsed).await
}

fn date_part(ts: &str) -> String {
    ts.split_whitespace().next().unwrap_or(ts).to_string()
}

fn parse(xml: &[u8]) -> Result<Parsed> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);

    let mut parsed = Parsed::default();
    let mut path: Vec<String> = Vec::new();
    let mut text = String::new();

    let mut cur_acc: Option<Account> = None;
    let mut cur_tx: Option<Transaction> = None;
    let mut cur_split: Option<Split> = None;
    let mut cur_price: Option<Price> = None;
    // Top-level <gnc:commodity> definition being read
    let mut cur_cmdty_def: Option<(CommodityRef, Option<i64>)> = None;
    // Commodity reference being accumulated (act:commodity, trn:currency, ...)
    let mut cmdty_ref = CommodityRef::default();
    // Stack of slot keys (slots nest via "frame" values)
    let mut slot_keys: Vec<String> = Vec::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "gnc:account" => cur_acc = Some(Account::default()),
                    "gnc:transaction" => cur_tx = Some(Transaction::default()),
                    "trn:split" => cur_split = Some(Split::default()),
                    "price" => cur_price = Some(Price::default()),
                    "gnc:commodity" => cur_cmdty_def = Some((CommodityRef::default(), None)),
                    "act:commodity" | "trn:currency" | "price:commodity" | "price:currency" => {
                        cmdty_ref = CommodityRef::default();
                    }
                    _ => {}
                }
                path.push(name);
                text.clear();
            }
            Event::Text(t) => {
                text.push_str(&t.unescape()?);
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let parent = path.get(path.len().saturating_sub(2)).cloned().unwrap_or_default();
                let value = std::mem::take(&mut text);
                let value = value.trim().to_string();

                match name.as_str() {
                    // ---- commodity refs and definitions ----
                    "cmdty:space" => {
                        if let Some((c, _)) = cur_cmdty_def.as_mut().filter(|_| parent == "gnc:commodity") {
                            c.space = value;
                        } else {
                            cmdty_ref.space = value;
                        }
                    }
                    "cmdty:id" => {
                        if let Some((c, _)) = cur_cmdty_def.as_mut().filter(|_| parent == "gnc:commodity") {
                            c.id = value;
                        } else {
                            cmdty_ref.id = value;
                        }
                    }
                    "cmdty:fraction" => {
                        if let Some((_, f)) = cur_cmdty_def.as_mut() {
                            *f = value.parse().ok();
                        }
                    }
                    "gnc:commodity" => {
                        if let Some((c, fraction)) = cur_cmdty_def.take() {
                            // Currencies default to fraction 100 unless specified
                            parsed.note_commodity(&c, fraction.or(Some(100)));
                        }
                    }
                    "act:commodity" => {
                        if let Some(a) = cur_acc.as_mut() {
                            parsed.note_commodity(&cmdty_ref, None);
                            a.commodity = Some(cmdty_ref.clone());
                        }
                    }
                    "trn:currency" => {
                        if let Some(t) = cur_tx.as_mut() {
                            parsed.note_commodity(&cmdty_ref, None);
                            t.currency = Some(cmdty_ref.clone());
                        }
                    }
                    "price:commodity" => {
                        if let Some(p) = cur_price.as_mut() {
                            parsed.note_commodity(&cmdty_ref, None);
                            p.commodity = Some(cmdty_ref.clone());
                        }
                    }
                    "price:currency" => {
                        if let Some(p) = cur_price.as_mut() {
                            parsed.note_commodity(&cmdty_ref, None);
                            p.currency = Some(cmdty_ref.clone());
                        }
                    }

                    // ---- account fields ----
                    "act:name" => cur_acc.as_mut().map(|a| a.name = value).unwrap_or(()),
                    "act:id" => cur_acc.as_mut().map(|a| a.id = value).unwrap_or(()),
                    "act:type" => cur_acc.as_mut().map(|a| a.kind = value).unwrap_or(()),
                    "act:code" => cur_acc.as_mut().map(|a| a.code = Some(value)).unwrap_or(()),
                    "act:description" => cur_acc.as_mut().map(|a| a.description = Some(value)).unwrap_or(()),
                    "act:parent" => cur_acc.as_mut().map(|a| a.parent = Some(value)).unwrap_or(()),
                    "gnc:account" => {
                        if let Some(a) = cur_acc.take() {
                            parsed.accounts.push(a);
                        }
                    }

                    // ---- slots (accounts + transactions) ----
                    "slot:key" => slot_keys.push(value),
                    "slot:value" => {
                        if slot_keys.len() == 1 {
                            let key = slot_keys[0].as_str();
                            if let Some(a) = cur_acc.as_mut() {
                                match key {
                                    "placeholder" => a.placeholder = value == "true",
                                    "hidden" => a.hidden = value == "true",
                                    "notes" => {
                                        if !value.is_empty() {
                                            a.notes = Some(value);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "gdate" => {
                        if cur_tx.is_some()
                            && cur_split.is_none()
                            && slot_keys.last().map(String::as_str) == Some("date-posted")
                        {
                            cur_tx.as_mut().unwrap().gdate_posted = Some(value);
                        }
                    }
                    "slot" => {
                        slot_keys.pop();
                    }

                    // ---- transaction fields ----
                    "trn:id" => cur_tx.as_mut().map(|t| t.id = value).unwrap_or(()),
                    "trn:num" => cur_tx.as_mut().map(|t| t.num = Some(value)).unwrap_or(()),
                    "trn:description" => cur_tx.as_mut().map(|t| t.description = Some(value)).unwrap_or(()),
                    "ts:date" => match parent.as_str() {
                        "trn:date-posted" => {
                            if let Some(t) = cur_tx.as_mut() {
                                t.date_posted = Some(date_part(&value));
                            }
                        }
                        "trn:date-entered" => {
                            if let Some(t) = cur_tx.as_mut() {
                                t.date_entered = Some(value);
                            }
                        }
                        "price:time" => {
                            if let Some(p) = cur_price.as_mut() {
                                p.date = Some(date_part(&value));
                            }
                        }
                        _ => {}
                    },
                    "gnc:transaction" => {
                        if let Some(t) = cur_tx.take() {
                            parsed.transactions.push(t);
                        }
                    }

                    // ---- split fields ----
                    "split:id" => cur_split.as_mut().map(|s| s.id = value).unwrap_or(()),
                    "split:memo" => cur_split.as_mut().map(|s| s.memo = Some(value)).unwrap_or(()),
                    "split:action" => cur_split.as_mut().map(|s| s.action = Some(value)).unwrap_or(()),
                    "split:reconciled-state" => cur_split.as_mut().map(|s| s.reconcile = value).unwrap_or(()),
                    "split:account" => cur_split.as_mut().map(|s| s.account = value).unwrap_or(()),
                    "split:value" => {
                        if let Some(s) = cur_split.as_mut() {
                            s.value = parse_gnc_fraction(&value)?;
                        }
                    }
                    "split:quantity" => {
                        if let Some(s) = cur_split.as_mut() {
                            s.quantity = parse_gnc_fraction(&value)?;
                        }
                    }
                    "trn:split" => {
                        if let (Some(t), Some(s)) = (cur_tx.as_mut(), cur_split.take()) {
                            t.splits.push(s);
                        }
                    }

                    // ---- price fields ----
                    "price:id" => cur_price.as_mut().map(|p| p.id = value).unwrap_or(()),
                    "price:source" => cur_price.as_mut().map(|p| p.source = Some(value)).unwrap_or(()),
                    "price:type" => cur_price.as_mut().map(|p| p.kind = Some(value)).unwrap_or(()),
                    "price:value" => {
                        if let Some(p) = cur_price.as_mut() {
                            p.value = parse_gnc_fraction(&value)?;
                        }
                    }
                    "price" => {
                        if let Some(p) = cur_price.take() {
                            parsed.prices.push(p);
                        }
                    }
                    _ => {}
                }
                path.pop();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(parsed)
}

async fn insert(pool: &SqlitePool, parsed: Parsed) -> Result<ImportStats> {
    let mut stats = ImportStats::default();
    let mut tx = pool.begin().await?;

    for (key, (ns, mnemonic, fraction)) in &parsed.commodities {
        sqlx::query(
            "INSERT INTO commodities (id, namespace, mnemonic, fraction) VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET fraction = excluded.fraction",
        )
        .bind(key)
        .bind(ns)
        .bind(mnemonic)
        .bind(fraction)
        .execute(&mut *tx)
        .await?;
        stats.commodities += 1;
    }

    for a in &parsed.accounts {
        // The template account tree (scheduled transactions) has non-currency
        // commodities; skip anything not tied to a real commodity except ROOT.
        if a.kind != "ROOT" && a.commodity.as_ref().is_none_or(|c| c.space == "template") {
            continue;
        }
        sqlx::query(
            "INSERT OR REPLACE INTO accounts
             (id, name, kind, commodity_id, parent_id, code, description, notes, placeholder, hidden)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&a.id)
        .bind(&a.name)
        .bind(&a.kind)
        .bind(a.commodity.as_ref().map(|c| c.key()))
        .bind(&a.parent)
        .bind(&a.code)
        .bind(&a.description)
        .bind(&a.notes)
        .bind(a.placeholder)
        .bind(a.hidden)
        .execute(&mut *tx)
        .await?;
        stats.accounts += 1;
    }

    for t in &parsed.transactions {
        let currency = t
            .currency
            .as_ref()
            .filter(|c| c.is_currency())
            .with_context(|| format!("transaction {} has no currency", t.id))?;
        let date = t
            .gdate_posted
            .clone()
            .or_else(|| t.date_posted.clone())
            .with_context(|| format!("transaction {} has no date", t.id))?;
        sqlx::query(
            "INSERT OR REPLACE INTO transactions
             (id, currency_id, num, date_posted, date_entered, description)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&t.id)
        .bind(currency.key())
        .bind(&t.num)
        .bind(&date)
        .bind(&t.date_entered)
        .bind(&t.description)
        .execute(&mut *tx)
        .await?;
        stats.transactions += 1;

        for s in &t.splits {
            sqlx::query(
                "INSERT OR REPLACE INTO splits
                 (id, tx_id, account_id, memo, action, reconcile_state,
                  value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&s.id)
            .bind(&t.id)
            .bind(&s.account)
            .bind(&s.memo)
            .bind(&s.action)
            .bind(&s.reconcile)
            .bind(s.value.0)
            .bind(s.value.1)
            .bind(s.quantity.0)
            .bind(s.quantity.1)
            .execute(&mut *tx)
            .await?;
            stats.splits += 1;
        }
    }

    for p in &parsed.prices {
        let (Some(c), Some(cur), Some(date)) = (&p.commodity, &p.currency, &p.date) else {
            continue;
        };
        sqlx::query(
            "INSERT OR REPLACE INTO prices
             (id, commodity_id, currency_id, date, source, kind, value_num, value_denom)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&p.id)
        .bind(c.key())
        .bind(cur.key())
        .bind(date)
        .bind(&p.source)
        .bind(&p.kind)
        .bind(p.value.0)
        .bind(p.value.1)
        .execute(&mut *tx)
        .await?;
        stats.prices += 1;
    }

    tx.commit().await?;
    Ok(stats)
}
