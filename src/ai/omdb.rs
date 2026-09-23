use super::{normalize_lang_code, FilmOrigin};
use std::fs;
use std::path::PathBuf;

/// Searches the local Jellyfin OMDb cache for a matching film metadata entry.
pub fn search_local_jellyfin_cache(
    title: &str,
    year: Option<u32>,
    stream_langs: &[String],
) -> Option<FilmOrigin> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let cache_dir = PathBuf::from(home).join("jellyfin/cache/omdb");
    if !cache_dir.exists() {
        return None;
    }

    let entries = fs::read_dir(cache_dir).ok()?.flatten();
    let mut best_match: Option<(i32, FilmOrigin)> = None;

    for entry in entries {
        let p = entry.path();
        if p.extension().is_some_and(|ext| ext == "json") {
            if let Ok(content) = fs::read_to_string(&p) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some((score, origin)) = score_candidate(&json, title, year, stream_langs)
                    {
                        if score > best_match.as_ref().map_or(-999, |(s, _)| *s) {
                            best_match = Some((score, origin));
                        }
                    }
                }
            }
        }
    }

    best_match.and_then(|(score, origin)| if score > 0 { Some(origin) } else { None })
}

fn score_candidate(
    json: &serde_json::Value,
    title: &str,
    year: Option<u32>,
    stream_langs: &[String],
) -> Option<(i32, FilmOrigin)> {
    let cached_title = json.get("Title").and_then(|v| v.as_str())?;
    let cached_year = json.get("Year").and_then(|v| v.as_str());

    if !cached_title.eq_ignore_ascii_case(title) {
        return None;
    }

    let year_matches = match (year, cached_year) {
        (Some(y), Some(cy)) => cy.contains(&y.to_string()),
        _ => true,
    };
    if !year_matches {
        return None;
    }

    let lang_str = json.get("Language").and_then(|v| v.as_str())?;
    let first_lang = lang_str.split(',').next()?.trim();
    let norm = normalize_lang_code(first_lang);

    let mut score = 10;
    let country = json.get("Country").and_then(|v| v.as_str()).unwrap_or("");

    let lang_in_streams = stream_langs.iter().any(|s| {
        let normalized = normalize_lang_code(s);
        normalized == norm || s.eq_ignore_ascii_case(norm)
    });

    if lang_in_streams {
        score += 100;
    } else if !stream_langs.is_empty() {
        score -= 100;
    }

    if country.contains("India") {
        score += 20;
    }

    Some((
        score,
        FilmOrigin {
            title: cached_title.to_string(),
            year,
            native_lang_code: norm.to_string(),
            native_lang_name: first_lang.to_string(),
            source: format!("Jellyfin OMDb Cache ({country})"),
            confidence: 100,
        },
    ))
}
