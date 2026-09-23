use crate::remux::format_bytes;
use crate::ui::{Action, TrackDecision};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct AudioStripNotification<'a> {
    pub title: &'a str,
    pub origin: &'a str,
    pub kept: &'a str,
    pub stripped: &'a str,
    pub prev_size: &'a str,
    pub new_size: &'a str,
    pub reclaimed: &'a str,
}

#[must_use]
pub fn format_track_summaries(decisions: &[TrackDecision]) -> (String, String) {
    let mut kept = Vec::new();
    let mut stripped = Vec::new();
    for d in decisions {
        let label = format!("{} {} [{}]", d.language, d.channels, d.codec);
        match d.action {
            Action::Keep => kept.push(label),
            Action::Strip => stripped.push(label),
        }
    }
    (kept.join(", "), stripped.join(", "))
}

pub fn notify_strip(
    title: &str,
    origin_desc: &str,
    decisions: &[TrackDecision],
    orig_size: u64,
    saved: u64,
) {
    let (kept, stripped) = format_track_summaries(decisions);
    let new_size = orig_size.saturating_sub(saved);
    let prev_str = format_bytes(orig_size);
    let new_str = format_bytes(new_size);
    let rec_str = format_bytes(saved);

    let notif = AudioStripNotification {
        title,
        origin: origin_desc,
        kept: &kept,
        stripped: &stripped,
        prev_size: &prev_str,
        new_size: &new_str,
        reclaimed: &rec_str,
    };
    let _ = send_strip_notification(&notif);
}

pub fn notify_preserved_multi(
    title: &str,
    origin_desc: &str,
    decisions: &[TrackDecision],
    file_size: u64,
) {
    let (kept, _) = format_track_summaries(decisions);
    let size_str = format_bytes(file_size);

    let notif = AudioStripNotification {
        title,
        origin: origin_desc,
        kept: &kept,
        stripped: "None (Preserved Multi)",
        prev_size: &size_str,
        new_size: &size_str,
        reclaimed: "0 B (Multi Kept)",
    };
    let _ = send_strip_notification(&notif);
}

pub fn send_strip_notification(notif: &AudioStripNotification) -> Result<()> {
    let Some(bin) = find_ryoiki_bin() else {
        return Ok(());
    };

    let _ = Command::new(bin)
        .args([
            "notify",
            "audio-strip",
            "--title",
            notif.title,
            "--origin",
            notif.origin,
            "--kept",
            notif.kept,
            "--stripped",
            notif.stripped,
            "--prev-size",
            notif.prev_size,
            "--new-size",
            notif.new_size,
            "--reclaimed",
            notif.reclaimed,
        ])
        .output();

    Ok(())
}

fn find_ryoiki_bin() -> Option<PathBuf> {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join("ryoiki");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let home = std::env::var("HOME").ok()?;
    let candidate = Path::new(&home).join(".local/bin/ryoiki");
    if candidate.is_file() {
        return Some(candidate);
    }
    None
}
