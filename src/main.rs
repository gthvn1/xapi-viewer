use std::fs::File;
use std::io::{BufRead, BufReader};

use xapi_viewer::{PatternCounts, count_patterns_in_line, truncate_for_display};

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

    // Initialize counters
    let mut last_line: String = first_line.clone(); // allocation
    let mut line_count: usize = 1;
    let mut byte_count: usize = first_line.len() + 1;
    let mut counts = PatternCounts::default();
    count_patterns_in_line(&first_line, &mut counts);

    // Loop: count lines/bytes/patterns
    // TODO: use faster API BufRead::read_line
    for line in lines_iter {
        let line = line?;
        line_count += 1;
        byte_count += line.len() + 1;
        count_patterns_in_line(&line, &mut counts);
        last_line = line;
    }

    // Print summary
    println!("{}: {} lines, {} bytes", &path, line_count, byte_count);
    println!("  {:11}:{:>4}", "task_id", counts.task_id);
    println!("  {:11}:{:>4}", "request_id", counts.request_id);
    println!("  {:11}:{:>4}", "track_id", counts.track_id);
    println!("  {:11}:{:>4}", "UUID", counts.uuid);
    println!("  {:11}:{:>4}", "OpaqueRef", counts.opaque_ref);
    println!("First line: {}", truncate_for_display(&first_line, 100));
    println!("Last line: {}", truncate_for_display(&last_line, 100));

    // TEMPORARY: touch the thing...
    use xapi_viewer::parse_line;
    let file = File::open(&path)?;
    let sample: Vec<_> = BufReader::new(file)
        .lines()
        .take(3)
        .filter_map(|l| l.ok())
        .map(parse_line)
        .collect();
    for log_line in &sample {
        dbg!(log_line);
    }

    Ok(())
}
