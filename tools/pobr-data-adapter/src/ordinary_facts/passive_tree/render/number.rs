//! Display-only unit formatting; values still retain their original integer evidence.
//! Handler scales/precision follow the pinned PyPoE TQNumberFormat definitions,
//! not an item/skill calculation law. No absent source value is synthesized.
use serde_json::Value;

pub(super) struct Number {
    value: i64,
    pub(super) format: pobr_data::catalog::stat_descriptions::StatValueFormat,
}
impl Number {
    pub fn new(value: i64) -> Self {
        Self {
            value,
            format: Default::default(),
        }
    }
    pub fn transform(&mut self, name: &str) -> Result<(), String> {
        let (multiplier, divisor, addend, precision, fixed) = match name {
            "negate" => (-1, 1, 0, None, false),
            "double" => (2, 1, 0, None, false),
            "negate_and_double" => (-2, 1, 0, None, false),
            "times_twenty" => (20, 1, 0, None, false),
            "multiply_by_four" => (4, 1, 0, None, false),
            "add_one" => (1, 1, 1, None, false),
            "subtract_one" => (1, 1, -1, None, false),
            "plus_two_hundred" => (1, 1, 200, None, false),
            "multiplicative_damage_modifier" => (1, 1, 100, None, false),
            "divide_by_one_hundred_and_negate" => (-1, 100, 0, None, false),
            "divide_by_one_hundred" => (1, 100, 0, None, false),
            "divide_by_one_hundred_0dp" => (1, 100, 0, Some(0), false),
            "divide_by_one_hundred_1dp" => (1, 100, 0, Some(1), true),
            "divide_by_one_hundred_2dp" => (1, 100, 0, Some(2), true),
            "divide_by_one_hundred_2dp_if_required" => (1, 100, 0, Some(2), false),
            "divide_by_ten_0dp" => (1, 10, 0, Some(0), false),
            "divide_by_ten_1dp" => (1, 10, 0, Some(1), true),
            "divide_by_ten_1dp_if_required" => (1, 10, 0, Some(1), false),
            "deciseconds_to_seconds" => (1, 10, 0, None, false),
            "divide_by_two_0dp" => (1, 2, 0, Some(0), false),
            "divide_by_fifteen_0dp" => (1, 15, 0, Some(0), false),
            "divide_by_twenty_then_double_0dp" => (2, 20, 0, Some(0), false),
            "divide_by_three" => (1, 3, 0, None, false),
            "divide_by_four" => (1, 4, 0, None, false),
            "divide_by_five" => (1, 5, 0, None, false),
            "divide_by_fifty" => (1, 50, 0, None, false),
            "per_minute_to_per_second" => (1, 60, 0, Some(1), false),
            "per_minute_to_per_second_0dp" => (1, 60, 0, Some(0), false),
            "per_minute_to_per_second_1dp" => (1, 60, 0, Some(1), true),
            "per_minute_to_per_second_2dp" => (1, 60, 0, Some(2), true),
            "per_minute_to_per_second_2dp_if_required" => (1, 60, 0, Some(2), false),
            "milliseconds_to_seconds" => (1, 1000, 0, None, false),
            "milliseconds_to_seconds_0dp" => (1, 1000, 0, Some(0), false),
            "milliseconds_to_seconds_1dp" => (1, 1000, 0, Some(1), true),
            "milliseconds_to_seconds_2dp" => (1, 1000, 0, Some(2), true),
            "milliseconds_to_seconds_2dp_if_required" => (1, 1000, 0, Some(2), false),
            _ => return Err(format!("unsupported CSD handler {name:?}")),
        };
        let f = &mut self.format;
        f.multiplier = f
            .multiplier
            .checked_mul(multiplier)
            .ok_or("CSD integer transform overflow")?;
        f.offset = f
            .offset
            .checked_mul(multiplier)
            .and_then(|v| {
                i64::try_from(f.divisor)
                    .ok()
                    .and_then(|d| d.checked_mul(divisor))
                    .and_then(|d| d.checked_mul(addend))
                    .and_then(|a| v.checked_add(a))
            })
            .ok_or("CSD offset overflow")?;
        f.divisor = f
            .divisor
            .checked_mul(divisor as u64)
            .ok_or("CSD divisor overflow")?;
        f.precision = precision;
        f.fixed = fixed;
        self.numerator()?;
        Ok(())
    }
    fn numerator(&self) -> Result<i64, String> {
        self.value
            .checked_mul(self.format.multiplier)
            .and_then(|v| v.checked_add(self.format.offset))
            .ok_or_else(|| "CSD integer transform overflow".into())
    }
    pub fn display(&self, positive_sign: bool) -> Result<String, String> {
        self.format
            .display(self.value, positive_sign)
            .ok_or_else(|| "invalid or overflowing CSD display format".into())
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
        if name != "canonical_stat" {
            value.transform(name)?;
        }
    }
    Ok(())
}
