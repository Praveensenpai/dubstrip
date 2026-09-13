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

fn handle_sweep(dir: &Path, auto: bool, dry_run: bool) -> Result<()> {
    println!(
        "\n  {} Sweeping directory: {}",
        "🔍".cyan().bold(),
        dir.display().to_string().bold()
    );

    let mut total_saved = 0u64;
    let mut processed = 0usize;

    for entry in walk_video_files(dir) {
        if let Ok(media) = probe::probe_file(&entry) {
            let origin = ai::resolve_film_origin(&entry)?;
            let decisions = decide::evaluate_audio_streams(&media, &origin);
            let strip_count = decisions
                .iter()
                .filter(|d| d.action == ui::Action::Strip)
                .count();

            if strip_count > 0 {
                ui::render_inspection_table(&media, &origin, &decisions);
                if !dry_run {
                    if let Ok(keep_indices) = ui::prompt_confirmation(&decisions, auto) {
                        if let Ok(saved) = remux::remux_lossless(&entry, &keep_indices) {
                            total_saved += saved;
                            processed += 1;
                        }
                    }
                } else {
                    processed += 1;
                }
            }
        }
    }

    if dry_run {
        println!(
            "\n  {} [Dry-run] Found {} files with redundant dub tracks.",
            "✔".green(),
            processed.to_string().bold()
        );
    } else {
        println!(
            "\n  {} Swept {} files. Total space reclaimed: {}\n",
            "✔".green().bold(),
            processed.to_string().bold(),
            remux::format_bytes(total_saved).green().bold()
        );
    }

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
