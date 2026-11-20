use std::fmt::Write;

use crate::model::TextStyle;

fn language(mark: Option<&TextStyle>) -> Option<&str> {
    let mark = mark?;
    if mark.right_to_left == Some(true) || mark.complex_script == Some(true) {
        return mark.language_bidi.as_deref();
    }
    mark.language.as_deref()
}

fn primary_language(mark: Option<&TextStyle>) -> String {
    let tag = language(mark).unwrap_or("en");
    let primary = tag
        .split(['-', '_'])
        .next()
        .unwrap_or(tag)
        .to_ascii_lowercase();
    match primary.as_str() {
        // Word uses English ordinal words for these proofing languages.
        "ar" | "fa" | "gl" | "he" | "hi" | "id" | "is" | "ja" | "ko" | "ms" | "th" | "vi"
        | "zh" => "en".into(),
        _ => primary,
    }
}

pub(super) fn write_numeric(out: &mut String, n: u64, mark: Option<&TextStyle>) -> Option<()> {
    let suffix = match primary_language(mark).as_str() {
        "en" => match (n % 100, n % 10) {
            (11..=13, _) => "th",
            (_, 1) => "st",
            (_, 2) => "nd",
            (_, 3) => "rd",
            _ => "th",
        },
        "da" => match (n, n % 100) {
            (0, _) => "te",
            (_, 0) => "ende",
            (_, 1) => "ste",
            (_, 2) => "nden",
            (_, 3) => "dje",
            (_, 4) => "rde",
            (_, 5 | 6 | 11 | 12 | 30) => "te",
            _ => "nde",
        },
        "fr" if n == 0 => "",
        "fr" if n == 1 => "er",
        "fr" => "e",
        "de" => ".",
        "es" | "pt" => "º",
        "it" => "°",
        "ru" => "-й",
        _ => return None,
    };
    write!(out, "{n}{suffix}").ok()
}

pub(super) fn write_spelled(out: &mut String, n: u64, mark: Option<&TextStyle>) -> Option<()> {
    // Above this limit Word suppresses the counter, retaining the template's literals.
    if n > 999_999 {
        return Some(());
    }
    let lang = primary_language(mark);
    let words = if n == 0 {
        let zero = match lang.as_str() {
            "bg" => "нулевят",
            "ca" | "fi" => return Some(()),
            "cs" | "sk" => "nultý",
            "da" | "nb" | "nn" | "no" => "nulte",
            "de" => "nullte",
            "el" => "μηδενικό",
            "en" => "zeroth",
            "es" => "cero",
            "et" => "nullis",
            "eu" => "zerogarren",
            "fr" => "zéro",
            "hr" => "nulti",
            "hu" => "nulladik",
            "it" | "pt" | "ro" => "zero",
            "lt" => "nulinis",
            "lv" => "nultais",
            "nl" => "nulde",
            "pl" => "zerowy",
            "ru" => "нулевой",
            "sl" => "ničti",
            "sv" => "nollte",
            "tr" => "sıfırıncı",
            "uk" => "нульовий",
            _ => return None,
        };
        zero.to_owned()
    } else {
        match lang.as_str() {
            "en" => n2words::en_us::to_ordinal(n).ok()?,
            "fr" => n2words::fr_fr::to_ordinal(n).ok()?,
            "de" => {
                let word = n2words::de::to_ordinal(n)
                    .ok()?
                    .replace("siebte", "siebente");
                word.strip_prefix("einhundert")
                    .map(|s| format!("hundert{s}"))
                    .unwrap_or(word)
            }
            "es" => spanish(n)?,
            "eu" => basque(n),
            "it" => n2words::it::to_ordinal(n)
                .ok()?
                .replace("centoun", "centun"),
            "pt" => portuguese(n)?,
            "nl" => dutch(n, true)?,
            "da" => danish(n, true)?,
            "el" => greek(n)?,
            "tr" => turkish(n)?,
            "uk" => n2words::uk::to_ordinal(n).ok()?.replace('i', "і"),
            "sv" => rule_ordinal("sv", n)?
                .replace("förste", "första")
                .replace("andre", "andra")
                .replace("tjugote", "tjugonde")
                .replace(' ', ""),
            "ru" => rule_ordinal("ru", n)?.replace('ё', "е"),
            "bg" => {
                let word = rule_ordinal("bg", n)?;
                format!(
                    "{}ят",
                    word.strip_suffix("ен")
                        .map(|s| format!("{s}ни"))
                        .unwrap_or(word)
                )
            }
            "ro" if n == 1 => "întâi".to_owned(),
            "nb" | "nn" | "no" => rule_ordinal("no", n)
                .or_else(|| n2words::nb::to_ordinal(n).ok())?
                .replace(' ', ""),
            "sk" => slovak(n)?,
            "et" => estonian(n, true)?,
            "sl" => {
                let word = rule_ordinal("sl", n)?;
                match word.strip_suffix("sedemi") {
                    Some(stem) => format!("{stem}sedmi"),
                    None => word,
                }
            }
            "fi" => {
                let word = rule_ordinal("fi", n)?;
                match (n % 100, n % 10) {
                    (20.., 1) => word
                        .strip_suffix("ensimmäinen")
                        .map(|stem| format!("{stem}yhdes"))
                        .unwrap_or(word),
                    (20.., 2) => word
                        .strip_suffix("toinen")
                        .map(|stem| format!("{stem}kahdes"))
                        .unwrap_or(word),
                    _ => word,
                }
            }
            "ca" | "cs" | "hu" | "ro" => rule_ordinal(&lang, n)?,
            "hr" => n2words::hr::to_ordinal(n).ok()?,
            "lt" => n2words::lt::to_ordinal(n).ok()?,
            "lv" => n2words::lv::to_ordinal(n).ok()?,
            "pl" => n2words::pl::to_ordinal(n).ok()?,
            "sr" => {
                let latin = language(mark)?
                    .split(['-', '_'])
                    .any(|s| s.eq_ignore_ascii_case("Latn"));
                if latin {
                    n2words::sr_latn_rs::to_ordinal(n)
                } else {
                    n2words::sr_cyrl_rs::to_ordinal(n)
                }
                .ok()?
            }
            _ => return None,
        }
    };
    let mut chars = words.chars();
    let initial = chars.next()?;
    if lang == "tr" && initial == 'i' {
        out.push('İ');
    } else {
        out.extend(initial.to_uppercase());
    }
    out.push_str(chars.as_str());
    Some(())
}

fn rule_ordinal(lang: &str, n: u64) -> Option<String> {
    let word = numbertext::ordinal(lang, &n.to_string()).ok()?;
    (word.chars().any(char::is_alphabetic) && !word.chars().any(char::is_numeric)).then_some(word)
}

fn spanish(n: u64) -> Option<String> {
    match n {
        _ if (21..=29).contains(&(n % 100)) => {
            let units = n % 10;
            Some(format!(
                "{} {}",
                n2words::es::to_ordinal(n - units).ok()?,
                n2words::es::to_ordinal(units).ok()?
            ))
        }
        _ => n2words::es::to_ordinal(n).ok(),
    }
}

fn turkish(n: u64) -> Option<String> {
    let mut stem = n2words::tr::to_cardinal(n).ok()?.replace(' ', "");
    let vowel = stem.chars().rev().find(|c| "aeıioöuü".contains(*c))?;
    let suffix_vowel = match vowel {
        'a' | 'ı' => 'ı',
        'e' | 'i' => 'i',
        'o' | 'u' => 'u',
        _ => 'ü',
    };
    if stem.ends_with("dört") {
        stem.pop();
        stem.push('d');
    }
    if !stem.ends_with(vowel) {
        stem.push(suffix_vowel);
    }
    stem.push_str("nc");
    stem.push(suffix_vowel);
    Some(stem)
}

fn greek(n: u64) -> Option<String> {
    let neuter = |value| {
        let words = n2words::el::to_ordinal(value).ok()?;
        Some(
            words
                .split_whitespace()
                .map(|w| w.strip_suffix('ς').unwrap_or(w))
                .collect::<Vec<_>>()
                .join(" "),
        )
    };
    let mut parts = Vec::new();
    let thousands = n / 1000;
    const THOUSAND_PREFIX: [&str; 20] = [
        "",
        "δισ",
        "τρισ",
        "τετρακισ",
        "πεντακισ",
        "εξακισ",
        "επτακισ",
        "οκτακισ",
        "εννιακισ",
        "δεκακισ",
        "ενδεκακισ",
        "δωδεκακισ",
        "δεκατριακισ",
        "δεκατετρακισ",
        "δεκαπεντακισ",
        "δεκαεξακισ",
        "δεκαεπτακισ",
        "δεκαοκτακισ",
        "δεκαεννιακισ",
        "εικοσακισ",
    ];
    if thousands > 0 {
        if let Some(prefix) = THOUSAND_PREFIX.get((thousands - 1) as usize) {
            parts.push(format!("{prefix}χιλιοστό"));
        } else {
            parts.push(greek(thousands)?);
            parts.push(neuter(1000)?);
        }
    }
    let hundreds = (n % 1000) / 100 * 100;
    if hundreds > 0 {
        parts.push(neuter(hundreds)?);
    }
    if n % 100 > 0 {
        parts.push(neuter(n % 100)?);
    }
    Some(parts.join(" "))
}

fn portuguese(n: u64) -> Option<String> {
    let below_thousand = |v| {
        n2words::pt_pt::to_ordinal(v).ok().map(|s| {
            s.replace("septuagésimo", "setuagésimo")
                .replace("tricentésimo", "trecentésimo")
        })
    };
    if n < 1000 {
        return below_thousand(n);
    }
    let thousands = n / 1000;
    let mut label = if thousands == 1 {
        "milésimo".to_owned()
    } else {
        format!("{} milésimos", n2words::pt_pt::to_cardinal(thousands).ok()?)
    };
    if n % 1000 != 0 {
        label.push_str(if n % 100 == 0 { " e " } else { " " });
        label.push_str(&below_thousand(n % 1000)?);
    }
    Some(label)
}

fn dutch(n: u64, ordinal: bool) -> Option<String> {
    if n < 100 {
        let word = if ordinal {
            n2words::nl::to_ordinal(n)
        } else {
            n2words::nl::to_cardinal(n)
        };
        return Some(word.ok()?.replace("een", "één"));
    }
    let (scale, unit) = if n >= 1000 {
        (1000, "duizend")
    } else {
        (100, "honderd")
    };
    let (quotient, rest) = (n / scale, n % scale);
    let mut word = if ordinal && n == 1000 {
        String::new()
    } else {
        dutch(quotient, false)?
    };
    if scale == 1000 && quotient % 100 >= 20 {
        word.push(' ');
    }
    word.push_str(unit);
    if rest > 0 {
        word.push_str(&dutch(rest, ordinal)?);
    } else if ordinal {
        word.push_str("ste");
    }
    Some(word)
}

fn danish(n: u64, ordinal: bool) -> Option<String> {
    const ORDINAL_TENS: [&str; 8] = [
        "tyvende",
        "tredivte",
        "fyrretyvende",
        "halvtredsindstyvende",
        "tresindstyvende",
        "halvfjerdsindstyvende",
        "firsindstyvende",
        "halvfemsindstyvende",
    ];
    if n < 100 {
        if !ordinal {
            if n == 1 {
                return Some("en".into());
            }
            return n2words::da::to_cardinal(n).ok().map(|v| v.replace(' ', ""));
        }
        if n < 20 {
            return n2words::da::to_ordinal(n).ok();
        }
        let tens = ORDINAL_TENS[(n / 10 - 2) as usize];
        return if n % 10 == 0 {
            Some(tens.to_owned())
        } else {
            Some(format!("{}og{tens}", danish(n % 10, false)?))
        };
    }
    if n == 1000 && ordinal {
        return Some("tusinde".into());
    }
    let (scale, unit) = if n >= 1000 {
        (1000, " tusinde")
    } else {
        (100, " hundrede")
    };
    let (quotient, rest) = (n / scale, n % scale);
    let mut word = if ordinal && quotient == 1 {
        "et".to_owned()
    } else {
        danish(quotient, false)?
    };
    word.push_str(unit);
    if rest != 0 {
        let join = match (ordinal, scale, rest) {
            (true, 100, _) => " og ",
            (true, 1000, 1..=99) => " og",
            _ => " ",
        };
        word.push_str(join);
        word.push_str(&danish(rest, ordinal)?);
    }
    Some(word)
}

fn basque(n: u64) -> String {
    const SMALL: [&str; 20] = [
        "zero",
        "bat",
        "bi",
        "hiru",
        "lau",
        "bost",
        "sei",
        "zazpi",
        "zortzi",
        "bederatzi",
        "hamar",
        "hamaika",
        "hamabi",
        "hamairu",
        "hamalau",
        "hamabost",
        "hamasei",
        "hamazazpi",
        "hemezortzi",
        "hemeretzi",
    ];
    const SCORES: [&str; 4] = ["hogei", "berrogei", "hirurogei", "laurogei"];
    const HUNDREDS: [&str; 9] = [
        "ehun",
        "berrehun",
        "hirurehun",
        "laurehun",
        "bostehun",
        "seiehun",
        "zazpiehun",
        "zortziehun",
        "bederatziehun",
    ];
    fn groups(n: u64) -> Vec<String> {
        let mut words = Vec::new();
        if n >= 100 {
            words.push(HUNDREDS[(n / 100 - 1) as usize].to_owned());
        }
        let low = n % 100;
        if low >= 20 {
            let base = SCORES[(low / 20 - 1) as usize];
            words.push(if low % 20 == 0 {
                base.to_owned()
            } else {
                format!("{base}ta {}", SMALL[(low % 20) as usize])
            });
        } else if low > 0 {
            words.push(SMALL[low as usize].to_owned());
        }
        words
    }
    if n == 1 {
        return "lehenengo".into();
    }
    let mut words = Vec::new();
    if n >= 1000 {
        let count = n / 1000;
        words.push(if count == 1 {
            "mila".to_owned()
        } else {
            format!("{} mila", groups(count).join(" "))
        });
    }
    words.extend(groups(n % 1000));
    let last = words.pop().unwrap_or_default();
    let stem = if words.is_empty() {
        last
    } else {
        format!("{} eta {last}", words.join(" "))
    };
    format!("{stem}garren")
}

fn slovak(n: u64) -> Option<String> {
    let (scale, name) = match n {
        0..=99 => return Some(rule_ordinal("sk", n)?.replace(' ', "")),
        100..=999 => (100, "sto"),
        _ => (1000, "tisíc"),
    };
    let (quotient, rest) = (n / scale, n % scale);
    if rest == 0 {
        if let Some(word) = rule_ordinal("sk", n) {
            return Some(word.replace(' ', ""));
        }
    }
    let mut word = if scale == 100 {
        numbertext::cardinal("sk", &(quotient * scale).to_string())
            .ok()?
            .replace(' ', "")
    } else if quotient == 1 {
        name.to_owned()
    } else {
        format!("{}{name}", slovak(quotient)?)
    };
    if rest > 0 {
        word.push_str(&slovak(rest)?);
    } else {
        word.push('i');
    }
    Some(word)
}

fn estonian(n: u64, ordinal: bool) -> Option<String> {
    const GENITIVE: [&str; 10] = [
        "", "ühe", "kahe", "kolme", "nelja", "viie", "kuue", "seitsme", "kaheksa", "üheksa",
    ];
    if n < 20 {
        return if ordinal {
            rule_ordinal("et", n)
        } else {
            Some(match n {
                0..=9 => GENITIVE[n as usize].into(),
                10 => "kümne".into(),
                _ => format!("{}teistkümne", GENITIVE[(n - 10) as usize]),
            })
        };
    }
    let scale = if n >= 1000 {
        1000
    } else if n >= 100 {
        100
    } else {
        10
    };
    let (quotient, rest) = (n / scale, n % scale);
    let exact = ordinal && rest == 0;
    let ending = match (scale, exact) {
        (10, true) => "kümnes",
        (10, false) => "kümne",
        (100, true) => "sajas",
        (100, false) => "saja",
        (_, true) => "tuhandes",
        (_, false) => "tuhande",
    };
    let mut word = if quotient == 1 {
        String::new()
    } else {
        estonian(quotient, false)?
    };
    if scale == 1000 && !word.is_empty() {
        word.push(' ');
    }
    word.push_str(ending);
    if rest != 0 {
        word.push(' ');
        word.push_str(&estonian(rest, ordinal)?);
    }
    Some(word)
}
