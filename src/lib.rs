//! Conditional transactions for one JSON state file.
//!
//! The permanent sidecar lock is the cooperative synchronization boundary.
//! The state file itself is replaced at commit, so locking it would lock the
//! old file identity rather than the stable logical resource.

use atomic_write_file::AtomicWriteFile;
use json_patch::Patch;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

pub const DEFAULT_MAX_STATE_BYTES: u64 = 16 * 1024 * 1024;
pub const DEFAULT_MAX_PATCH_BYTES: u64 = 1024 * 1024;
pub const DEFAULT_MAX_PATCH_OPS: usize = 10_000;

#[derive(Debug, Clone)]
pub struct Options {
    pub lock_timeout: Duration,
    pub max_state_bytes: u64,
    pub max_patch_bytes: u64,
    pub max_patch_ops: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            lock_timeout: Duration::from_secs(15),
            max_state_bytes: DEFAULT_MAX_STATE_BYTES,
            max_patch_bytes: DEFAULT_MAX_PATCH_BYTES,
            max_patch_ops: DEFAULT_MAX_PATCH_OPS,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub version: String,
    pub document: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    pub previous_version: Option<String>,
    pub version: String,
    pub document: Value,
    pub durability: &'static str,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct TxError {
    pub code: &'static str,
    pub phase: &'static str,
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub committed: Option<bool>,
    pub os_error: Option<i32>,
}

impl TxError {
    pub fn exit_code(&self) -> u8 {
        match self.code {
            "VERSION_CONFLICT" => 20,
            "LOCK_TIMEOUT" => 21,
            "INVALID_JSON" | "INVALID_PATCH" | "PATCH_APPLY_FAILED" | "SCHEMA_REJECTED" => 30,
            "COMMIT_OUTCOME_UNKNOWN" => 41,
            _ => 40,
        }
    }

    fn new(code: &'static str, phase: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            phase,
            message: message.into(),
            expected: None,
            actual: None,
            committed: Some(false),
            os_error: None,
        }
    }

    fn io(code: &'static str, phase: &'static str, error: io::Error) -> Self {
        let os_error = error.raw_os_error();
        let mut result = Self::new(code, phase, error.to_string());
        result.os_error = os_error;
        result
    }
}

pub fn version_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut version = String::with_capacity(71);
    version.push_str("sha256:");
    for byte in digest {
        write!(&mut version, "{byte:02x}").expect("writing to a String cannot fail");
    }
    version
}

pub fn read(path: &Path, options: &Options) -> Result<Snapshot, TxError> {
    let _lock = acquire_lock(path, options.lock_timeout)?;
    read_locked(path, options.max_state_bytes)
}

pub fn init(
    path: &Path,
    document_bytes: &[u8],
    schema: Option<&Value>,
    options: &Options,
) -> Result<Commit, TxError> {
    if document_bytes.len() as u64 > options.max_state_bytes {
        return Err(TxError::new(
            "STATE_TOO_LARGE",
            "input",
            "document exceeds the configured byte limit",
        ));
    }
    let document: Value = serde_json::from_slice(document_bytes)
        .map_err(|e| TxError::new("INVALID_JSON", "input", e.to_string()))?;
    validate_schema(&document, schema)?;

    ensure_parent(path)?;
    let _lock = acquire_lock(path, options.lock_timeout)?;
    if path.exists() {
        let current = read_locked(path, options.max_state_bytes)?;
        let mut error = TxError::new("VERSION_CONFLICT", "compare", "state already exists");
        error.expected = Some("null".to_owned());
        error.actual = Some(current.version);
        return Err(error);
    }

    let bytes = encode_document(&document, options.max_state_bytes)?;
    publish(path, &bytes)?;
    Ok(Commit {
        previous_version: None,
        version: version_bytes(&bytes),
        document,
        durability: platform_durability(),
    })
}

pub fn apply(
    path: &Path,
    patch_bytes: &[u8],
    expected_version: &str,
    schema: Option<&Value>,
    options: &Options,
) -> Result<Commit, TxError> {
    if patch_bytes.len() as u64 > options.max_patch_bytes {
        return Err(TxError::new(
            "PATCH_TOO_LARGE",
            "input",
            "patch exceeds the configured byte limit",
        ));
    }
    let patch: Patch = serde_json::from_slice(patch_bytes)
        .map_err(|e| TxError::new("INVALID_PATCH", "input", e.to_string()))?;
    if patch.0.len() > options.max_patch_ops {
        return Err(TxError::new(
            "PATCH_TOO_LARGE",
            "input",
            "patch exceeds the configured operation limit",
        ));
    }
    if !expected_version.starts_with("sha256:")
        || expected_version.len() != 71
        || !expected_version[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(TxError::new(
            "INVALID_VERSION",
            "input",
            "version must be sha256 followed by 64 lowercase hexadecimal characters",
        ));
    }

    let _lock = acquire_lock(path, options.lock_timeout)?;
    let current = read_locked(path, options.max_state_bytes)?;
    if current.version != expected_version {
        let mut error = TxError::new(
            "VERSION_CONFLICT",
            "compare",
            "state version does not match --if-version",
        );
        error.expected = Some(expected_version.to_owned());
        error.actual = Some(current.version);
        return Err(error);
    }

    let previous_version = current.version;
    let mut candidate = current.document;
    json_patch::patch(&mut candidate, &patch)
        .map_err(|e| TxError::new("PATCH_APPLY_FAILED", "patch", e.to_string()))?;
    validate_schema(&candidate, schema)?;
    let bytes = encode_document(&candidate, options.max_state_bytes)?;
    publish(path, &bytes)?;

    Ok(Commit {
        previous_version: Some(previous_version),
        version: version_bytes(&bytes),
        document: candidate,
        durability: platform_durability(),
    })
}

fn ensure_parent(path: &Path) -> Result<(), TxError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| TxError::io("STATE_IO", "prepare", e))
}

fn lock_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

fn acquire_lock(path: &Path, timeout: Duration) -> Result<File, TxError> {
    ensure_parent(path)?;
    let lock_path = lock_path(path);
    let deadline = Instant::now() + timeout;
    loop {
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(_) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(25));
                continue;
            }
            Err(error) => return Err(lock_timeout_error(&lock_path, error)),
        };
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Err(error) => return Err(lock_timeout_error(&lock_path, error.into())),
        }
    }
}

fn lock_timeout_error(path: &Path, error: io::Error) -> TxError {
    let mut result = TxError::new(
        "LOCK_TIMEOUT",
        "lock",
        format!("timed out acquiring {}: {error}", path.display()),
    );
    result.os_error = error.raw_os_error();
    result
}

fn read_locked(path: &Path, max_bytes: u64) -> Result<Snapshot, TxError> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        let code = if e.kind() == io::ErrorKind::NotFound {
            "STATE_NOT_FOUND"
        } else {
            "STATE_IO"
        };
        TxError::io(code, "read", e)
    })?;
    if metadata.len() > max_bytes {
        return Err(TxError::new(
            "STATE_TOO_LARGE",
            "read",
            "state exceeds the configured byte limit",
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| TxError::io("STATE_IO", "read", e))?;
    let document = serde_json::from_slice(&bytes)
        .map_err(|e| TxError::new("INVALID_JSON", "read", e.to_string()))?;
    Ok(Snapshot {
        version: version_bytes(&bytes),
        document,
    })
}

fn validate_schema(document: &Value, schema: Option<&Value>) -> Result<(), TxError> {
    let Some(schema) = schema else {
        return Ok(());
    };
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(schema)
        .map_err(|e| TxError::new("INVALID_SCHEMA", "schema_compile", e.to_string()))?;
    let errors: Vec<String> = validator
        .iter_errors(document)
        .take(16)
        .map(|e| e.to_string())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(TxError::new(
            "SCHEMA_REJECTED",
            "validate",
            errors.join("; "),
        ))
    }
}

fn encode_document(document: &Value, max_bytes: u64) -> Result<Vec<u8>, TxError> {
    let mut bytes = serde_json::to_vec_pretty(document)
        .map_err(|e| TxError::new("SERIALIZE_FAILED", "serialize", e.to_string()))?;
    bytes.push(b'\n');
    if bytes.len() as u64 > max_bytes {
        return Err(TxError::new(
            "STATE_TOO_LARGE",
            "serialize",
            "candidate exceeds the configured byte limit",
        ));
    }
    Ok(bytes)
}

fn publish(path: &Path, bytes: &[u8]) -> Result<(), TxError> {
    let mut file = AtomicWriteFile::open(path)
        .map_err(|e| TxError::io("TEMP_CREATE_FAILED", "temp_create", e))?;
    file.write_all(bytes)
        .map_err(|e| TxError::io("TEMP_WRITE_FAILED", "temp_write", e))?;
    file.commit().map_err(|e| {
        let mut error = TxError::io("COMMIT_OUTCOME_UNKNOWN", "publish", e);
        error.committed = None;
        error
    })
}

#[cfg(unix)]
fn platform_durability() -> &'static str {
    "file_and_parent_directory_synced"
}

#[cfg(windows)]
fn platform_durability() -> &'static str {
    "content_flushed_metadata_best_effort"
}

#[cfg(not(any(unix, windows)))]
fn platform_durability() -> &'static str {
    "platform_dependent"
}
