use clap::{Args, Parser, Subcommand};
use json_state_tx::{Options, TxError};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "json-state-tx",
    version,
    long_version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("JSON_STATE_TX_GIT_SHA"), ")"),
    about = "Conditional transactions for one JSON state file"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Read(ReadArgs),
    Init(InitArgs),
    Apply(ApplyArgs),
}

#[derive(Args)]
struct Common {
    #[arg(long)]
    file: PathBuf,
    #[arg(long, default_value_t = 15_000)]
    lock_timeout_ms: u64,
    #[arg(long, default_value_t = json_state_tx::DEFAULT_MAX_STATE_BYTES)]
    max_state_bytes: u64,
}

#[derive(Args)]
struct ReadArgs {
    #[command(flatten)]
    common: Common,
}

#[derive(Args)]
struct InitArgs {
    #[command(flatten)]
    common: Common,
    #[arg(long, value_name = "PATH")]
    document: PathBuf,
    #[arg(long, value_name = "PATH")]
    schema: Option<PathBuf>,
}

#[derive(Args)]
struct ApplyArgs {
    #[command(flatten)]
    common: Common,
    #[arg(long, value_name = "PATH")]
    patch: PathBuf,
    #[arg(long)]
    if_version: String,
    #[arg(long, value_name = "PATH")]
    schema: Option<PathBuf>,
    #[arg(long, default_value_t = json_state_tx::DEFAULT_MAX_PATCH_BYTES)]
    max_patch_bytes: u64,
    #[arg(long, default_value_t = json_state_tx::DEFAULT_MAX_PATCH_OPS)]
    max_patch_ops: usize,
}

#[derive(Serialize)]
struct Success<T: Serialize> {
    ok: bool,
    result: T,
}

#[derive(Serialize)]
struct Failure<'a> {
    ok: bool,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'static str,
    phase: &'static str,
    message: &'a str,
    expected: &'a Option<String>,
    actual: &'a Option<String>,
    committed: Option<bool>,
    os_error: Option<i32>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(value) => {
            println!(
                "{}",
                serde_json::to_string(&Success {
                    ok: true,
                    result: value
                })
                .expect("serializable success envelope")
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!(
                "{}",
                serde_json::to_string(&Failure {
                    ok: false,
                    error: ErrorBody {
                        code: error.code,
                        phase: error.phase,
                        message: &error.message,
                        expected: &error.expected,
                        actual: &error.actual,
                        committed: error.committed,
                        os_error: error.os_error,
                    },
                })
                .expect("serializable error envelope")
            );
            ExitCode::from(error.exit_code())
        }
    }
}

fn run(cli: Cli) -> Result<Value, TxError> {
    match cli.command {
        Command::Read(args) => {
            let snapshot =
                json_state_tx::read(&args.common.file, &options(&args.common, None, None))?;
            serde_json::to_value(snapshot).map_err(serialization_error)
        }
        Command::Init(args) => {
            let document = read_limited(&args.document, args.common.max_state_bytes, "document")?;
            let schema = read_schema(args.schema.as_deref(), args.common.max_state_bytes)?;
            let result = json_state_tx::init(
                &args.common.file,
                &document,
                schema.as_ref(),
                &options(&args.common, None, None),
            )?;
            serde_json::to_value(result).map_err(serialization_error)
        }
        Command::Apply(args) => {
            let patch = read_limited(&args.patch, args.max_patch_bytes, "patch")?;
            let schema = read_schema(args.schema.as_deref(), args.common.max_state_bytes)?;
            let opts = options(
                &args.common,
                Some(args.max_patch_bytes),
                Some(args.max_patch_ops),
            );
            let result = json_state_tx::apply(
                &args.common.file,
                &patch,
                &args.if_version,
                schema.as_ref(),
                &opts,
            )?;
            serde_json::to_value(result).map_err(serialization_error)
        }
    }
}

fn options(common: &Common, max_patch_bytes: Option<u64>, max_patch_ops: Option<usize>) -> Options {
    let mut options = Options {
        lock_timeout: Duration::from_millis(common.lock_timeout_ms),
        max_state_bytes: common.max_state_bytes,
        ..Options::default()
    };
    if let Some(value) = max_patch_bytes {
        options.max_patch_bytes = value;
    }
    if let Some(value) = max_patch_ops {
        options.max_patch_ops = value;
    }
    options
}

fn read_limited(path: &Path, max_bytes: u64, label: &'static str) -> Result<Vec<u8>, TxError> {
    let metadata = std::fs::metadata(path).map_err(|e| input_io(label, e))?;
    if metadata.len() > max_bytes {
        return Err(TxError {
            code: "INPUT_TOO_LARGE",
            phase: "input",
            message: format!("{label} exceeds the configured byte limit"),
            expected: None,
            actual: None,
            committed: Some(false),
            os_error: None,
        });
    }
    std::fs::read(path).map_err(|e| input_io(label, e))
}

fn read_schema(path: Option<&Path>, max_bytes: u64) -> Result<Option<Value>, TxError> {
    path.map(|path| {
        let bytes = read_limited(path, max_bytes, "schema")?;
        serde_json::from_slice(&bytes).map_err(|e| TxError {
            code: "INVALID_SCHEMA",
            phase: "input",
            message: e.to_string(),
            expected: None,
            actual: None,
            committed: Some(false),
            os_error: None,
        })
    })
    .transpose()
}

fn input_io(label: &'static str, error: std::io::Error) -> TxError {
    TxError {
        code: "INPUT_IO",
        phase: "input",
        message: format!("cannot read {label}: {error}"),
        expected: None,
        actual: None,
        committed: Some(false),
        os_error: error.raw_os_error(),
    }
}

fn serialization_error(error: serde_json::Error) -> TxError {
    TxError {
        code: "SERIALIZE_FAILED",
        phase: "output",
        message: error.to_string(),
        expected: None,
        actual: None,
        committed: Some(false),
        os_error: None,
    }
}
