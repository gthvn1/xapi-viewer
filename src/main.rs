use std::fs::File;
use std::io::{BufRead, BufReader};

use xapi_viewer::truncate_for_display;

fn main() -> std::io::Result<()> {
    // Read path from command line
    let mut args = std::env::args();
    let path = match args.nth(1) {
        None => {
            eprintln!("Usage: xapi-viewer <path>");
            std::process::exit(1);
        }
        Some(p) => p,
    };

    // Read file
    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    let mut lines_iter = reader.lines();

    let first_line: String = match lines_iter.next() {
        None => {
            println!("file is empty");
            std::process::exit(0);
        }
        Some(line) => line?,
    };

    let mut last_line: String = first_line.clone(); // allocation
    let mut line_count: usize = 1;
    let mut byte_count: usize = first_line.len() + 1;

    // TODO: use faster API BufRead::read_line
    for line in lines_iter {
        let line = line?;
        line_count += 1;
        byte_count += line.len() + 1;
        last_line = line;
    }

    println!("{}: {} lines, {} bytes", &path, line_count, byte_count);
    println!("First line: {}", truncate_for_display(&first_line, 100));
    println!("Last line: {}", truncate_for_display(&last_line, 100));

    Ok(())
}
