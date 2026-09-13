mod ai;
mod config;
mod decide;
mod gemini;
mod notify;
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
    /// Configure Google Gemini API key and persistent settings
    Config {
        /// Store Gemini API key persistently in ~/.config/dubstrip/config.toml
        #[arg(long)]
        set_key: Option<String>,

        /// Preferred Gemini model (defaults to gemini-3.6-flash)
        #[arg(long)]
        set_model: Option<String>,
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
        Commands::Config { set_key, set_model } => handle_config(set_key, set_model),
    }
}

fn handle_config(set_key: Option<String>, set_model: Option<String>) -> Result<()> {
    if set_key.is_some() || set_model.is_some() {
        let mut cfg = config::load_config();
        if let Some(key) = set_key {
            cfg.gemini_api_key = Some(key.trim().to_string());
        }
        if let Some(model) = set_model {
            cfg.gemini_model = Some(model.trim().to_string());
        }
        config::save_config(&cfg)?;
        println!(
            "\n  {} Saved configuration to ~/.config/dubstrip/config.toml\n",
            "✔".green().bold()
        );
        return Ok(());
    }

    println!("\n  {} DubStrip Configuration Status", "⚙️".cyan().bold());
    println!("  {}", "─".repeat(45).dimmed());
    let active_key = config::get_or_prompt_gemini_key(false);
    if let Some(key) = active_key {
        let masked = if key.len() > 8 {
            format!("{}...{}", &key[..4], &key[key.len() - 4..])
        } else {
            "****".to_string()
        };
        println!("  • Gemini API Key: {}", masked.green());
    } else {
        println!(
            "  • Gemini API Key: {}",
            "Not set (local heuristics only)".dimmed()
        );
    }
    let model = config::get_gemini_model();
    println!("  • Gemini Model:   {}", model.cyan());
    println!();
    Ok(())
}

fn handle_inspect(path: &Path) -> Result<()> {
    let media = probe::probe_file(path)?;
    let stream_langs: Vec<String> = media
        .audio_streams
        .iter()
        .map(|s| s.language.clone())
        .collect();
    let origin = ai::resolve_film_origin(path, &stream_langs, true)?;
    let decisions = decide::evaluate_audio_streams(&media, &origin);

    ui::render_inspection_table(&media, &origin, &decisions);
    Ok(())
}

fn handle_strip(path: &Path, auto: bool, dry_run: bool, force: bool) -> Result<()> {
    if !safety::is_file_safe_and_complete(path, force)? {
        println!(
            "  {} Skipping {}: File is downloading, active, or incomplete.",
            "⏳".yellow(),
            path.display().to_string().cyan()
        );
        return Ok(());
    }

    let media = probe::probe_file(path)?;
    let stream_langs: Vec<String> = media
        .audio_streams
        .iter()
        .map(|s| s.language.clone())
        .collect();
    let origin = ai::resolve_film_origin(path, &stream_langs, !auto)?;
    let decisions = decide::evaluate_audio_streams(&media, &origin);

    ui::render_inspection_table(&media, &origin, &decisions);

    let strip_count = decisions
        .iter()
        .filter(|d| d.action == ui::Action::Strip)
        .count();

    if strip_count == 0 {
        let filename = path
            .file_name()
            .map_or_else(|| "File".to_string(), |s| s.to_string_lossy().into_owned());
        println!(
            "\n  {} {} is already clean (only native {} audio present).\n",
            "✔".green().bold(),
            filename.cyan(),
            origin.native_lang_name.bold()
        );
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

    let origin_desc = format!("{} (via {})", origin.native_lang_name, origin.source);
    notify::notify_strip(
        &origin.title,
        &origin_desc,
        &decisions,
        media.size_bytes,
        saved,
    );

    Ok(())
}

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
                "  {} {} is already clean ({})",
                "✔".green(),
                filename.dimmed(),
                origin.native_lang_name.dimmed()
            );
            continue;
        }

        ui::render_inspection_table(&media, &origin, &decisions);

        let origin_desc = format!("{} (via {})", origin.native_lang_name, origin.source);
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
