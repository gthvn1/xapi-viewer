const TASK_ID_PREFIX: &str = "D:";
const TASK_ID_HEX_LEN: usize = 12;

const REQUEST_ID_PREFIX: &str = "R:";
const REQUEST_ID_HEX_LEN: usize = 12;

const TRACK_ID_PREFIX: &str = "trackid=";
const TRACK_ID_HEX_LEN: usize = 32;

const UUID_PREFIX: &str = "uuid:";

pub fn is_task_id(s: &str) -> bool {
    match s.strip_prefix(TASK_ID_PREFIX) {
        None => false,
        Some(rest) => rest.len() == TASK_ID_HEX_LEN && rest.chars().all(|c| c.is_ascii_hexdigit()),
    }
}

pub fn is_request_id(s: &str) -> bool {
    match s.strip_prefix(REQUEST_ID_PREFIX) {
        None => false,
        Some(rest) => {
            rest.len() == REQUEST_ID_HEX_LEN && rest.chars().all(|c| c.is_ascii_hexdigit())
        }
    }
}

pub fn is_track_id(s: &str) -> bool {
    match s.strip_prefix(TRACK_ID_PREFIX) {
        None => false,
        Some(rest) => rest.len() == TRACK_ID_HEX_LEN && rest.chars().all(|c| c.is_ascii_hexdigit()),
    }
}

pub fn is_uuid(s: &str) -> bool {
    match s.strip_prefix(UUID_PREFIX) {
        None => false,
        Some(rest) => {
            // We are expecting 8, 4, 4, 4 and 12 hex chars
            let part_len = [8, 4, 4, 4, 12];
            let parts: Vec<&str> = rest.split('-').collect();

            if parts.len() != part_len.len() {
                return false;
            };

            for (p, expected_len) in std::iter::zip(parts, part_len) {
                if p.len() != expected_len || !p.chars().all(|c| c.is_ascii_hexdigit()) {
                    return false;
                }
            }

            true
        }
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
    fn accepts_task_id_valid() {
        assert!(is_task_id("D:ae5fb3924f47"));
    }

    #[test]
    fn rejects_task_id_empty() {
        assert!(!is_task_id(""));
    }

    #[test]
    fn rejects_task_id_wrong_prefix() {
        assert!(!is_task_id("R:ae5fb3924f47"));
    }

    #[test]
    fn rejects_task_id_no_prefix() {
        assert!(!is_task_id("ae5fb3924f47"));
    }

    #[test]
    fn rejects_task_id_too_short() {
        assert!(!is_task_id("D:ae5fb392"));
    }

    #[test]
    fn rejects_task_id_too_long() {
        assert!(!is_task_id("D:ae5fb39212312132"));
    }

    #[test]
    fn rejects_task_id_non_hex_chars() {
        assert!(!is_task_id("D:ae5fb3924fzz"));
    }

    #[test]
    fn check_task_id_length_constant_matches_real_sample() {
        // From the xapi log sample:
        let sample = "D:ae5fb3924f47";
        assert!(is_task_id(sample));
        // And verify the constant is what we think:
        assert_eq!(sample.len() - TASK_ID_PREFIX.len(), TASK_ID_HEX_LEN);
    }

    #[test]
    fn accepts_req_id_valid() {
        assert!(is_request_id("R:620f6218c82d"));
    }

    #[test]
    fn rejects_req_id_empty() {
        assert!(!is_request_id(""));
    }

    #[test]
    fn rejects_req_id_wrong_prefix() {
        assert!(!is_request_id("D:ae5fb3924f47"));
    }

    #[test]
    fn rejects_req_id_no_prefix() {
        assert!(!is_request_id("ae5fb3924f47"));
    }

    #[test]
    fn rejects_req_id_too_short() {
        assert!(!is_request_id("R:a"));
    }

    #[test]
    fn rejects_req_id_too_long() {
        assert!(!is_request_id("R:123456789ABCDEF1"));
    }

    #[test]
    fn rejects_req_id_non_hex_chars() {
        assert!(!is_request_id("R:56789ABCDEFG"));
    }

    #[test]
    fn check_req_id_length_constant_matches_real_sample() {
        // From the xapi log sample:
        let sample = "R:2f23e0b62781";
        assert!(is_request_id(sample));
        // And verify the constant is what we think:
        assert_eq!(sample.len() - REQUEST_ID_PREFIX.len(), REQUEST_ID_HEX_LEN);
    }

    #[test]
    fn accepts_track_id_valid() {
        assert!(is_track_id("trackid=e48e7b5a693b76fe0835dc08535e44fe"));
    }

    #[test]
    fn rejects_track_id_empty() {
        assert!(!is_track_id(""));
    }

    #[test]
    fn rejects_track_id_wrong_prefix() {
        assert!(!is_track_id("tid=e48e7b5a693b76fe0835dc08535e44fe"));
    }

    #[test]
    fn rejects_track_id_no_prefix() {
        assert!(!is_track_id("e48e7b5a693b76fe0835dc08535e44fe"));
    }

    #[test]
    fn rejects_track_id_too_short() {
        assert!(!is_track_id("trackid=e48e7b5a693b76fe0835dc08535e44f"));
    }

    #[test]
    fn rejects_track_id_too_long() {
        assert!(!is_track_id("trackid=e48e7b5a693b76fe0835dc08535e44fe1"));
    }

    #[test]
    fn rejects_track_id_non_hex_chars() {
        assert!(!is_track_id("trackid=z48e7b5a693b76fe0835dc08535e44fe"));
    }

    #[test]
    fn check_track_id_length_constant_matches_real_sample() {
        // From the xapi log sample:
        let sample = "trackid=e48e7b5a693b76fe0835dc08535e44fe";
        assert!(is_track_id(sample));
        // And verify the constant is what we think:
        assert_eq!(sample.len() - TRACK_ID_PREFIX.len(), TRACK_ID_HEX_LEN);
    }

    #[test]
    fn accepts_uuid_lowercase_hex() {
        assert!(is_uuid("uuid:22b24399-2a35-a70f-78b4-3fd3f978a9d1"));
    }

    #[test]
    fn accepts_uuid_uppercase_hex() {
        assert!(is_uuid("uuid:12B24F99-2A35-A70F-78B4-3FD3F9780000"));
    }

    #[test]
    fn rejects_uuid_no_hyphens() {
        assert!(!is_uuid("uuid:22b243992a35a70f78b43fd3f978a9d1"));
    }

    #[test]
    fn rejects_uuid_wrong_hyphens_placement() {
        assert!(!is_uuid("uuid:22b243992-a35-a70f-78b4-3fd3f978a9d1"));
    }

    #[test]
    fn rejects_uuid_non_hex() {
        assert!(!is_uuid("uuid:z2b24399-2a35-a70f-78b4-3fd3f978a9d1"));
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
