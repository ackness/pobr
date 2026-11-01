//! Display-only unit formatting; values still retain their original integer evidence.
//! Handler scales/precision follow the pinned PyPoE TQNumberFormat definitions,
//! not an item/skill calculation law. No absent source value is synthesized.
use serde_json::Value;

pub(super) struct Number {
    value: i64,
    divisor: u64,
    precision: Option<usize>,
    fixed: bool,
}
impl Number {
    pub fn new(value: i64) -> Self {
        Self {
            value,
            divisor: 1,
            precision: None,
            fixed: false,
        }
    }
    pub fn transform(&mut self, name: &str) -> Result<(), String> {
        let integer = match name {
            "negate" => Some(-1),
            "double" => Some(2),
            "negate_and_double" => Some(-2),
            _ => None,
        };
        if let Some(scale) = integer {
            self.value = self
                .value
                .checked_mul(scale)
                .ok_or("CSD integer transform overflow")?;
            self.precision = None;
            self.fixed = false;
            return Ok(());
        }
        let (divisor, precision, fixed) = match name {
            "divide_by_one_hundred" => (100, None, false),
            "divide_by_ten_1dp_if_required" => (10, Some(1), false),
            "per_minute_to_per_second" => (60, Some(1), false),
            "per_minute_to_per_second_2dp_if_required" => (60, Some(2), false),
            "milliseconds_to_seconds" => (1000, None, false),
            "milliseconds_to_seconds_1dp" => (1000, Some(1), true),
            "milliseconds_to_seconds_2dp_if_required" => (1000, Some(2), false),
            _ => return Err(format!("unsupported CSD handler {name:?}")),
        };
        self.divisor = self
            .divisor
            .checked_mul(divisor)
            .ok_or("CSD divisor overflow")?;
        self.precision = precision;
        self.fixed = fixed;
        Ok(())
    }
    pub fn display(&self, positive_sign: bool) -> Result<String, String> {
        if self.divisor == 1 && self.precision.is_none() {
            return Ok(if positive_sign {
                format!("{:+}", self.value)
            } else {
                self.value.to_string()
            });
        }
        // All native stat values are signed i32. Reject lossy wider inputs rather
        // than silently discarding their low bits during unit conversion.
        if self.value.unsigned_abs() > (1u64 << 53) || self.divisor > (1u64 << 53) {
            return Err("CSD display value exceeds exact float integer range".into());
        }
        let value = self.value as f64 / self.divisor as f64;
        let mut out = if let Some(precision) = self.precision {
            format!("{value:.precision$}")
        } else if value.fract() == 0.0 {
            (value as i64).to_string()
        } else {
            general(value)
        };
        if self.precision.is_some() && !self.fixed && out.contains('.') {
            out = out.trim_end_matches('0').trim_end_matches('.').to_owned();
        }
        if positive_sign && !out.starts_with('-') {
            out.insert(0, '+');
        }
        Ok(out)
    }
}

pub(super) fn apply(values: &mut [Number], handlers: &[Value]) -> Result<(), String> {
    for q in handlers {
        let name = q["name_raw"].as_str().ok_or("invalid CSD handler name")?;
        let args = q["argument_tokens_raw"]
            .as_array()
            .ok_or("invalid CSD handler args")?;
        if name == "canonical_line" && args.is_empty() {
            continue;
        }
        let [arg] = args.as_slice() else {
            return Err("unsupported CSD handler arity".into());
        };
        let index = arg
            .as_str()
            .ok_or("invalid CSD handler index")?
            .parse::<usize>()
            .map_err(|e| e.to_string())?;
        let value = index
            .checked_sub(1)
            .and_then(|i| values.get_mut(i))
            .ok_or("CSD handler index out of bounds")?;
        value.transform(name)?;
    }
    Ok(())
}

// PyPoE's unqualified `n` format uses six significant digits in the C locale.
// Derive fixed/scientific notation from Rust's rounded six-digit mantissa.
fn general(value: f64) -> String {
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("finite scientific format");
    let exponent: i32 = exponent.parse().expect("scientific exponent");
    let sign = if mantissa.starts_with('-') { "-" } else { "" };
    let digits = mantissa.trim_start_matches('-').replace('.', "");
    if !(-4..6).contains(&exponent) {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        return format!("{mantissa}e{exponent:+03}");
    }
    let decimal = exponent + 1;
    let mut out = if decimal <= 0 {
        format!("{sign}0.{}{digits}", "0".repeat((-decimal) as usize))
    } else {
        let at = decimal as usize;
        format!("{sign}{}.{}", &digits[..at], &digits[at..])
    };
    if out.contains('.') {
        out = out.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    out
}
