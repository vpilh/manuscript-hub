//! Title normalization and similarity for matching mail without a manuscript ID (FR10).

/// Lowercase words of at least two characters; punctuation and case are ignored.
pub fn normalize_title(title: &str) -> Vec<String> {
    title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .map(str::to_lowercase)
        .collect()
}

/// Jaccard similarity of the title word sets, in `0.0..=1.0`.
pub fn title_similarity(a: &str, b: &str) -> f32 {
    let a = normalize_title(a);
    let b = normalize_title(b);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.iter().filter(|w| b.contains(w)).count();
    let union = a.len() + b.iter().filter(|w| !a.contains(w)).count();
    shared as f32 / union as f32
}

/// Titles at or above this similarity are suggested as the same manuscript.
pub const TITLE_MATCH_THRESHOLD: f32 = 0.8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_case_and_punctuation() {
        assert_eq!(
            title_similarity(
                "Synthetic Signals: in Simulated Data",
                "synthetic signals in simulated data."
            ),
            1.0
        );
    }

    #[test]
    fn distinguishes_different_titles() {
        assert!(title_similarity("Synthetic signals", "Quantum gardening") < TITLE_MATCH_THRESHOLD);
        assert_eq!(title_similarity("", "anything"), 0.0);
    }
}
