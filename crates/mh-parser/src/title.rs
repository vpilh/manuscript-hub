/// The paper's title, from a quoted title next to a cue ("your article
/// "…"", "entitled "…"", "mit dem Titel „…""), or a `Title:` field as in
/// submission-system mail. `None` when nothing title-like is found.
pub fn find_title(body: &str) -> Option<String> {
    quoted_title(body).or_else(|| title_field(body))
}

/// Words that introduce a paper's title within the few words before a quote.
const CUES: &[&str] = &[
    "article",
    "manuscript",
    "submission",
    "paper",
    "entitled",
    "titled",
    "title",
    "beitrag",
    "manuskript",
    "titel",
    "arbeit",
];

const QUOTES: &[(char, &[char])] = &[
    ('"', &['"']),
    ('\u{201C}', &['\u{201D}']),
    ('\u{201E}', &['\u{201C}', '\u{201D}']),
    ('\u{00AB}', &['\u{00BB}']),
];

fn quoted_title(text: &str) -> Option<String> {
    for (index, open) in text.char_indices() {
        let Some((_, closers)) = QUOTES.iter().find(|(o, _)| *o == open) else {
            continue;
        };
        let inner_start = index + open.len_utf8();
        let Some(length) = text[inner_start..].find(*closers) else {
            continue;
        };
        let candidate = &text[inner_start..inner_start + length];
        if !looks_like_title(candidate) {
            continue;
        }
        let mut before_start = index.saturating_sub(60);
        while !text.is_char_boundary(before_start) {
            before_start += 1;
        }
        let before = text[before_start..index].to_lowercase();
        let cued = before
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .rev()
            .take(5)
            .any(|word| CUES.contains(&word));
        if cued {
            return Some(clean(candidate));
        }
    }
    None
}

/// `Title: …` or `Manuscript Title: …`, up to the end of the line or the
/// next field (HTML-to-text conversion can run fields together).
fn title_field(text: &str) -> Option<String> {
    for (index, _) in text.match_indices("Title:") {
        let rest = text[index + "Title:".len()..].lines().next().unwrap_or("");
        let end = ["Authors:", "Author:", "Manuscript", "Article Type:"]
            .iter()
            .filter_map(|field| rest.find(field))
            .min()
            .unwrap_or(rest.len());
        let candidate = rest[..end]
            .trim()
            .trim_matches(['"', '\u{201C}', '\u{201D}']);
        if looks_like_title(candidate) {
            return Some(clean(candidate));
        }
    }
    None
}

fn looks_like_title(text: &str) -> bool {
    let words = text.split_whitespace().count();
    (3..=40).contains(&words) && (10..=300).contains(&text.len())
}

fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_quoted_titles_after_cues() {
        assert_eq!(
            find_title("We have received your article \"Synthetic Studies of Example Data\" for consideration.")
                .as_deref(),
            Some("Synthetic Studies of Example Data")
        );
        assert_eq!(
            find_title("Ihr Beitrag mit dem Titel \u{201E}Beispielhafte Studien zu Testdaten\u{201C} wurde")
                .as_deref(),
            Some("Beispielhafte Studien zu Testdaten")
        );
        assert_eq!(
            find_title("the manuscript entitled \u{201C}A Note on\n  Invented Results\u{201D} is")
                .as_deref(),
            Some("A Note on Invented Results")
        );
    }

    #[test]
    fn finds_title_fields() {
        assert_eq!(
            find_title("Journal of Synthetic StudiesTitle: Invented Methods for Example DataAuthors: A. Example")
                .as_deref(),
            Some("Invented Methods for Example Data")
        );
    }

    #[test]
    fn ignores_quotes_without_a_cue_and_short_quotes() {
        assert_eq!(
            find_title("Click \"Submissions Being Processed\" to continue."),
            None
        );
        assert_eq!(find_title("your article \"Draft\" is"), None);
        assert_eq!(find_title("No title here."), None);
    }
}
