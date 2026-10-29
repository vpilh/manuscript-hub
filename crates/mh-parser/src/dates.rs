use mh_core::CalendarDate;

/// An absolute date found in text, with the exact source substring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundDate {
    pub date: CalendarDate,
    pub original: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Number { value: u32, digits: usize },
    Word(String),
    Punct(char),
}

#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c.is_ascii_digit() {
            let mut end = start + 1;
            while let Some(&(i, d)) = chars.peek() {
                if !d.is_ascii_digit() {
                    break;
                }
                end = i + 1;
                chars.next();
            }
            let digits = &text[start..end];
            // Anything longer than a year is not part of a date.
            let kind = match digits.parse::<u32>() {
                Ok(value) if digits.len() <= 4 => Kind::Number {
                    value,
                    digits: digits.len(),
                },
                _ => Kind::Punct('#'),
            };
            tokens.push(Token { kind, start, end });
        } else if c.is_alphabetic() {
            let mut end = start + c.len_utf8();
            while let Some(&(i, a)) = chars.peek() {
                if !a.is_alphabetic() {
                    break;
                }
                end = i + a.len_utf8();
                chars.next();
            }
            let word = text[start..end].to_lowercase();
            tokens.push(Token {
                kind: Kind::Word(word),
                start,
                end,
            });
        } else {
            tokens.push(Token {
                kind: Kind::Punct(c),
                start,
                end: start + c.len_utf8(),
            });
        }
    }
    tokens
}

/// English and German month names and common abbreviations.
fn month_number(word: &str) -> Option<u8> {
    let month = match word {
        "january" | "januar" | "jänner" | "jan" => 1,
        "february" | "februar" | "feb" => 2,
        "march" | "märz" | "maerz" | "mar" | "mär" => 3,
        "april" | "apr" => 4,
        "may" | "mai" => 5,
        "june" | "juni" | "jun" => 6,
        "july" | "juli" | "jul" => 7,
        "august" | "aug" => 8,
        "september" | "sept" | "sep" => 9,
        "october" | "oktober" | "oct" | "okt" => 10,
        "november" | "nov" => 11,
        "december" | "dezember" | "dec" | "dez" => 12,
        _ => return None,
    };
    Some(month)
}

struct Cursor<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl Cursor<'_> {
    fn number(&mut self, max_digits: usize) -> Option<u32> {
        match self.tokens.get(self.pos)?.kind {
            Kind::Number { value, digits } if digits <= max_digits => {
                self.pos += 1;
                Some(value)
            }
            _ => None,
        }
    }

    fn year(&mut self) -> Option<i32> {
        match self.tokens.get(self.pos)?.kind {
            Kind::Number { value, digits: 4 } => {
                self.pos += 1;
                i32::try_from(value).ok()
            }
            _ => None,
        }
    }

    fn punct(&mut self, expected: char) -> bool {
        let matches = matches!(
            self.tokens.get(self.pos),
            Some(Token { kind: Kind::Punct(c), .. }) if *c == expected
        );
        if matches {
            self.pos += 1;
        }
        matches
    }

    fn month(&mut self) -> Option<u8> {
        let Some(Token {
            kind: Kind::Word(word),
            ..
        }) = self.tokens.get(self.pos)
        else {
            return None;
        };
        let month = month_number(word)?;
        self.pos += 1;
        self.punct('.');
        Some(month)
    }

    fn ordinal_suffix(&mut self) {
        if let Some(Token {
            kind: Kind::Word(word),
            ..
        }) = self.tokens.get(self.pos)
        {
            if matches!(word.as_str(), "st" | "nd" | "rd" | "th") {
                self.pos += 1;
            }
        }
    }
}

/// `2026-11-12`
fn iso(c: &mut Cursor<'_>) -> Option<(i32, u8, u8)> {
    let year = c.year()?;
    c.punct('-').then_some(())?;
    let month = c.number(2)?;
    c.punct('-').then_some(())?;
    let day = c.number(2)?;
    Some((year, month as u8, day as u8))
}

/// `12.11.2026` (German numeric). Slash dates are ambiguous between locales and are skipped.
fn dotted(c: &mut Cursor<'_>) -> Option<(i32, u8, u8)> {
    let day = c.number(2)?;
    c.punct('.').then_some(())?;
    let month = c.number(2)?;
    c.punct('.').then_some(())?;
    let year = c.year()?;
    Some((year, month as u8, day as u8))
}

/// `12 November 2026`, `12. November 2026`, `12th Nov. 2026`
fn day_month_year(c: &mut Cursor<'_>) -> Option<(i32, u8, u8)> {
    let day = c.number(2)?;
    c.punct('.');
    c.ordinal_suffix();
    let month = c.month()?;
    c.punct(',');
    let year = c.year()?;
    Some((year, month, day as u8))
}

/// `November 12, 2026`, `Nov 12th 2026`
fn month_day_year(c: &mut Cursor<'_>) -> Option<(i32, u8, u8)> {
    let month = c.month()?;
    let day = c.number(2)?;
    c.ordinal_suffix();
    c.punct(',');
    let year = c.year()?;
    Some((year, month, day as u8))
}

/// All absolute dates in `text`, in order of appearance.
pub fn find_dates(text: &str) -> Vec<FoundDate> {
    let tokens = tokenize(text);
    let mut found = Vec::new();
    let mut pos = 0;
    while pos < tokens.len() {
        let mut matched = None;
        for pattern in [iso, dotted, day_month_year, month_day_year] {
            let mut cursor = Cursor {
                tokens: &tokens,
                pos,
            };
            if let Some((y, m, d)) = pattern(&mut cursor) {
                if let Ok(date) = CalendarDate::new(y, m, d) {
                    matched = Some((date, cursor.pos));
                    break;
                }
            }
        }
        match matched {
            Some((date, next)) => {
                let original = text[tokens[pos].start..tokens[next - 1].end].to_string();
                found.push(FoundDate { date, original });
                pos = next;
            }
            None => pos += 1,
        }
    }
    found
}

/// Relative deadlines such as "within 30 days" or "innerhalb von 4 Wochen", in days.
pub fn find_relative_deadline_days(text: &str) -> Option<u32> {
    let tokens = tokenize(text);
    let word = |i: usize| match tokens.get(i).map(|t| &t.kind) {
        Some(Kind::Word(w)) => Some(w.as_str()),
        _ => None,
    };
    for i in 0..tokens.len() {
        let mut next = match word(i) {
            Some("within" | "binnen") => i + 1,
            Some("innerhalb") if word(i + 1) == Some("von") => i + 2,
            Some("innerhalb") => i + 1,
            _ => continue,
        };
        let Some(Kind::Number { value, .. }) = tokens.get(next).map(|t| &t.kind) else {
            continue;
        };
        next += 1;
        let days = match word(next) {
            Some("day" | "days" | "tag" | "tage" | "tagen") => *value,
            Some("week" | "weeks" | "woche" | "wochen") => value * 7,
            _ => continue,
        };
        return Some(days);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dates(text: &str) -> Vec<String> {
        find_dates(text)
            .into_iter()
            .map(|d| d.date.to_string())
            .collect()
    }

    #[test]
    fn parses_english_and_german_formats() {
        assert_eq!(dates("due 2026-11-12."), ["2026-11-12"]);
        assert_eq!(dates("bis zum 12.11.2026 einreichen"), ["2026-11-12"]);
        assert_eq!(dates("by 12 November 2026"), ["2026-11-12"]);
        assert_eq!(dates("bis zum 3. März 2027"), ["2027-03-03"]);
        assert_eq!(dates("by November 12, 2026 at the latest"), ["2026-11-12"]);
        assert_eq!(
            dates("on 1st Dec. 2026 and Okt 5 2026"),
            ["2026-12-01", "2026-10-05"]
        );
    }

    #[test]
    fn keeps_the_source_substring() {
        let found = find_dates("Frist: 12. Dezember 2026.");
        assert_eq!(found[0].original, "12. Dezember 2026");
    }

    #[test]
    fn rejects_invalid_and_ambiguous_dates() {
        assert!(dates("31.02.2026").is_empty());
        assert!(dates("11/12/2026").is_empty());
        assert!(dates("manuscript 123456789").is_empty());
    }

    #[test]
    fn finds_relative_deadlines() {
        assert_eq!(
            find_relative_deadline_days("within 30 days of this letter"),
            Some(30)
        );
        assert_eq!(
            find_relative_deadline_days("innerhalb von 4 Wochen"),
            Some(28)
        );
        assert_eq!(find_relative_deadline_days("binnen 14 Tagen"), Some(14));
        assert_eq!(find_relative_deadline_days("within the next weeks"), None);
    }
}
