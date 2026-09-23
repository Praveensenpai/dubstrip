use anyhow::Result;
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::ai;
use crate::decide;
use crate::notify;
use crate::probe;
use crate::remux;
use crate::ui;

struct SweepJob {
    path: PathBuf,
    keep_indices: Vec<u32>,
    strip_count: usize,
    filename: String,
    title: String,
    origin_desc: String,
    decisions: Vec<ui::TrackDecision>,
    orig_size: u64,
}

pub fn handle_sweep(dir: &Path, auto: bool, dry_run: bool) -> Result<()> {
    println!(
        "\n  {} Sweeping directory: {}",
        "🔍".cyan().bold(),
        dir.display().to_string().bold()
    );

    let jobs = collect_sweep_jobs(dir, auto, dry_run)?;
    if jobs.is_empty() {
        println!(
            "\n  {} No media files with redundant dubs found.\n",
            "•".dimmed()
        );
        return Ok(());
    }

    if dry_run {
        println!(
            "\n  {} [Dry-run] Found {} file(s) with redundant dub tracks.\n",
            "✔".green(),
            jobs.len().to_string().bold()
        );
        return Ok(());
    }

    let (successful, total_saved) = execute_sweep_jobs(&jobs);
    println!(
        "  {} Batch sweep complete! Processed {}/{} files. Total space reclaimed: {}\n",
        "✔".green().bold(),
        successful.to_string().bold(),
        jobs.len(),
        remux::format_bytes(total_saved).green().bold()
    );

    Ok(())
}

fn collect_sweep_jobs(dir: &Path, auto: bool, dry_run: bool) -> Result<Vec<SweepJob>> {
    let mut jobs = Vec::new();
    for entry in walk_video_files(dir) {
        let Ok(media) = probe::probe_file(&entry) else {
            continue;
        };
        let stream_langs: Vec<String> = media
            .audio_streams
            .iter()
            .map(|s| s.language.clone())
            .collect();
        let origin = ai::resolve_film_origin(&entry, &stream_langs, !auto)?;
        let decisions = decide::evaluate_audio_streams(&media, &origin);
        let strip_count = decisions
            .iter()
            .filter(|d| d.action == ui::Action::Strip)
            .count();

        let filename = entry.file_name().map_or_else(
            || "Unknown".to_string(),
            |s| s.to_string_lossy().into_owned(),
        );

        if strip_count == 0 {
            println!(
                "  {} {} is already clean or preserved as multi ({})",
                "✔".green(),
                filename.dimmed(),
                origin.native_lang_name.dimmed()
            );
            continue;
        }

        ui::render_inspection_table(&media, &origin, &decisions);

        let origin_desc = format!(
            "{} [{}% confident] (via {})",
            origin.native_lang_name, origin.confidence, origin.source
        );
        let keep_indices = if dry_run {
            println!("  🔍 [Dry-run] Would queue for stripping ({strip_count} dubs)");
            Vec::new()
        } else {
            match ui::prompt_confirmation(&decisions, auto) {
                Ok(indices) => {
                    println!("  {} Queued for batch stripping.", "✔".green());
                    indices
                }
                Err(_) => {
                    println!("  {} Skipped by user.", "•".dimmed());
                    continue;
                }
            }
        };

        jobs.push(SweepJob {
            path: entry,
            keep_indices,
            strip_count,
            filename,
            title: origin.title,
            origin_desc,
            decisions,
            orig_size: media.size_bytes,
        });
    }
    Ok(jobs)
}

fn execute_sweep_jobs(jobs: &[SweepJob]) -> (usize, u64) {
    let mut total_saved = 0u64;
    let mut successful = 0usize;
    let total = jobs.len();

    println!(
        "\n  {} Starting batch strip on {} queued file(s)...\n",
        "🚀".cyan().bold(),
        total.to_string().bold()
    );

    for (i, job) in jobs.iter().enumerate() {
        println!(
            "  [{}/{}] Remuxing {}...",
            i + 1,
            total,
            job.filename.bold()
        );
        match remux::remux_lossless(&job.path, &job.keep_indices) {
            Ok(saved) => {
                total_saved += saved;
                successful += 1;
                println!(
                    "    {} Stripped {} dubs (reclaimed: {})\n",
                    "✔".green().bold(),
                    job.strip_count,
                    remux::format_bytes(saved).green().bold()
                );
                notify::notify_strip(
                    &job.title,
                    &job.origin_desc,
                    &job.decisions,
                    job.orig_size,
                    saved,
                );
            }
            Err(err) => {
                eprintln!("    ⚠️ Failed to remux {}: {err}\n", job.filename);
            }
        }
    }
    (successful, total_saved)
}

fn walk_video_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walk_video_files(&path));
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                if ext_lower == "mkv" || ext_lower == "mp4" {
                    files.push(path);
                }
            }
        }
    }
    files
}
