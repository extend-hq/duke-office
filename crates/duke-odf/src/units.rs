//! Lengths, percentages and colors as OpenDocument attributes spell them.

/// A length in English Metric Units (914,400 per inch), the unit both Office
/// models already use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Length(pub i64);

impl Length {
    pub const EMU_PER_INCH: f64 = 914_400.0;
    pub const EMU_PER_POINT: f64 = 12_700.0;

    pub fn emu(self) -> i64 {
        self.0
    }

    /// Twentieths of a point, rounded.
    pub fn twips(self) -> i64 {
        (self.0 as f64 / 635.0).round() as i64
    }

    pub fn points(self) -> f64 {
        self.0 as f64 / Self::EMU_PER_POINT
    }

    pub fn from_points(points: f64) -> Self {
        Self((points * Self::EMU_PER_POINT).round() as i64)
    }
}

fn split_number(value: &str) -> Option<(f64, &str)> {
    let value = value.trim();
    let bytes = value.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'-' | b'+')));
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
    {
        end += 1;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut cursor = end + 1;
        if matches!(bytes.get(cursor), Some(b'-' | b'+')) {
            cursor += 1;
        }
        if bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                cursor += 1;
            }
            end = cursor;
        }
    }
    let number = value[..end].parse::<f64>().ok()?;
    number.is_finite().then_some((number, value[end..].trim()))
}

/// Parses `2.54cm`, `1in`, `12pt`, `10mm`, `1pc` or `96px`. A bare number or
/// a percentage is not a length.
pub fn parse_length(value: &str) -> Option<Length> {
    let (number, unit) = split_number(value)?;
    let emu_per_unit = match unit {
        "cm" => 360_000.0,
        "mm" => 36_000.0,
        "in" | "inch" => Length::EMU_PER_INCH,
        "pt" => Length::EMU_PER_POINT,
        "pc" => 152_400.0,
        "px" => 9_525.0,
        _ => return None,
    };
    let emu = number * emu_per_unit;
    (emu.abs() < 9.0e15).then(|| Length(emu.round() as i64))
}

/// Parses `80%` into `80.0`.
pub fn parse_percent(value: &str) -> Option<f64> {
    let (number, unit) = split_number(value)?;
    (unit == "%").then_some(number)
}

/// Parses a unitless number such as a view box coordinate or `office:value`.
pub fn parse_number(value: &str) -> Option<f64> {
    let (number, unit) = split_number(value)?;
    unit.is_empty().then_some(number)
}

/// Parses `#rrggbb` or `#rgb` into upper-case `RRGGBB`. `transparent` and
/// anything else is `None`.
pub fn parse_color(value: &str) -> Option<String> {
    let hex = value.trim().strip_prefix('#')?;
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        6 => Some(hex.to_ascii_uppercase()),
        3 => Some(
            hex.chars()
                .flat_map(|ch| [ch.to_ascii_uppercase(), ch.to_ascii_uppercase()])
                .collect(),
        ),
        _ => None,
    }
}

pub fn parse_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_every_length_unit() {
        assert_eq!(parse_length("2.54cm"), Some(Length(914_400)));
        assert_eq!(parse_length("1in"), Some(Length(914_400)));
        assert_eq!(parse_length("25.4mm"), Some(Length(914_400)));
        assert_eq!(parse_length("72pt"), Some(Length(914_400)));
        assert_eq!(parse_length("6pc"), Some(Length(914_400)));
        assert_eq!(parse_length("-0.5in").map(Length::twips), Some(-720));
        assert_eq!(parse_length("1.5e1pt"), Some(Length(190_500)));
        assert_eq!(parse_length("12"), None);
        assert_eq!(parse_length("50%"), None);
        assert_eq!(parse_length("wide"), None);
    }

    #[test]
    fn parses_percentages_numbers_and_colors() {
        assert_eq!(parse_percent("115%"), Some(115.0));
        assert_eq!(parse_percent("12pt"), None);
        assert_eq!(parse_number("-21600"), Some(-21600.0));
        assert_eq!(parse_color("#1a2B3c").as_deref(), Some("1A2B3C"));
        assert_eq!(parse_color("#abc").as_deref(), Some("AABBCC"));
        assert_eq!(parse_color("transparent"), None);
        assert_eq!(parse_color("#12345"), None);
    }
}
