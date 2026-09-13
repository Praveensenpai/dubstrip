use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime};

/// Extensions used by download clients for in-progress files.
const INCOMPLETE_EXTENSIONS: &[&str] =
    &["!qb", "part", "crdownload", "tmp", "downloading", "aria2"];

/// Verifies whether a video file is safe and complete to process.
/// Enforces safety gates to prevent touching downloading/copying files.
/// When `bypass_quiescence` is true, skips the 60s settling timer while still
/// validating open write locks, container index, and incomplete extensions.
pub fn is_file_safe_and_complete(path: &Path, bypass_quiescence: bool) -> Result<bool> {
    if !passes_name_and_path_check(path) {
        return Ok(false);
    }

    if is_file_actively_locked(path)? {
        return Ok(false);
    }

    if !bypass_quiescence && !passes_quiescence_window(path)? {
        return Ok(false);
    }

    if !passes_container_index_probe(path) {
        return Ok(false);
    }

    Ok(true)
}

/// Gate 1: Rejects in-progress extensions or paths in incomplete download folders.
pub fn passes_name_and_path_check(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();
    if path_str.contains("/incomplete/") || path_str.contains("/downloading/") {
        return false;
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        if INCOMPLETE_EXTENSIONS.contains(&ext_lower.as_str()) {
            return false;
        }
    }

    true
}

/// Gate 2: Checks if any process holds an open write lock on the file via `fuser` (fallback to `lsof`).
pub fn is_file_actively_locked(path: &Path) -> Result<bool> {
    match Command::new("fuser").arg(path).output() {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stdout.contains('w') || stderr.contains('w') || !stdout.trim().is_empty() {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            // Fall back to lsof if fuser is missing
            if let Ok(output) = Command::new("lsof").arg(path).output() {
                if output.status.success() && !output.stdout.is_empty() {
                    return Ok(true);
                }
            }
            // If neither tool is installed, rely on the remaining 3 safety gates
            Ok(false)
        }
        Err(err) => {
            Err(err).with_context(|| format!("Failed to check file lock for {}", path.display()))
        }
    }
}

/// Gate 3: Verifies that the file has not been modified within the last 60 seconds.
pub fn passes_quiescence_window(path: &Path) -> Result<bool> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to read metadata for {}", path.display()))?;

    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
        if elapsed < Duration::from_secs(60) {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Gate 4: Quick ffprobe index validation ensuring container headers are intact.
pub fn passes_container_index_probe(path: &Path) -> bool {
    let status = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            &path.display().to_string(),
        ])
        .output();

    status.is_ok_and(|out| out.status.success() && !out.stdout.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incomplete_extensions_rejected() {
        let p1 = Path::new("/downloads/movie.mkv.!qB");
        let p2 = Path::new("/downloads/movie.mkv.part");
        let p3 = Path::new("/downloads/incomplete/movie.mkv");
        let p4 = Path::new("/downloads/completed/movie.mkv");

        assert!(!passes_name_and_path_check(p1));
        assert!(!passes_name_and_path_check(p2));
        assert!(!passes_name_and_path_check(p3));
        assert!(passes_name_and_path_check(p4));
    }
}
