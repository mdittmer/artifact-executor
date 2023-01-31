use argh::FromArgs;
use std::path::PathBuf;

pub const DEFAULT_LOG_LEVEL: &'static str = "warn";

fn default_log_level() -> String {
    DEFAULT_LOG_LEVEL.to_string()
}

fn default_cache_directory() -> PathBuf {
    PathBuf::from("./ae-cache")
}

/// execute programs when inputs have changed.
#[derive(Debug, FromArgs, PartialEq)]
pub struct Args {
    /// log level.
    #[argh(option, default = "default_log_level()")]
    pub log_level: String,

    /// directory where previous program execution data is stored.
    #[argh(option, default = "default_cache_directory()")]
    pub cache_directory: PathBuf,

    #[argh(subcommand)]
    pub command: Command,
}

/// artifact-executor command.
#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand)]
pub enum Command {
    Execute(Execute),
}

/// execute a program.
#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name = "execute")]
pub struct Execute {
    /// file where program executable is stored.
    #[argh(option)]
    pub program: PathBuf,

    /// file where environment variable `key=value` pairs are stored.
    #[argh(option)]
    pub environment: PathBuf,

    /// file where manifest of input files is stored.
    #[argh(option)]
    pub inputs: PathBuf,

    /// file where manifest of output files is stored.
    #[argh(option)]
    pub outputs: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::Args;
    use argh::FromArgs as _;

    const OK_EXECUTE_ARGS: [&'static str; 8] = [
        "--program",
        "./exe",
        "--environment",
        "./env",
        "--inputs",
        "./in",
        "--outputs",
        "./out",
    ];

    #[test]
    fn test_defaults() {
        let cmd = ["test-artifact-executor"];
        let mut args: Vec<&str> = vec!["execute"];
        args.extend(OK_EXECUTE_ARGS);
        let _args = Args::from_args(&cmd, &args).expect("args with defaults to work");
    }

    #[test]
    fn test_log_level() {
        let cmd = ["test-artifact-executor"];
        for log_level in ["trace", "debug", "info", "warn", "error"] {
            let mut args: Vec<&str> = vec!["--log-level", log_level, "execute"];
            args.extend(OK_EXECUTE_ARGS);
            let _args = Args::from_args(&cmd, &args).expect("args with valid log-level to work");
        }
    }

    #[test]
    fn test_cache_dir() {
        let cmd = ["test-artifact-executor"];
        let mut args: Vec<&str> = vec!["--cache-directory", "/custom/cache/dir", "execute"];
        args.extend(OK_EXECUTE_ARGS);
        let _args = Args::from_args(&cmd, &args).expect("args with custom cache directory to work");
    }

    #[test]
    fn test_log_level_and_cache_dir() {
        let cmd = ["test-artifact-executor"];
        for log_level in ["trace", "debug", "info", "warn", "error"] {
            let mut args: Vec<&str> = vec![
                "--cache-directory",
                "/custom/cache/dir",
                "--log-level",
                log_level,
                "execute",
            ];
            args.extend(OK_EXECUTE_ARGS);
            let _args = Args::from_args(&cmd, &args).expect("args with valid log-level to work");
        }
    }

    #[test]
    fn test_missing_program() {
        let cmd = ["test-artifact-executor"];
        let args = vec![
            "execute",
            // Missing: `--program [program]`.
            "--environment",
            "./env",
            "--inputs",
            "./in",
            "--outputs",
            "./out",
        ];
        assert!(Args::from_args(&cmd, &args).is_err());
    }

    #[test]
    fn test_missing_env() {
        let cmd = ["test-artifact-executor"];
        let args = vec![
            "execute",
            "--program",
            "./exe",
            // Missing: `--environment [environment]`.
            "--inputs",
            "./in",
            "--outputs",
            "./out",
        ];

        assert!(Args::from_args(&cmd, &args).is_err());
    }

    #[test]
    fn test_missing_inputs() {
        let cmd = ["test-artifact-executor"];
        let args = vec![
            "execute",
            "--program",
            "./exe",
            "--environment",
            "./env",
            // Missing: `--inputs [inputs]`.
            "--outputs",
            "./out",
        ];
        assert!(Args::from_args(&cmd, &args).is_err());
    }

    #[test]
    fn test_missing_outputs() {
        let cmd = ["test-artifact-executor"];
        let args = vec![
            "execute",
            "--program",
            "./exe",
            "--environment",
            "./env",
            "--inputs",
            "./in",
            // Missing: `--outputs [out]`.
        ];
        assert!(Args::from_args(&cmd, &args).is_err());
    }
}
