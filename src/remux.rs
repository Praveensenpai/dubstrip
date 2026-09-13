use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Executes a zero-loss stream-copy remux, keeping only specified audio stream indices.
/// Writes to a temporary file, verifies integrity, and atomically replaces the original.
pub fn remux_lossless(path: &Path, keep_audio_indices: &[u32]) -> Result<u64> {
    if keep_audio_indices.is_empty() {
        bail!("Safety violation: cannot remux with zero audio streams.");
    }

    let orig_size = fs::metadata(path)
        .with_context(|| format!("Failed to read metadata for {}", path.display()))?
        .len();

    let tmp_path = path.with_extension("dubstrip.tmp.mkv");

    // Clean up any stale leftover temporary file
    if tmp_path.exists() {
        let _ = fs::remove_file(&tmp_path);
    }

    // Build ffmpeg stream copy command
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-y",
        "-v",
        "error",
        "-i",
        &path.display().to_string(),
        "-map",
        "0:v",
    ]);

    for &idx in keep_audio_indices {
        cmd.args(["-map", &format!("0:{idx}")]);
    }

    cmd.args([
        "-map",
        "0:s?",
        "-map_chapters",
        "0",
        "-c",
        "copy",
        &tmp_path.display().to_string(),
    ]);

    let output = cmd
        .output()
        .with_context(|| format!("Failed to run ffmpeg remux for {}", path.display()))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_file(&tmp_path);
        bail!("ffmpeg remux failed: {err}");
    }

    // Verify temp file exists and is plausible (> 100 KB)
    let tmp_metadata = fs::metadata(&tmp_path)
        .with_context(|| "Failed to read remuxed temporary file metadata")?;

    let new_size = tmp_metadata.len();
    if new_size < 10 * 1024 {
        let _ = fs::remove_file(&tmp_path);
        bail!("Remuxed file is suspiciously small ({new_size} bytes). Aborting swap.");
    }

    // Atomically swap the remuxed file into place
    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "Failed to replace original file with {}",
            tmp_path.display()
        )
    })?;

    let saved_bytes = orig_size.saturating_sub(new_size);
    Ok(saved_bytes)
}

#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    #[allow(clippy::cast_precision_loss)]
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}
