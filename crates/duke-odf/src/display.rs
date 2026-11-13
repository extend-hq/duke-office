pub(crate) fn cell_value(kind: &str, raw: &str, code: Option<&str>) -> String {
    if kind == "date" {
        if let (Some(code), Some(date)) = (code, raw.get(..10)) {
            let parts: Vec<_> = date.split('-').collect();
            if parts.len() == 3
                && parts[0].len() == 4
                && parts[1].len() == 2
                && parts[2].len() == 2
                && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
            {
                let year: u32 = parts[0].parse().unwrap_or(0);
                let month: usize = parts[1].parse().unwrap_or(0);
                let day: u32 = parts[2].parse().unwrap_or(0);
                let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
                let days = [
                    0,
                    31,
                    if leap { 29 } else { 28 },
                    31,
                    30,
                    31,
                    30,
                    31,
                    31,
                    30,
                    31,
                    30,
                    31,
                ];
                if month == 0 || month > 12 || day == 0 || day > days[month] {
                    return raw.to_owned();
                }
                let mut out = String::new();
                let mut chars = code.chars().peekable();
                while let Some(ch) = chars.next() {
                    if ch == '\\' {
                        if let Some(ch) = chars.next() {
                            out.push(ch);
                        }
                        continue;
                    }
                    let mut count = 1;
                    while matches!(ch, 'y' | 'm' | 'd') && chars.peek() == Some(&ch) {
                        chars.next();
                        count += 1;
                    }
                    match ch {
                        'y' => out.push_str(if count == 2 { &parts[0][2..] } else { parts[0] }),
                        'm' | 'd' if count <= 2 => {
                            let part = parts[if ch == 'm' { 1 } else { 2 }];
                            out.push_str(if count == 1 {
                                part.trim_start_matches('0')
                            } else {
                                part
                            });
                        }
                        '/' | '-' | '.' | ' ' => out.push(ch),
                        _ => return raw.to_owned(),
                    }
                }
                return out;
            }
        }
        return raw.to_owned();
    }
    if kind == "boolean" {
        return match raw {
            "true" | "1" => "TRUE",
            "false" | "0" => "FALSE",
            _ => raw,
        }
        .to_owned();
    }
    raw.to_owned()
}
