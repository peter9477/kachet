//! Exchange-rate fetching from the Bank of Canada Valet API.
//!
//! The book's historical prices match BoC series (verified against the
//! GnuCash import): the legacy *closing* rate (IEXE0102) before 2017,
//! and the single daily indicative rate (FXUSDCAD) that replaced the
//! noon/closing pair in April 2017. Fetches fill only missing dates —
//! prices entered by hand are never overwritten.

use crate::api::{ApiError, ApiResult, AppState, new_guid};
use crate::money;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use sqlx::Row as _;
use std::collections::HashSet;

const CAD: &str = "CURRENCY:CAD";
/// First date of the modern indicative-rate series.
const MODERN_START: &str = "2017-01-03";

#[derive(Deserialize)]
pub(crate) struct FetchParams {
    /// Fetch up to this date (the client's today).
    end: String,
}

pub(crate) async fn fetch_boc(
    State(st): State<AppState>,
    Json(p): Json<FetchParams>,
) -> ApiResult<serde_json::Value> {
    // Foreign currencies actually used by the book.
    let currencies: Vec<String> = sqlx::query(
        "SELECT DISTINCT commodity_id FROM accounts
         WHERE commodity_id LIKE 'CURRENCY:%' AND commodity_id != ?
         UNION
         SELECT DISTINCT currency_id FROM transactions WHERE currency_id != ?",
    )
    .bind(CAD)
    .bind(CAD)
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|r| r.get(0))
    .collect();

    let client = reqwest::Client::new();
    let mut added = 0u64;
    let mut series_used = Vec::new();

    for cur in &currencies {
        let mnemonic = cur.split(':').next_back().unwrap_or(cur);
        // Earliest date this currency appears (transaction or account use).
        let start: Option<String> = sqlx::query_scalar(
            "SELECT MIN(t.date_posted) FROM transactions t
             LEFT JOIN splits s ON s.tx_id = t.id
             LEFT JOIN accounts a ON a.id = s.account_id
             WHERE t.currency_id = ?1 OR a.commodity_id = ?1",
        )
        .bind(cur)
        .fetch_one(&st.pool)
        .await?;
        let Some(start) = start else { continue };
        if start > p.end {
            continue;
        }

        let existing: HashSet<String> =
            sqlx::query_scalar("SELECT date FROM prices WHERE commodity_id = ? AND currency_id = ?")
                .bind(cur)
                .bind(CAD)
                .fetch_all(&st.pool)
                .await?
                .into_iter()
                .collect();

        // (series, span) pairs: legacy closing before 2017, modern after.
        let mut spans: Vec<(String, String, String)> = Vec::new();
        if start < MODERN_START.to_string() && mnemonic == "USD" {
            spans.push(("IEXE0102".into(), start.clone(), p.end.clone().min("2016-12-31".into())));
        }
        let modern_start = start.max(MODERN_START.to_string());
        if modern_start <= p.end {
            spans.push((format!("FX{mnemonic}CAD"), modern_start, p.end.clone()));
        }

        for (series, from, to) in spans {
            let url = format!(
                "https://www.bankofcanada.ca/valet/observations/{series}/json?start_date={from}&end_date={to}"
            );
            let resp = client.get(&url).send().await.map_err(|e| {
                ApiError(StatusCode::BAD_GATEWAY, format!("Bank of Canada request failed: {e}"))
            })?;
            if !resp.status().is_success() {
                return Err(ApiError(
                    StatusCode::BAD_GATEWAY,
                    format!("Bank of Canada returned {} for {series}", resp.status()),
                ));
            }
            let body: serde_json::Value = resp.json().await.map_err(|e| {
                ApiError(StatusCode::BAD_GATEWAY, format!("bad Valet response: {e}"))
            })?;
            let obs = body["observations"].as_array().cloned().unwrap_or_default();
            for o in &obs {
                let (Some(date), Some(v)) = (o["d"].as_str(), o[&series]["v"].as_str()) else {
                    continue;
                };
                if existing.contains(date) {
                    continue;
                }
                let frac_len = v.split('.').nth(1).map(|f| f.len()).unwrap_or(0) as u32;
                let (num, denom) = money::parse(v, 10i64.pow(frac_len))?;
                sqlx::query(
                    "INSERT INTO prices (id, commodity_id, currency_id, date, source, kind,
                                         value_num, value_denom)
                     VALUES (?, ?, ?, ?, 'bank-of-canada', 'last', ?, ?)",
                )
                .bind(new_guid())
                .bind(cur)
                .bind(CAD)
                .bind(date)
                .bind(num)
                .bind(denom)
                .execute(&st.pool)
                .await?;
                added += 1;
            }
            series_used.push(series);
        }
    }

    Ok(Json(serde_json::json!({
        "added": added,
        "currencies": currencies,
        "series": series_used,
    })))
}
