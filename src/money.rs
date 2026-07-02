use anyhow::{Result, anyhow, bail};

/// Exact rational amount arithmetic. Denominators are powers of ten in
/// practice (GnuCash SCUs), but addition handles the general case.
pub fn add(a: (i128, i128), b: (i128, i128)) -> (i128, i128) {
    if a.1 == b.1 {
        return (a.0 + b.0, a.1);
    }
    let g = gcd(a.1, b.1);
    let d = a.1 / g * b.1;
    (a.0 * (d / a.1) + b.0 * (d / b.1), d)
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.abs().max(1)
}

/// Format num/denom as a plain decimal string, e.g. (-45654, 100) -> "-456.54".
pub fn format(num: i128, denom: i128) -> String {
    if denom <= 0 {
        return format!("{num}/{denom}");
    }
    let scale = pow10_scale(denom);
    let Some(scale) = scale else {
        // Non-decimal denominator: render with 6 places.
        return format!("{:.6}", num as f64 / denom as f64);
    };
    let neg = num < 0;
    let abs = num.unsigned_abs();
    let base = 10u128.pow(scale);
    let (int, frac) = (abs / base, abs % base);
    let sign = if neg { "-" } else { "" };
    if scale == 0 {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}.{frac:0width$}", width = scale as usize)
    }
}

fn pow10_scale(denom: i128) -> Option<u32> {
    let mut d = denom;
    let mut s = 0;
    while d > 1 {
        if d % 10 != 0 {
            return None;
        }
        d /= 10;
        s += 1;
    }
    Some(s)
}

/// Parse a decimal string like "-1,234.56" into num/denom with the given
/// commodity fraction (e.g. 100). Fails if more precision than the fraction allows.
pub fn parse(s: &str, fraction: i64) -> Result<(i64, i64)> {
    let s: String = s.chars().filter(|c| *c != ',' && *c != '$' && !c.is_whitespace()).collect();
    if s.is_empty() {
        bail!("empty amount");
    }
    let scale = pow10_scale(fraction as i128).ok_or_else(|| anyhow!("bad fraction {fraction}"))?;
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s.as_str(), ""),
    };
    if frac_part.len() > scale as usize {
        bail!("amount '{s}' has more decimal places than allowed ({scale})");
    }
    let neg = int_part.starts_with('-');
    let int_digits = int_part.trim_start_matches(['-', '+']);
    let int_val: i64 = if int_digits.is_empty() { 0 } else { int_digits.parse()? };
    let frac_val: i64 = if frac_part.is_empty() {
        0
    } else {
        frac_part.parse::<i64>()? * 10i64.pow(scale - frac_part.len() as u32)
    };
    let mag = int_val
        .checked_mul(fraction)
        .and_then(|v| v.checked_add(frac_val))
        .ok_or_else(|| anyhow!("amount overflow"))?;
    Ok((if neg { -mag } else { mag }, fraction))
}

/// Parse a GnuCash fraction string like "44677/100".
pub fn parse_gnc_fraction(s: &str) -> Result<(i64, i64)> {
    let (n, d) = s.split_once('/').ok_or_else(|| anyhow!("bad fraction '{s}'"))?;
    Ok((n.trim().parse()?, d.trim().parse()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        assert_eq!(format(-45654, 100), "-456.54");
        assert_eq!(format(0, 100), "0.00");
        assert_eq!(format(5, 100), "0.05");
        assert_eq!(parse("456.54", 100).unwrap(), (45654, 100));
        assert_eq!(parse("-1,234.5", 100).unwrap(), (-123450, 100));
        assert_eq!(parse("7", 100).unwrap(), (700, 100));
        assert!(parse("1.234", 100).is_err());
    }

    #[test]
    fn rational_add() {
        assert_eq!(add((1, 100), (1, 10)), (11, 100));
        assert_eq!(add((5, 100), (-5, 100)), (0, 100));
    }
}
