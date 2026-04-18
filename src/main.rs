const TASK_ID_PREFIX: &str = "D:";
const TASK_ID_HEX_LEN: usize = 12;

#[allow(dead_code)]
fn is_task_id(s: &str) -> bool {
    match s.strip_prefix(TASK_ID_PREFIX) {
        None => false,
        Some(rest) => rest.len() == TASK_ID_HEX_LEN && rest.chars().all(|c| c.is_ascii_hexdigit()),
    }
}

fn main() -> std::io::Result<()> {
    let mut args = std::env::args();
    let path = match args.nth(1) {
        None => {
            eprintln!("Usage: xapi-viewer <path>");
            std::process::exit(1);
        }
        Some(p) => p,
    };

    // Read sample file
    let contents = std::fs::read_to_string(&path)?;
    println!("{}: {} bytes", &path, contents.len());

    Ok(())
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
}
