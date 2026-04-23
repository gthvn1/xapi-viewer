use std::fs::File;
use std::io::{BufRead, BufReader};

use xapi_viewer::{PatternKind, parse_line, truncate_for_display};

// TODO: extract analyse_file() and FileAnalysis that will hold counters.
fn main() -> std::io::Result<()> {
    // Parse args: read path from command line
    let mut args = std::env::args();
    let path = match args.nth(1) {
        None => {
            eprintln!("Usage: xapi-viewer <path>");
            std::process::exit(1);
        }
        Some(p) => p,
    };

    // Open the file and get line iterator
    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    let mut lines_iter = reader.lines();

    // Get first line
    let first_line: String = match lines_iter.next() {
        None => {
            println!("file is empty");
            std::process::exit(0);
        }
        Some(line) => line?,
    };

    let mut last_line: String = first_line.clone();

    // Initialize counters
    let mut line_count: usize = 1; // we count the first line
    let mut byte_count: usize = first_line.len() + 1;
    let mut all_matches: Vec<xapi_viewer::Match> = Vec::new();

    // We pass a clone() because we want to keep first line for summary
    let first_log_line = parse_line(first_line.clone());
    all_matches.extend(first_log_line.matches);

    // Loop: count lines/bytes/patterns
    // TODO: use faster API BufRead::read_line
    for line in lines_iter {
        let line = line?;
        line_count += 1;
        byte_count += line.len() + 1;

        let log_line = parse_line(line);
        all_matches.extend(log_line.matches);

        last_line = log_line.raw;
    }

    // Compute all counts
    let task_id_count = all_matches
        .iter()
        .filter(|m| m.kind == PatternKind::TaskId)
        .count();

    let request_id_count = all_matches
        .iter()
        .filter(|m| m.kind == PatternKind::RequestId)
        .count();

    let track_id_count = all_matches
        .iter()
        .filter(|m| m.kind == PatternKind::TrackId)
        .count();

    let uuid_count = all_matches
        .iter()
        .filter(|m| m.kind == PatternKind::Uuid)
        .count();

    let opaque_ref_count = all_matches
        .iter()
        .filter(|m| m.kind == PatternKind::OpaqueRef)
        .count();

    // Print summary
    println!("{}: {} lines, {} bytes", &path, line_count, byte_count);
    println!("  {:11}: {:>4}", "task_id", task_id_count);
    println!("  {:11}: {:>4}", "request_id", request_id_count);
    println!("  {:11}: {:>4}", "track_id", track_id_count);
    println!("  {:11}: {:>4}", "UUID", uuid_count);
    println!("  {:11}: {:>4}", "OpaqueRef", opaque_ref_count);
    println!("First line: {}", truncate_for_display(&first_line, 100));
    println!("Last line:  {}", truncate_for_display(&last_line, 100));

    Ok(())
}
