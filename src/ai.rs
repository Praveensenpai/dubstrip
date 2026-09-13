use anyhow::Result;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilmOrigin {
    pub title: String,
    pub year: Option<u32>,
    pub native_lang_code: String,
    pub native_lang_name: String,
    pub source: String,
}

/// Normalizes 2-letter or 3-letter language codes to standard 3-letter ISO-639-2.
#[must_use]
pub fn normalize_lang_code(code: &str) -> &'static str {
    let lower = code.trim().to_lowercase();
    match lower.as_str() {
        "kn" | "kan" | "kannada" => "kan",
        "te" | "tel" | "telugu" => "tel",
        "ta" | "tam" | "tamil" => "tam",
        "ml" | "mal" | "malayalam" => "mal",
        "hi" | "hin" | "hindi" => "hin",
        "en" | "eng" | "english" => "eng",
        "ja" | "jpn" | "japanese" => "jpn",
        "ko" | "kor" | "korean" => "kor",
        "fr" | "fra" | "fre" | "french" => "fre",
        "es" | "spa" | "spanish" => "spa",
        "ru" | "rus" | "russian" => "rus",
        "de" | "deu" | "ger" | "german" => "ger",
        "it" | "ita" | "italian" => "ita",
        "zh" | "zho" | "chi" | "chinese" => "chi",
        _ => "und",
    }
}

/// Resolves the movie's native theatrical origin language.
/// Uses 3 tiers: Local Jellyfin cache -> Release filename context -> Fallback.
pub fn resolve_film_origin(path: &Path) -> Result<FilmOrigin> {
    let (raw_title, year) = parse_title_and_year(path);

    // Tier 1: Check local Jellyfin metadata cache (~/jellyfin/cache/omdb/)
    if let Some(origin) = search_local_jellyfin_cache(&raw_title, year) {
        return Ok(origin);
    }

    // Tier 2: Check context from release groups & known titles
    if let Some(origin) = infer_origin_from_context(&raw_title, year) {
        return Ok(origin);
    }

    // Tier 3: Default fallback
    Ok(FilmOrigin {
        title: raw_title,
        year,
        native_lang_code: "und".to_string(),
        native_lang_name: "Unknown / Unresolved".to_string(),
        source: "Unresolved".to_string(),
    })
}

/// Parses a clean movie title and optional release year from path.
pub fn parse_title_and_year(path: &Path) -> (String, Option<u32>) {
    let filename = path.file_stem().map_or_else(
        || "Unknown".to_string(),
        |s| s.to_string_lossy().into_owned(),
    );

    // Clean common release junk: [1TamilMV.day], www.1TamilMV.day -, etc.
    let cleaned = filename
        .replace("[1TamilMV.day]", "")
        .replace("www.1TamilMV.day -", "")
        .replace("www.1TamilMV.day", "")
        .replace("1TamilMV", "")
        .replace("TamilBlasters", "");

    let re_year =
        Regex::new(r"\((\d{4})\)").unwrap_or_else(|_| Regex::new("$^").expect("fallback"));
    let year = re_year
        .captures(&cleaned)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok());

    let title_part = if let Some(idx) = cleaned.find('(') {
        cleaned[..idx].trim().to_string()
    } else if let Some(idx) = cleaned.find('[') {
        cleaned[..idx].trim().to_string()
    } else {
        cleaned.trim().to_string()
    };

    let title = title_part
        .trim_matches(|c: char| c == '-' || c == '_' || c == '.' || c.is_whitespace())
        .to_string();

    (title, year)
}

/// Searches local Jellyfin OMDb cache files for matching title and year.
fn search_local_jellyfin_cache(title: &str, year: Option<u32>) -> Option<FilmOrigin> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let cache_dir = PathBuf::from(home).join("jellyfin/cache/omdb");

    if !cache_dir.exists() {
        return None;
    }

    let entries = fs::read_dir(cache_dir).ok()?.flatten();
    for entry in entries {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    let cached_title = json.get("Title").and_then(|v| v.as_str())?;
                    let cached_year = json.get("Year").and_then(|v| v.as_str());

                    let title_matches = cached_title.eq_ignore_ascii_case(title);
                    let year_matches = match (year, cached_year) {
                        (Some(y), Some(cy)) => cy.contains(&y.to_string()),
                        _ => true,
                    };

                    if title_matches && year_matches {
                        if let Some(lang_str) = json.get("Language").and_then(|v| v.as_str()) {
                            let first_lang = lang_str.split(',').next()?.trim();
                            let norm = normalize_lang_code(first_lang);
                            return Some(FilmOrigin {
                                title: cached_title.to_string(),
                                year,
                                native_lang_code: norm.to_string(),
                                native_lang_name: first_lang.to_string(),
                                source: "Jellyfin OMDb Cache".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    None
}

/// Contextual and film industry disambiguation for known titles.
fn infer_origin_from_context(title: &str, year: Option<u32>) -> Option<FilmOrigin> {
    let lower = title.to_lowercase();

    // Specific Indian cinema disambiguation overrides (where TMDB mislabels)
    if lower == "45" {
        return Some(FilmOrigin {
            title: "45".to_string(),
            year,
            native_lang_code: "kan".to_string(),
            native_lang_name: "Kannada".to_string(),
            source: "Context Resolver".to_string(),
        });
    }

    if lower.contains("they call him og") || lower == "og" {
        return Some(FilmOrigin {
            title: "They Call Him OG".to_string(),
            year,
            native_lang_code: "tel".to_string(),
            native_lang_name: "Telugu".to_string(),
            source: "Context Resolver".to_string(),
        });
    }

    if lower.contains("chhaava") {
        return Some(FilmOrigin {
            title: "Chhaava".to_string(),
            year,
            native_lang_code: "hin".to_string(),
            native_lang_name: "Hindi".to_string(),
            source: "Context Resolver".to_string(),
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_lang_code() {
        assert_eq!(normalize_lang_code("kn"), "kan");
        assert_eq!(normalize_lang_code("kannada"), "kan");
        assert_eq!(normalize_lang_code("tel"), "tel");
        assert_eq!(normalize_lang_code("hindi"), "hin");
        assert_eq!(normalize_lang_code("en"), "eng");
    }

    #[test]
    fn test_parse_title_and_year() {
        let p1 = Path::new("/movies/They Call Him OG (2025) [1080p].mkv");
        let (t1, y1) = parse_title_and_year(p1);
        assert_eq!(t1, "They Call Him OG");
        assert_eq!(y1, Some(2025));

        let p2 = Path::new("/torrents/www.1TamilMV.day - 45 (2025) [DDP5.1].mkv");
        let (t2, y2) = parse_title_and_year(p2);
        assert_eq!(t2, "45");
        assert_eq!(y2, Some(2025));
    }
}
