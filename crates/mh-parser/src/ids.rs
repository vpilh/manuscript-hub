/// Generic manuscript-ID candidates such as `JSS-2026-0142` or `PLOS-D-26-01234`,
/// deduplicated, in order of appearance. Journal-specific patterns from the rule
/// catalog take precedence once they exist.
pub fn find_manuscript_ids(text: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let candidate = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if looks_like_manuscript_id(candidate) && !ids.iter().any(|id| id == candidate) {
            ids.push(candidate.to_string());
        }
    }
    ids
}

fn looks_like_manuscript_id(s: &str) -> bool {
    let digits = s.chars().filter(char::is_ascii_digit).count();
    (6..=40).contains(&s.len())
        && s.starts_with(|c: char| c.is_ascii_uppercase())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
        && s.contains(['-', '.', '_'])
        && digits >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_typical_ids() {
        assert_eq!(
            find_manuscript_ids(
                "Manuscript ID JSS-2026-0142 (ref. JSS-2026-0142), see PLOS-D-26-01234."
            ),
            ["JSS-2026-0142", "PLOS-D-26-01234"]
        );
    }

    #[test]
    fn skips_dates_and_short_codes() {
        assert!(find_manuscript_ids("on 2026-11-12, COVID-19, Ms. Smith").is_empty());
    }
}
