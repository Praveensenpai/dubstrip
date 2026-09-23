use anyhow::{bail, Result};
use colored::Colorize;
use std::io::{self, BufRead, Write};

use crate::ai::FilmOrigin;
use crate::probe::MediaInfo;
use crate::remux::format_bytes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Keep,
    Strip,
}

#[derive(Debug, Clone)]
pub struct TrackDecision {
    pub index: u32,
    pub language: String,
    pub codec: String,
    pub channels: String,
    pub title: String,
    pub action: Action,
    pub reason: String,
}

/// Renders a formatted analysis table showing all detected streams and planned actions.
pub fn render_inspection_table(
    media: &MediaInfo,
    origin: &FilmOrigin,
    decisions: &[TrackDecision],
) {
    let filename = media.path.file_name().map_or_else(
        || "Unknown".to_string(),
        |s| s.to_string_lossy().into_owned(),
    );

    println!("\n  {} {}", "🎬 Analyzing:".bold(), filename.cyan());
    println!(
        "  {} {} ({})",
        "📦 File Size:".bold(),
        format_bytes(media.size_bytes),
        format!(
            "{} video, {} audio, {} subs",
            media.video_count,
            media.audio_streams.len(),
            media.subtitle_count
        )
        .dimmed()
    );

    if origin.native_lang_code != "und" {
        let conf_badge = if origin.is_confident() {
            format!("[{}% confident]", origin.confidence).green()
        } else {
            format!("[{}% confident — UNCERTAIN]", origin.confidence)
                .yellow()
                .bold()
        };
        println!(
            "  {} {} ({}) {conf_badge} — via {}",
            "🧠 Detected Origin:".bold(),
            origin.native_lang_name.green().bold(),
            origin.native_lang_code.dimmed(),
            origin.source.dimmed()
        );
        if !origin.is_confident() {
            println!(
                "  {} Low confidence — preserving all audio tracks as Multi",
                "🛡️".yellow().bold()
            );
        }
    } else {
        println!(
            "  {} {} (Will preserve all valid original tracks as Multi)",
            "⚠️ Detected Origin:".yellow().bold(),
            "Unresolved / Undefined".yellow()
        );
    }

    println!("  {}", "─".repeat(70).dimmed());
    println!(
        "  {:<4} {:<8} {:<12} {:<10} {:<24} {:<8}",
        "#".bold(),
        "Lang".bold(),
        "Channels".bold(),
        "Codec".bold(),
        "Track Title".bold(),
        "Action".bold()
    );
    println!("  {}", "─".repeat(70).dimmed());

    for d in decisions {
        let action_label = match d.action {
            Action::Keep => "KEEP".green().bold(),
            Action::Strip => "STRIP".red().bold(),
        };

        let short_title = if d.title.chars().count() > 22 {
            format!("{}…", d.title.chars().take(21).collect::<String>())
        } else if d.title.is_empty() {
            "—".dimmed().to_string()
        } else {
            d.title.clone()
        };

        println!(
            "  {:<4} {:<8} {:<12} {:<10} {:<24} {}  {}",
            d.index.to_string().cyan(),
            d.language,
            d.channels,
            d.codec,
            short_title,
            action_label,
            format!("({})", d.reason).dimmed()
        );
    }
    println!("  {}", "─".repeat(70).dimmed());
}

/// Asks for confirmation or allows custom per-track selection.
pub fn prompt_confirmation(decisions: &[TrackDecision], auto_yes: bool) -> Result<Vec<u32>> {
    let strip_count = decisions
        .iter()
        .filter(|d| d.action == Action::Strip)
        .count();
    let default_keep: Vec<u32> = decisions
        .iter()
        .filter(|d| d.action == Action::Keep)
        .map(|d| d.index)
        .collect();

    if strip_count == 0 {
        println!("  {} No dub audio tracks need stripping.", "✔".green());
        return Ok(default_keep);
    }

    if auto_yes {
        return Ok(default_keep);
    }

    print!("\n  Apply strip? [Y/n/custom] (default Y): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let choice = input.trim().to_lowercase();

    match choice.as_str() {
        "" | "y" | "yes" => Ok(default_keep),
        "n" | "no" => {
            bail!("Operation cancelled by user.");
        }
        "c" | "custom" => prompt_custom_selection(decisions),
        _ => {
            println!("  Invalid input. Cancelling.");
            bail!("Operation cancelled.");
        }
    }
}

/// Allows the user to interactively choose which track to keep.
fn prompt_custom_selection(decisions: &[TrackDecision]) -> Result<Vec<u32>> {
    println!("\n  {} Custom Track Selection:", "🛠".bold());
    let mut chosen = Vec::new();

    for d in decisions {
        print!(
            "  Keep Track #{} [{} - {}] (y/N)? ",
            d.index.to_string().cyan(),
            d.language.bold(),
            d.title
        );
        io::stdout().flush()?;
        let mut ans = String::new();
        io::stdin().lock().read_line(&mut ans)?;
        if ans.trim().eq_ignore_ascii_case("y") {
            chosen.push(d.index);
        }
    }

    if chosen.is_empty() {
        bail!("Safety rule: you must keep at least one audio track!");
    }

    Ok(chosen)
}
