/// The new part of a message: drops `>`-quoted lines and everything after a
/// reply or forward header, so a decision quoted from an older message cannot
/// produce a new event (PRD §6).
pub fn strip_quoted(body: &str) -> String {
    let mut kept = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if is_reply_boundary(trimmed) {
            break;
        }
        if trimmed.starts_with('>') {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n")
}

fn is_reply_boundary(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("-----original message")
        || lower.starts_with("-----ursprüngliche nachricht")
        || lower.starts_with("---------- forwarded message")
        || lower.starts_with("---------- weitergeleitete nachricht")
        || (line.len() >= 10 && line.chars().all(|c| c == '_'))
        || (lower.starts_with("on ") && lower.ends_with("wrote:"))
        || (lower.starts_with("am ") && lower.contains("schrieb") && lower.ends_with(':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_quoted_lines_and_reply_history() {
        let body = "Revision received, thank you.\n\
                    > We are pleased to accept your paper\n\
                    Best regards\n\
                    On Mon, 2 Mar 2026, Editor <ed@example.org> wrote:\n\
                    Your paper has been rejected.";
        assert_eq!(
            strip_quoted(body),
            "Revision received, thank you.\nBest regards"
        );
    }

    #[test]
    fn handles_german_reply_headers() {
        let body = "Neue Nachricht\nAm 02.03.2026 um 10:00 schrieb Redaktion <r@example.org>:\nalt";
        assert_eq!(strip_quoted(body), "Neue Nachricht");
    }
}
