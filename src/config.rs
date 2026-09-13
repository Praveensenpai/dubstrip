use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DubstripConfig {
    pub gemini_api_key: Option<String>,
    pub gemini_model: Option<String>,
}

/// Returns the configuration path ~/.config/dubstrip/config.toml.
pub fn config_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    Ok(Path::new(&home).join(".config/dubstrip/config.toml"))
}

/// Loads the persistent configuration from ~/.config/dubstrip/config.toml.
#[must_use]
pub fn load_config() -> DubstripConfig {
    let Ok(path) = config_path() else {
        return DubstripConfig::default();
    };
    if !path.exists() {
        return DubstripConfig::default();
    }
    let Ok(content) = fs::read_to_string(&path) else {
        return DubstripConfig::default();
    };
    parse_toml_config(&content)
}

fn parse_toml_config(content: &str) -> DubstripConfig {
    let mut config = DubstripConfig::default();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("gemini_api_key") {
            let key_val = rest.trim_start_matches(|c: char| c == '=' || c.is_whitespace());
            let clean = key_val.trim_matches('"').trim_matches('\'').trim();
            if !clean.is_empty() {
                config.gemini_api_key = Some(clean.to_string());
            }
        }
        if let Some(rest) = trimmed.strip_prefix("gemini_model") {
            let key_val = rest.trim_start_matches(|c: char| c == '=' || c.is_whitespace());
            let clean = key_val.trim_matches('"').trim_matches('\'').trim();
            if !clean.is_empty() {
                config.gemini_model = Some(clean.to_string());
            }
        }
    }
    config
}

/// Saves the configuration to ~/.config/dubstrip/config.toml.
pub fn save_config(config: &DubstripConfig) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut content = String::from("# DubStrip Configuration\n");
    if let Some(key) = &config.gemini_api_key {
        content.push_str(&format!("gemini_api_key = \"{key}\"\n"));
    }
    if let Some(model) = &config.gemini_model {
        content.push_str(&format!("gemini_model = \"{model}\"\n"));
    }
    fs::write(&path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Resolves the configured or default Gemini model (defaults to gemini-3.5-flash).
#[must_use]
pub fn get_gemini_model() -> String {
    if let Ok(model) = std::env::var("GEMINI_MODEL") {
        let trimmed = model.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let cfg = load_config();
    if let Some(model) = cfg.gemini_model {
        let trimmed = model.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "gemini-3.6-flash".to_string()
}

/// Resolves Gemini API key with priority:
/// 1. GEMINI_API_KEY env var
/// 2. ~/.config/dubstrip/config.toml
/// 3. Inherited from ~/.config/ryoiki/telegram.toml
/// 4. Interactive prompt (if interactive terminal)
pub fn get_or_prompt_gemini_key(interactive: bool) -> Option<String> {
    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let cfg = load_config();
    if let Some(key) = cfg.gemini_api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Some(key) = check_ryoiki_inherited_key() {
        return Some(key);
    }

    if !interactive || !io::stdin().is_terminal() {
        return None;
    }

    prompt_and_save_key().ok().flatten()
}

fn check_ryoiki_inherited_key() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let ryoiki_cfg = Path::new(&home).join(".config/ryoiki/telegram.toml");
    if ryoiki_cfg.exists() {
        if let Ok(content) = fs::read_to_string(&ryoiki_cfg) {
            let parsed = parse_toml_config(&content);
            if let Some(key) = parsed.gemini_api_key {
                return Some(key);
            }
        }
    }
    None
}

fn prompt_and_save_key() -> Result<Option<String>> {
    println!();
    println!(
        "  {} {}",
        "🤖 Gemini AI:".cyan().bold(),
        "Enhances movie theatrical origin disambiguation.".dimmed()
    );
    println!(
        "     Get a free key at: {}",
        "https://aistudio.google.com/".underline().cyan()
    );
    print!("  Enter Gemini API Key [press Enter to skip]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let key = trimmed.to_string();
    let mut config = load_config();
    config.gemini_api_key = Some(key.clone());
    save_config(&config)?;
    println!(
        "  {} Saved Gemini API key to ~/.config/dubstrip/config.toml\n",
        "✔".green().bold()
    );
    Ok(Some(key))
}
