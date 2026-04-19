const TASK_ID_PREFIX: &str = "D:";
const TASK_ID_HEX_LEN: usize = 12;

const REQUEST_ID_PREFIX: &str = "R:";
const REQUEST_ID_HEX_LEN: usize = 12;

const TRACK_ID_PREFIX: &str = "trackid=";
const TRACK_ID_HEX_LEN: usize = 32;

const UUID_PREFIX: &str = "uuid:";

const OPAQUE_REF_PREFIX: &str = "OpaqueRef:";

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

pub fn is_opaque_ref(s: &str) -> bool {
    match s.strip_prefix(OPAQUE_REF_PREFIX) {
        None => false,
        Some("NULL") => true,
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

    // --- task_id ---

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

    // --- request_id ---

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

    // --- trackid ---

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

    // --- UUID ---

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

    // --- OpaqueRef: valid cases ---

    #[test]
    fn accepts_opaque_ref_valid() {
        assert!(is_opaque_ref(
            "OpaqueRef:b12859d9-2107-8341-d4c5-d027be864d45"
        ));
    }

    #[test]
    fn accepts_opaque_ref_null() {
        // OpaqueRef:NULL is xapi's "no reference" sentinel. Must be accepted.
        assert!(is_opaque_ref("OpaqueRef:NULL"));
    }

    #[test]
    fn accepts_opaque_ref_uppercase_hex() {
        // Lenient, same as is_uuid.
        assert!(is_opaque_ref(
            "OpaqueRef:B12859D9-2107-8341-D4C5-D027BE864D45"
        ));
    }

    // --- OpaqueRef: invalid cases ---

    #[test]
    fn rejects_opaque_ref_empty() {
        assert!(!is_opaque_ref(""));
    }

    #[test]
    fn rejects_opaque_ref_prefix_only() {
        // Just the prefix with nothing after.
        assert!(!is_opaque_ref("OpaqueRef:"));
    }

    #[test]
    fn rejects_opaque_ref_wrong_prefix_case() {
        // The prefix is case-sensitive. "opaqueref:" is not valid.
        assert!(!is_opaque_ref(
            "opaqueref:b12859d9-2107-8341-d4c5-d027be864d45"
        ));
    }

    #[test]
    fn rejects_opaque_ref_no_prefix() {
        assert!(!is_opaque_ref("b12859d9-2107-8341-d4c5-d027be864d45"));
    }

    #[test]
    fn rejects_opaque_ref_uuid_prefix() {
        // Right body shape, wrong prefix — this is a uuid, not an opaque_ref.
        assert!(!is_opaque_ref("uuid:b12859d9-2107-8341-d4c5-d027be864d45"));
    }

    #[test]
    fn rejects_opaque_ref_null_lowercase() {
        // NULL is a literal — lowercase "null" is not accepted.
        assert!(!is_opaque_ref("OpaqueRef:null"));
    }

    #[test]
    fn rejects_opaque_ref_null_with_trailing_garbage() {
        // "NULL" followed by anything = not a valid opaque_ref.
        assert!(!is_opaque_ref("OpaqueRef:NULL!"));
    }

    #[test]
    fn rejects_opaque_ref_wrong_body_shape() {
        // 7-4-4-4-12 instead of 8-4-4-4-12.
        assert!(!is_opaque_ref(
            "OpaqueRef:b12859d-2107-8341-d4c5-d027be864d45"
        ));
    }

    #[test]
    fn rejects_opaque_ref_non_hex_body() {
        assert!(!is_opaque_ref(
            "OpaqueRef:z12859d9-2107-8341-d4c5-d027be864d45"
        ));
    }

    #[test]
    fn rejects_opaque_ref_no_hyphens() {
        assert!(!is_opaque_ref(
            "OpaqueRef:b128 59d921078341d4c5d027be864d45"
        ));
    }

    #[test]
    fn rejects_opaque_ref_trailing_garbage_after_uuid() {
        // Real xapi logs often have trailing characters. Our function should
        // reject them — whoever calls us is responsible for extracting the
        // candidate substring before calling us.
        assert!(!is_opaque_ref(
            "OpaqueRef:1c026124-2dde-3b57-dca7-405a43ecf019!"
        ));
    }

    // --- truncate ---

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
