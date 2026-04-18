const TASK_ID_PREFIX: &str = "D:";
const TASK_ID_HEX_LEN: usize = 12;

pub fn is_task_id(s: &str) -> bool {
    match s.strip_prefix(TASK_ID_PREFIX) {
        None => false,
        Some(rest) => rest.len() == TASK_ID_HEX_LEN && rest.chars().all(|c| c.is_ascii_hexdigit()),
    }
}

pub fn truncate_for_display(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        s.chars().take(max_chars).collect::<String>()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_task_id() {
        assert!(is_task_id("D:ae5fb3924f47"));
    }

    #[test]
    fn rejects_empty_task_id() {
        assert!(!is_task_id(""));
    }

    #[test]
    fn rejects_wrong_prefix() {
        assert!(!is_task_id("R:ae5fb3924f47"));
    }

    #[test]
    fn rejects_no_prefix() {
        assert!(!is_task_id("ae5fb3924f47"));
    }

    #[test]
    fn rejects_too_short_task_id() {
        assert!(!is_task_id("D:ae5fb392"));
    }

    #[test]
    fn rejects_too_long_task_id() {
        assert!(!is_task_id("D:ae5fb39212312132"));
    }

    #[test]
    fn rejects_non_hex_chars() {
        assert!(!is_task_id("D:ae5fb3924fzz"));
    }

    #[test]
    fn task_id_length_constant_matches_real_sample() {
        // From the xapi log sample:
        let sample = "D:ae5fb3924f47";
        assert!(is_task_id(sample));
        // And verify the constant is what we think:
        assert_eq!(sample.len() - TASK_ID_PREFIX.len(), TASK_ID_HEX_LEN);
    }

    #[test]
    fn truncate_shorter_than_max_returns_unchanged() {
        assert_eq!(truncate_for_display("hello", 100), "hello");
    }

    #[test]
    fn truncate_longer_than_max_returns_max_chars() {
        let s = "a".repeat(200);
        let result = truncate_for_display(&s, 100);
        assert_eq!(result.chars().count(), 100);
    }

    #[test]
    fn truncate_respects_max_chars_parameter() {
        let s = "abcdefghij";
        assert_eq!(truncate_for_display(s, 5), "abcde");
    }
}
