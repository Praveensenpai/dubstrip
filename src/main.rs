mod ai;
mod decide;
mod probe;
mod remux;
mod safety;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "dubstrip")]
#[command(author = "Praveensenpai <pvnt20@gmail.com>")]
#[command(version)]
#[command(about = "Autonomous, AI-augmented zero-loss dub stripper for movies & series", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect audio streams, detected film origin, and planned actions without touching file
    Inspect {
        /// Path to video file
        path: PathBuf,
    },
    /// Strip unwanted dub audio from a video file
    Strip {
        /// Path to video file
        path: PathBuf,

        /// Automatically proceed without interactive confirmation
        #[arg(short = 'y', long = "yes", alias = "auto")]
        auto: bool,

        /// Only preview changes and space savings without modifying file
        #[arg(long)]
        dry_run: bool,

        /// Skip safety settling check for completed local files
        #[arg(long)]
        force: bool,
    },
    /// Sweep a directory and strip unwanted dub audio across all media files
    Sweep {
        /// Path to folder to sweep
        path: PathBuf,

        /// Automatically proceed without interactive confirmation
        #[arg(short = 'y', long = "yes", alias = "auto")]
        auto: bool,

        /// Only preview changes and space savings without modifying files
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Inspect { path } => handle_inspect(&path),
        Commands::Strip {
            path,
            auto,
            dry_run,
            force,
        } => handle_strip(&path, auto, dry_run, force),
        Commands::Sweep {
            path,
            auto,
            dry_run,
        } => handle_sweep(&path, auto, dry_run),
    }
}

fn handle_inspect(path: &Path) -> Result<()> {
    let media = probe::probe_file(path)?;
    let origin = ai::resolve_film_origin(path)?;
    let decisions = decide::evaluate_audio_streams(&media, &origin);

    ui::render_inspection_table(&media, &origin, &decisions);
    Ok(())
}

fn handle_strip(path: &Path, auto: bool, dry_run: bool, force: bool) -> Result<()> {
    if !force && !safety::is_file_safe_and_complete(path)? {
        println!(
            "  {} Skipping {}: File is downloading, active, or incomplete.",
            "⏳".yellow(),
            path.display().to_string().cyan()
        );
        return Ok(());
    }

    let media = probe::probe_file(path)?;
    let origin = ai::resolve_film_origin(path)?;
    let decisions = decide::evaluate_audio_streams(&media, &origin);

    ui::render_inspection_table(&media, &origin, &decisions);

    let strip_count = decisions
        .iter()
        .filter(|d| d.action == ui::Action::Strip)
        .count();

    if strip_count == 0 {
        return Ok(());
    }

    if dry_run {
        println!(
            "\n  {} [Dry-run] Would strip {} audio tracks losslessly.",
            "🔍".cyan(),
            strip_count.to_string().bold()
        );
        return Ok(());
    }

    let keep_indices = ui::prompt_confirmation(&decisions, auto)?;
    println!("  {} Remuxing losslessly (stream-copy)...", "▶".cyan());

    let saved = remux::remux_lossless(path, &keep_indices)?;
    println!(
        "  {} Successfully stripped {} dubs! Reclaimed: {}\n",
        "✔".green().bold(),
        strip_count.to_string().cyan().bold(),
        remux::format_bytes(saved).green().bold()
    );

    Ok(())
}

struct SweepJob {
    path: PathBuf,
    keep_indices: Vec<u32>,
    strip_count: usize,
    filename: String,
}

fn collect_sweep_jobs(dir: &Path, auto: bool, dry_run: bool) -> Result<Vec<SweepJob>> {
    let mut jobs = Vec::new();
    for entry in walk_video_files(dir) {
        let Ok(media) = probe::probe_file(&entry) else {
            continue;
        };
        let origin = ai::resolve_film_origin(&entry)?;
        let decisions = decide::evaluate_audio_streams(&media, &origin);
        let strip_count = decisions
            .iter()
            .filter(|d| d.action == ui::Action::Strip)
            .count();

        if strip_count == 0 {
            continue;
        }

        ui::render_inspection_table(&media, &origin, &decisions);
        let filename = entry.file_name().map_or_else(
            || "Unknown".to_string(),
            |s| s.to_string_lossy().into_owned(),
        );

        if dry_run {
            println!("  🔍 [Dry-run] Would queue for stripping ({strip_count} dubs)");
            jobs.push(SweepJob {
                path: entry,
                keep_indices: Vec::new(),
                strip_count,
                filename,
            });
            continue;
        }

        match ui::prompt_confirmation(&decisions, auto) {
            Ok(keep_indices) => {
                println!("  {} Queued for batch stripping.", "✔".green());
                jobs.push(SweepJob {
                    path: entry,
                    keep_indices,
                    strip_count,
                    filename,
                });
            }
            Err(_) => {
                println!("  {} Skipped by user.", "•".dimmed());
            }
        }
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
            }
            Err(err) => {
                eprintln!("    ⚠️ Failed to remux {}: {err}\n", job.filename);
            }
        }
    }
    (successful, total_saved)
}

fn handle_sweep(dir: &Path, auto: bool, dry_run: bool) -> Result<()> {
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
