use regex::Regex;
use std::ops::Range;
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
    Regex::new(r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b")
        .expect("UUID_RE regex is invalid")
});

static OPAQUE_REF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\bOpaqueRef:(?:NULL|[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\b"
    )
    .expect("OPAQUE_REF_RE regex is invalid")
});

/// Classifies the kind of identifier matched within a log line.
///
/// Each variant corresponds to one of the token formats recognised by the
/// XAPI log parser and determines how the match is coloured in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternKind {
    /// A task identifier of the form `D:<12 hex digits>`.
    TaskId,
    /// A request identifier of the form `R:<12 hex digits>`.
    RequestId,
    /// A tracking identifier of the form `trackid=<32 hex digits>`.
    TrackId,
    /// A UUID in standard hyphenated form, prefixed with `uuid:`.
    Uuid,
    /// An XAPI opaque reference: `OpaqueRef:<UUID>` or `OpaqueRef:NULL`.
    OpaqueRef,
}

/// A single pattern match found within a log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// The kind of identifier that was matched.
    pub kind: PatternKind,
    /// Byte range within the parent [`LogLine::raw`] string where the match
    /// starts and ends. The range is guaranteed to be valid UTF-8 boundaries.
    pub range: Range<usize>,
}

/// A parsed log line together with all identifier matches it contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// The original, unmodified text of the line.
    pub raw: String,
    /// All identifier matches found in `raw`, sorted by their start byte
    /// offset and guaranteed to be non-overlapping.
    pub matches: Vec<Match>,
}

/// Returns the index of the first [`Match`] in `matches` whose kind equals
/// `kind`, or the first match of any kind when `kind` is `None`.
///
/// Returns `None` when no qualifying match exists.
pub fn first_match_idx(matches: &[Match], kind: Option<PatternKind>) -> Option<usize> {
    matches
        .iter()
        .position(|m| kind.is_none_or(|k| m.kind == k))
}

/// Returns the index of the last [`Match`] in `matches` whose kind equals
/// `kind`, or the last match of any kind when `kind` is `None`.
///
/// Returns `None` when no qualifying match exists.
pub fn last_match_idx(matches: &[Match], kind: Option<PatternKind>) -> Option<usize> {
    matches
        .iter()
        .rposition(|m| kind.is_none_or(|k| m.kind == k))
}

/// Finds all occurrences of `re` in `line` and returns them as a `Vec<Match>`
/// tagged with the given `kind`.  The ranges are in byte offsets into `line`.
fn find_all_occurences_of(re: &LazyLock<Regex>, kind: PatternKind, line: &str) -> Vec<Match> {
    re.find_iter(line)
        .map(|m| Match {
            kind,
            range: m.start()..m.end(),
        })
        .collect()
}

/// Scans `line` for all recognised identifier patterns and returns them sorted
/// by start byte offset.
///
/// When an `OpaqueRef` token contains a bare UUID (which would also be matched
/// by the UUID pattern), the overlapping UUID match is discarded so that each
/// byte range appears at most once in the result.
pub fn find_all_matches(line: &str) -> Vec<Match> {
    use PatternKind::*;

    let mut v = find_all_occurences_of(&TASK_ID_RE, TaskId, line);
    v.extend(find_all_occurences_of(&REQUEST_ID_RE, RequestId, line));
    v.extend(find_all_occurences_of(&TRACK_ID_RE, TrackId, line));
    v.extend(find_all_occurences_of(&UUID_RE, Uuid, line));
    v.extend(find_all_occurences_of(&OPAQUE_REF_RE, OpaqueRef, line));

    v.sort_by_key(|a| a.range.start);

    // OpaqueRef and UUID overlap, only keep OpaqueRef in this case
    let mut filtered: Vec<Match> = Vec::new();

    for m in v {
        if let Some(prev) = filtered.last()
            && m.range.start < prev.range.end
        {
            // Overlap detected:
            // Drop matches that start inside the previous (already-kept) match.
            // OpaqueRef:UUID always wins over the bare UUID inside it because OpaqueRef
            // starts earlier and is therefore kept first by the sort.
            continue;
        }

        filtered.push(m);
    }

    filtered
}

/// Parses a raw log line string into a [`LogLine`] by running all pattern
/// matchers against it and storing the results alongside the original text.
pub fn parse_line(raw: String) -> LogLine {
    let matches = find_all_matches(&raw);
    LogLine { raw, matches }
}

/// Returns `true` when `s` consists of exactly `prefix` followed by exactly
/// `hex_len` ASCII hexadecimal characters, and `false` otherwise.
fn is_hex_id_with_prefix(s: &str, prefix: &str, hex_len: usize) -> bool {
    match s.strip_prefix(prefix) {
        None => false,
        Some(rest) => rest.len() == hex_len && rest.chars().all(|c| c.is_ascii_hexdigit()),
    }
}

/// Returns `true` when `s` is exactly of the form
/// `XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX` where every `X` is an ASCII
/// hexadecimal digit (groups of 8-4-4-4-12 characters separated by hyphens).
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

/// Returns `true` when `s` is a valid XAPI task identifier (`D:` followed by
/// 12 hexadecimal digits).
pub fn is_task_id(s: &str) -> bool {
    is_hex_id_with_prefix(s, TASK_ID_PREFIX, TASK_ID_HEX_LEN)
}

/// Returns `true` when `s` is a valid XAPI request identifier (`R:` followed
/// by 12 hexadecimal digits).
pub fn is_request_id(s: &str) -> bool {
    is_hex_id_with_prefix(s, REQUEST_ID_PREFIX, REQUEST_ID_HEX_LEN)
}

/// Returns `true` when `s` is a valid XAPI track identifier (`trackid=`
/// followed by 32 hexadecimal digits).
pub fn is_track_id(s: &str) -> bool {
    is_hex_id_with_prefix(s, TRACK_ID_PREFIX, TRACK_ID_HEX_LEN)
}

/// Returns `true` when `s` is of the form `uuid:<UUID>` where `<UUID>` is a
/// standard hyphenated UUID (8-4-4-4-12 hex groups).
pub fn is_uuid(s: &str) -> bool {
    match s.strip_prefix(UUID_PREFIX) {
        None => false,
        Some(rest) => is_uuid_shaped(rest),
    }
}

/// Returns `true` when `s` is a valid XAPI opaque reference: either
/// `OpaqueRef:NULL` (the null sentinel) or `OpaqueRef:<UUID>`.
pub fn is_opaque_ref(s: &str) -> bool {
    match s.strip_prefix(OPAQUE_REF_PREFIX) {
        None => false,
        Some("NULL") => true,
        Some(rest) => is_uuid_shaped(rest),
    }
}

/// Returns `s` unchanged when it is at most `max_chars` Unicode scalar values
/// long, or a prefix of exactly `max_chars` scalar values otherwise.
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
    // --- find_all_matches ---

    #[test]
    fn find_all_matches_empty_line() {
        let matches = find_all_matches("");
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn find_all_matches_no_patterns() {
        let matches = find_all_matches("random text with no patterns");
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn find_all_matches_single_task_id() {
        let line = "some text D:ae5fb3924f47 more text";
        let matches = find_all_matches(line);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].kind, PatternKind::TaskId);
        assert_eq!(&line[matches[0].range.clone()], "D:ae5fb3924f47");
    }

    #[test]
    fn find_all_matches_multiple_kinds() {
        // Realistic xapi line shape.
        let line = "Session.create trackid=7db09a594ce3e498b0143bf7270424fa D:ae5fb3924f47";
        let matches = find_all_matches(line);
        assert_eq!(matches.len(), 2);

        // First match should be the trackid (appears earlier in the string).
        assert_eq!(matches[0].kind, PatternKind::TrackId);
        assert_eq!(
            &line[matches[0].range.clone()],
            "trackid=7db09a594ce3e498b0143bf7270424fa"
        );

        // Second should be the task_id.
        assert_eq!(matches[1].kind, PatternKind::TaskId);
        assert_eq!(&line[matches[1].range.clone()], "D:ae5fb3924f47");
    }

    #[test]
    fn find_all_matches_returns_in_byte_order() {
        // Two matches of the SAME kind — verify order by position, not arbitrary.
        let line = "first D:aaaabbbbcccc then D:ddddeeeeffff";
        let matches = find_all_matches(line);
        assert_eq!(matches.len(), 2);
        assert!(matches[0].range.start < matches[1].range.start);
        assert_eq!(&line[matches[0].range.clone()], "D:aaaabbbbcccc");
        assert_eq!(&line[matches[1].range.clone()], "D:ddddeeeeffff");
    }

    #[test]
    fn find_all_matches_uuid_in_parentheses() {
        // Slice 3 lesson: UUIDs are always parenthesized in real xapi logs.
        let line = "task created (uuid:ef6e722e-a0fe-f91e-7c02-09ae2a256f7f) ok";
        let matches = find_all_matches(line);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].kind, PatternKind::Uuid);
        assert_eq!(
            &line[matches[0].range.clone()],
            "ef6e722e-a0fe-f91e-7c02-09ae2a256f7f"
        );
    }

    #[test]
    fn find_all_matches_five_different_kinds() {
        // All five kinds in one line.
        let line = "D:ae5fb3924f47 R:620f6218c82d trackid=e48e7b5a693b76fe0835dc08535e44fe uuid:22b24399-2a35-a70f-78b4-3fd3f978a9d1 OpaqueRef:b12859d9-2107-8341-d4c5-d027be864d45";
        let matches = find_all_matches(line);
        assert_eq!(matches.len(), 5);

        let kinds: Vec<PatternKind> = matches.iter().map(|m| m.kind).collect();
        assert!(kinds.contains(&PatternKind::TaskId));
        assert!(kinds.contains(&PatternKind::RequestId));
        assert!(kinds.contains(&PatternKind::TrackId));
        assert!(kinds.contains(&PatternKind::Uuid));
        assert!(kinds.contains(&PatternKind::OpaqueRef));
    }

    #[test]
    fn find_all_matches_range_lengths_are_correct() {
        let line = "D:aaaabbbbcccc";
        let matches = find_all_matches(line);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].range.len(), 14); // "D:" + 12 hex = 14 chars
    }

    #[test]
    fn find_all_matches_dedupes_uuid_inside_opaque_ref() {
        let line = "ref OpaqueRef:b12859d9-2107-8341-d4c5-d027be864d45 done";
        let matches = find_all_matches(line);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].kind, PatternKind::OpaqueRef);
    }

    // --- LogLine / parse_line ---

    #[test]
    fn parse_line_empty_string() {
        let log_line = parse_line(String::new());
        assert_eq!(log_line.raw, "");
        assert_eq!(log_line.matches.len(), 0);
    }

    #[test]
    fn parse_line_no_patterns() {
        let log_line = parse_line("just some text".to_string());
        assert_eq!(log_line.raw, "just some text");
        assert_eq!(log_line.matches.len(), 0);
    }

    #[test]
    fn parse_line_with_one_pattern() {
        let raw = "task D:ae5fb3924f47 done".to_string();
        let log_line = parse_line(raw);
        assert_eq!(log_line.raw, "task D:ae5fb3924f47 done");
        assert_eq!(log_line.matches.len(), 1);
        assert_eq!(log_line.matches[0].kind, PatternKind::TaskId);
    }

    #[test]
    fn parse_line_with_multiple_patterns() {
        let raw =
            "Session.create trackid=7db09a594ce3e498b0143bf7270424fa D:ae5fb3924f47".to_string();
        let log_line = parse_line(raw);
        assert_eq!(log_line.matches.len(), 2);
        // Verify byte order is preserved.
        assert_eq!(log_line.matches[0].kind, PatternKind::TrackId);
        assert_eq!(log_line.matches[1].kind, PatternKind::TaskId);
    }

    #[test]
    fn parse_line_matches_index_into_raw_correctly() {
        // Critical invariant: the byte ranges in matches must point to valid
        // substrings of raw. This catches any future bug where parse_line
        // somehow desyncs raw and matches.
        let raw = "found D:aaaabbbbcccc here".to_string();
        let log_line = parse_line(raw);
        assert_eq!(log_line.matches.len(), 1);
        let matched_text = &log_line.raw[log_line.matches[0].range.clone()];
        assert_eq!(matched_text, "D:aaaabbbbcccc");
    }

    #[test]
    fn parse_line_preserves_raw_text_exactly() {
        let raw = "  weird   spacing\twith\ttabs  ".to_string();
        let log_line = parse_line(raw.clone());
        assert_eq!(log_line.raw, raw);
    }
}
