#[derive(Clone, Copy)]
struct Token {
    ch: char,
    pattern: bool,
}

fn sections(code: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let (mut quoted, mut escaped, mut bracket, mut start) = (false, false, false, 0);
    for (i, ch) in code.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' | '_' | '*' if !quoted => escaped = true,
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            ';' if !quoted && !bracket => {
                result.push(&code[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&code[start..]);
    result
}

fn condition(section: &str) -> Option<(&'static str, f64)> {
    for part in section.split('[').skip(1) {
        let part = part.split_once(']')?.0;
        for op in [">=", "<=", "<>", ">", "<", "="] {
            if let Some(number) = part.strip_prefix(op).and_then(|n| n.parse::<f64>().ok()) {
                return Some((op, number));
            }
        }
    }
    None
}

fn matches_condition((op, bound): (&str, f64), value: f64) -> bool {
    match op {
        ">=" => value >= bound,
        "<=" => value <= bound,
        "<>" => value != bound,
        ">" => value > bound,
        "<" => value < bound,
        _ => value == bound,
    }
}

fn negative_only(sections: &[&str], selected: usize) -> bool {
    if let Some((op, bound)) = condition(sections[selected]) {
        return (op == "<" && bound <= 0.) || (matches!(op, "<=" | "=") && bound < 0.);
    }
    sections[..selected].iter().any(|s| {
        condition(s).is_some_and(|(op, bound)| matches!(op, ">" | ">=" | "<>") && bound <= 0.)
    })
}

fn tokens(section: &str) -> Option<Vec<Token>> {
    let mut out = Vec::new();
    let mut chars = section.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                for ch in chars.by_ref().take_while(|ch| *ch != '"') {
                    out.push(Token { ch, pattern: false });
                }
            }
            '\\' => out.push(Token {
                ch: chars.next()?,
                pattern: false,
            }),
            '_' | '*' => {
                chars.next()?;
            }
            '[' => {
                let tag: String = chars.by_ref().take_while(|ch| *ch != ']').collect();
                if let Some(currency) = tag.strip_prefix('$') {
                    for ch in currency.split('-').next().unwrap_or("").chars() {
                        out.push(Token { ch, pattern: false });
                    }
                }
            }
            _ => out.push(Token { ch, pattern: true }),
        }
    }
    Some(out)
}

fn placeholder(token: &Token) -> bool {
    token.pattern && matches!(token.ch, '0' | '#' | '?')
}

fn literal(tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.ch).collect()
}

pub(super) fn format(value: f64, code: Option<&str>) -> Option<String> {
    let negative = value.is_sign_negative();
    let code = code.filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("General"))?;
    let sections = sections(code);
    let conditional = sections.iter().any(|s| condition(s).is_some());
    let selected = if conditional {
        sections
            .iter()
            .position(|s| condition(s).is_none_or(|c| matches_condition(c, value)))?
    } else if value == 0. && sections.len() > 2 {
        2
    } else {
        usize::from(value < 0. && sections.len() > 1)
    };
    let section = sections[selected];
    if section.is_empty() {
        return Some(String::new());
    }
    let tokens = tokens(section)?;
    let Some(first) = tokens.iter().position(placeholder) else {
        return (!tokens
            .iter()
            .any(|t| t.pattern && t.ch.is_ascii_alphabetic()))
        .then(|| literal(&tokens));
    };
    let last = tokens.iter().rposition(placeholder)?;
    let mut end = last + 1;
    while tokens.get(end).is_some_and(|t| t.pattern && t.ch == ',') {
        end += 1;
    }
    let numeric: String = tokens[first..=last].iter().map(|t| t.ch).collect();
    if tokens[first..=last].iter().any(|t| !t.pattern)
        || numeric
            .chars()
            .any(|c| !matches!(c, '0' | '#' | '?' | '.' | ',' | 'E' | 'e' | '+' | '-'))
    {
        return None;
    }
    let percent = tokens.iter().filter(|t| t.pattern && t.ch == '%').count();
    let trailing_commas = end - last - 1;
    let value = value.abs() * 100_f64.powi(percent.min(10) as i32)
        / 1000_f64.powi(trailing_commas.min(100) as i32);
    let (mantissa, exponent) = numeric
        .find(['E', 'e'])
        .map_or((numeric.as_str(), None), |i| {
            (&numeric[..i], Some(&numeric[i + 1..]))
        });
    let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let precision = fraction
        .chars()
        .filter(|c| matches!(c, '0' | '#' | '?'))
        .count()
        .min(12);
    let mut label = if let Some(exponent) = exponent {
        let scientific = format!("{value:.precision$E}");
        let (mantissa, power) = scientific.split_once('E')?;
        let power = power.parse::<i32>().ok()?;
        let width = exponent.chars().filter(|c| *c == '0').count().max(1);
        let sign = if power < 0 {
            "-"
        } else if exponent.starts_with('+') {
            "+"
        } else {
            ""
        };
        format!("{mantissa}E{sign}{:0width$}", power.unsigned_abs())
    } else {
        // Decimal half values use the spreadsheet rounding rule.
        let midpoint = value
            .to_string()
            .split_once('.')
            .is_some_and(|(_, digits)| digits.len() == precision + 1 && digits.ends_with('5'));
        let rounded = if midpoint {
            let scale = 10_f64.powi(precision as i32);
            ((value * scale).floor() + 1.) / scale
        } else {
            value
        };
        let label = format!("{rounded:.precision$}");
        let (whole, decimals) = label.split_once('.').unwrap_or((&label, ""));
        let required = integer.chars().filter(|c| *c == '0').count();
        let mut whole = if required == 0 && whole == "0" {
            String::new()
        } else {
            whole.to_owned()
        };
        while whole.len() < required {
            whole.insert(0, '0');
        }
        if integer.contains(',') {
            let mut i = whole.len();
            while i > 3 {
                i -= 3;
                whole.insert(i, ',');
            }
        }
        let mut decimals = decimals.to_owned();
        for token in fraction.chars().rev() {
            if token == '#' && decimals.ends_with('0') {
                decimals.pop();
            } else {
                break;
            }
        }
        if !decimals.is_empty() {
            whole.push('.');
            whole.push_str(&decimals);
        }
        whole
    };
    label.insert_str(0, &literal(&tokens[..first]));
    label.push_str(&literal(&tokens[end..]));
    let automatic_sign = if conditional {
        !negative_only(&sections, selected)
    } else {
        sections.len() == 1
    };
    if automatic_sign && negative {
        label.insert(0, '-');
    }
    Some(label)
}
