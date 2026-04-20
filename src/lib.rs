use regex::Regex;
use std::sync::LazyLock;

const TASK_ID_PREFIX: &str = "D:";
const TASK_ID_HEX_LEN: usize = 12;

const REQUEST_ID_PREFIX: &str = "R:";
const REQUEST_ID_HEX_LEN: usize = 12;

const TRACK_ID_PREFIX: &str = "trackid=";
const TRACK_ID_HEX_LEN: usize = 32;

const UUID_PREFIX: &str = "uuid:";

const OPAQUE_REF_PREFIX: &str = "OpaqueRef:";

static TASK_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bD:[0-9a-fA-F]{12}\b").expect("TASK_ID_RE regex is invalid"));
static REQUEST_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bR:[0-9a-fA-F]{12}\b").expect("REQUEST_ID_RE regex is invalid"));
static TRACK_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\btrackid=[0-9a-fA-F]{32}\b").expect("TRACK_ID_RE regex is invalid")
});
static UUID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\buuid:[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b",
    )
    .expect("UUID_RE regex is invalid")
});

static OPAQUE_REF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\bOpaqueRef:(?:NULL|[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\b"
    )
    .expect("OPAQUE_REF_RE regex is invalid")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternKind {
    TaskId,
    RequestId,
    TrackId,
    Uuid,
    OpaqueRef,
}

#[derive(Debug, Default)]
pub struct PatternCounts {
    pub task_id: usize,
    pub request_id: usize,
    pub track_id: usize,
    pub uuid: usize,
    pub opaque_ref: usize,
}

/// Count occurrences of each pattern in a single line using compiled regexes.
///
/// Patterns are matched with word boundaries, so occurrences inside compound
/// tokens (e.g. `parent=trackid=...`) are correctly detected.
pub fn count_patterns_in_line(line: &str, counts: &mut PatternCounts) {
    counts.task_id += TASK_ID_RE.find_iter(line).count();
    counts.request_id += REQUEST_ID_RE.find_iter(line).count();
    counts.track_id += TRACK_ID_RE.find_iter(line).count();
    counts.uuid += UUID_RE.find_iter(line).count();
    counts.opaque_ref += OPAQUE_REF_RE.find_iter(line).count();
}

// Private predicates.
// These predicates validate a single candidate substring. Code that needs
// to count pattern occurrences in a whole line should use
// `count_patterns_in_line` instead, which uses regex for efficiency.
fn is_hex_id_with_prefix(s: &str, prefix: &str, hex_len: usize) -> bool {
    match s.strip_prefix(prefix) {
        None => false,
        Some(rest) => rest.len() == hex_len && rest.chars().all(|c| c.is_ascii_hexdigit()),
    }
}

// Return true if s is of form XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX
// where X is an hexadecimal digit
fn is_uuid_shaped(s: &str) -> bool {
    let part_len = [8, 4, 4, 4, 12];
    let parts: Vec<&str> = s.split('-').collect();

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

pub fn is_task_id(s: &str) -> bool {
    is_hex_id_with_prefix(s, TASK_ID_PREFIX, TASK_ID_HEX_LEN)
}

pub fn is_request_id(s: &str) -> bool {
    is_hex_id_with_prefix(s, REQUEST_ID_PREFIX, REQUEST_ID_HEX_LEN)
}

pub fn is_track_id(s: &str) -> bool {
    is_hex_id_with_prefix(s, TRACK_ID_PREFIX, TRACK_ID_HEX_LEN)
}

pub fn is_uuid(s: &str) -> bool {
    match s.strip_prefix(UUID_PREFIX) {
        None => false,
        Some(rest) => is_uuid_shaped(rest),
    }
}

pub fn is_opaque_ref(s: &str) -> bool {
    match s.strip_prefix(OPAQUE_REF_PREFIX) {
        None => false,
        Some("NULL") => true,
        Some(rest) => is_uuid_shaped(rest),
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

    // --- count_patterns_in_line ---

    #[test]
    fn count_empty_line_returns_zero() {
        let mut counts = PatternCounts::default();
        count_patterns_in_line("", &mut counts);
        assert_eq!(counts.task_id, 0);
        assert_eq!(counts.request_id, 0);
        assert_eq!(counts.track_id, 0);
        assert_eq!(counts.uuid, 0);
        assert_eq!(counts.opaque_ref, 0);
    }

    #[test]
    fn count_line_with_one_task_id() {
        let mut counts = PatternCounts::default();
        count_patterns_in_line("task dispatch D:ae5fb3924f47 created", &mut counts);
        assert_eq!(counts.task_id, 1);
    }

    #[test]
    fn count_line_with_two_task_ids() {
        // This is the common xapi shape: "task X created by task Y".
        let mut counts = PatternCounts::default();
        count_patterns_in_line(
            "task dispatch:event.from D:cdb76b62c8c1 created by task D:b3c7cbfe916e",
            &mut counts,
        );
        assert_eq!(counts.task_id, 2);
    }

    #[test]
    fn count_mixed_patterns_in_one_line() {
        // A realistic xapi session.create line.
        let mut counts = PatternCounts::default();
        count_patterns_in_line(
            "Session.create trackid=7db09a594ce3e498b0143bf7270424fa D:ae5fb3924f47 R:620f6218c82d",
            &mut counts,
        );
        assert_eq!(counts.task_id, 1);
        assert_eq!(counts.request_id, 1);
        assert_eq!(counts.track_id, 1);
        assert_eq!(counts.uuid, 0);
        assert_eq!(counts.opaque_ref, 0);
    }

    #[test]
    fn count_accumulates_across_calls() {
        // Calling count on two lines should sum, not reset.
        let mut counts = PatternCounts::default();
        count_patterns_in_line("first D:ae5fb3924f47", &mut counts);
        count_patterns_in_line("second D:b3c7cbfe916e", &mut counts);
        assert_eq!(counts.task_id, 2);
    }

    #[test]
    fn count_ignores_non_pattern_tokens() {
        let mut counts = PatternCounts::default();
        count_patterns_in_line("random words with no patterns 12345 hello", &mut counts);
        assert_eq!(counts.task_id, 0);
        assert_eq!(counts.request_id, 0);
        assert_eq!(counts.track_id, 0);
        assert_eq!(counts.uuid, 0);
        assert_eq!(counts.opaque_ref, 0);
    }

    #[test]
    fn count_opaque_ref_in_compound_token() {
        // FORMERLY a known limitation (slice 2f): the whitespace tokenizer missed
        // patterns embedded in compound tokens. As of slice 3, regex with word
        // boundaries detects these correctly.
        let mut counts = PatternCounts::default();
        count_patterns_in_line(
            "for uuid:22b24399-2a35-a70f-78b4-3fd3f978a9d1 ref=OpaqueRef:b12859d9-2107-8341-d4c5-d027be864d45",
            &mut counts,
        );
        assert_eq!(counts.uuid, 1);
        assert_eq!(counts.opaque_ref, 1); // was 0 under whitespace tokenization
    }

    // --- PatternKind ---

    #[test]
    fn pattern_kinds_are_equal_to_themselves() {
        assert_eq!(PatternKind::TaskId, PatternKind::TaskId);
    }

    #[test]
    fn pattern_kinds_are_not_equal_to_others() {
        assert_ne!(PatternKind::TaskId, PatternKind::RequestId);
        assert_ne!(PatternKind::Uuid, PatternKind::OpaqueRef);
    }

    #[test]
    fn pattern_kinds_are_copy() {
        // If PatternKind were not Copy, this code wouldn't compile.
        // The assignment `let b = a` would move `a`, and then `a` on the
        // last line would be a use-after-move error.
        let a = PatternKind::TaskId;
        let b = a;
        assert_eq!(a, b); // uses both `a` and `b` after the "copy"
    }

    #[test]
    fn pattern_kinds_have_debug_output() {
        // Just verify Debug doesn't panic and produces something.
        let s = format!("{:?}", PatternKind::OpaqueRef);
        assert!(!s.is_empty());
        assert!(s.contains("OpaqueRef")); // Debug format defaults to the variant name
    }
}
