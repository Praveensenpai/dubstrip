mod ai;
mod config;
mod decide;
mod gemini;
mod notify;
mod probe;
mod remux;
mod safety;
mod sweep;
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
        } => sweep::handle_sweep(&path, auto, dry_run),
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
        let origin_desc = format!(
            "{} [{}% confident] (via {})",
            origin.native_lang_name, origin.confidence, origin.source
        );
        if !origin.is_confident() {
            println!(
                "\n  {} {} preserved with all audio tracks (origin confidence {}% < 80% — kept Multi).\n",
                "✔".green().bold(),
                filename.cyan(),
                origin.confidence
            );
            notify::notify_preserved_multi(
                &origin.title,
                &origin_desc,
                &decisions,
                media.size_bytes,
            );
        } else {
            println!(
                "\n  {} {} is already clean (only native {} audio present).\n",
                "✔".green().bold(),
                filename.cyan(),
                origin.native_lang_name.bold()
            );
        }
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

    let origin_desc = format!(
        "{} [{}% confident] (via {})",
        origin.native_lang_name, origin.confidence, origin.source
    );
    notify::notify_strip(
        &origin.title,
        &origin_desc,
        &decisions,
        media.size_bytes,
        saved,
    );

    Ok(())
}
