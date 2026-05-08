use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub struct CliArgs {
    pub log_file: PathBuf,
    pub db_file: Option<PathBuf>,
}

pub fn parse_args(argv: &[String]) -> Result<CliArgs, String> {
    let mut db_file: Option<PathBuf> = None;
    let mut log_file: Option<PathBuf> = None;

    // First argument is the executable so just skip it
    let mut argiter = argv[1..].iter();
    while let Some(s) = argiter.next() {
        if s.starts_with("--") {
            if s == "--db" {
                // We are expecting the database
                let db = argiter.next().ok_or("file is expected after --db")?;
                if db_file.is_some() {
                    return Err("database already set".into());
                }
                db_file = Some(PathBuf::from(db));
            } else {
                return Err("unknown flag".into());
            }
        } else {
            // The only mandatory parameter is the log file
            if log_file.is_some() {
                return Err("log file already set".into());
            }
            log_file = Some(PathBuf::from(s))
        }
    }

    let log_file = log_file.ok_or("log file is mandatory")?;
    Ok(CliArgs { log_file, db_file })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        std::iter::once("xapi-viewer")
            .chain(args.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn just_log_file() {
        let args = parse_args(&argv(&["foo.log"])).unwrap();
        assert_eq!(args.log_file, PathBuf::from("foo.log"));
        assert_eq!(args.db_file, None);
    }

    #[test]
    fn log_with_db_flag() {
        let args = parse_args(&argv(&["foo.log", "--db", "state.db"])).unwrap();
        assert_eq!(args.log_file, PathBuf::from("foo.log"));
        assert_eq!(args.db_file, Some(PathBuf::from("state.db")));
    }

    #[test]
    fn db_flag_before_log() {
        let args = parse_args(&argv(&["--db", "state.db", "foo.log"])).unwrap();
        assert_eq!(args.log_file, PathBuf::from("foo.log"));
        assert_eq!(args.db_file, Some(PathBuf::from("state.db")));
    }

    #[test]
    fn missing_value_after_db() {
        assert!(parse_args(&argv(&["foo.log", "--db"])).is_err());
    }

    #[test]
    fn no_log_file() {
        assert!(parse_args(&argv(&[])).is_err());
    }

    #[test]
    fn unknown_flag() {
        assert!(parse_args(&argv(&["foo.log", "--whatever"])).is_err());
    }

    #[test]
    fn two_positionals() {
        assert!(parse_args(&argv(&["a.log", "b.log"])).is_err());
    }
}
