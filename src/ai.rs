pub mod omdb;
pub mod wikipedia;

use anyhow::Result;
use regex::Regex;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilmOrigin {
    pub title: String,
    pub year: Option<u32>,
    pub native_lang_code: String,
    pub native_lang_name: String,
    pub source: String,
    pub confidence: u8,
}

impl FilmOrigin {
    #[must_use]
    pub fn is_confident(&self) -> bool {
        self.confidence >= 80 && self.native_lang_code != "und"
    }
}

#[must_use]
pub fn normalize_lang_code(code: &str) -> &'static str {
    match code.trim().to_lowercase().as_str() {
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
        "pl" | "pol" | "polish" => "pol",
        _ => "und",
    }
}

pub fn resolve_film_origin(
    path: &Path,
    stream_langs: &[String],
    interactive: bool,
) -> Result<FilmOrigin> {
    let (raw_title, year) = parse_title_and_year(path);

    if let Some(origin) = omdb::search_local_jellyfin_cache(&raw_title, year, stream_langs) {
        return Ok(origin);
    }
    if let Some(origin) = wikipedia::query_wikipedia_film_origin(&raw_title, year, stream_langs) {
        return Ok(origin);
    }

    let raw_name = path
        .file_name()
        .map_or_else(|| raw_title.clone(), |s| s.to_string_lossy().into_owned());

    if let Some(api_key) = crate::config::get_or_prompt_gemini_key(interactive) {
        if let Ok(origin) = crate::gemini::query_gemini_film_origin(
            &raw_title,
            year,
            stream_langs,
            &raw_name,
            &api_key,
        ) {
            return Ok(origin);
        }
    }

    if let Some(origin) = infer_origin_from_streams(&raw_title, year, stream_langs) {
        return Ok(origin);
    }

    Ok(FilmOrigin {
        title: raw_title,
        year,
        native_lang_code: "und".to_string(),
        native_lang_name: "Unknown / Unresolved".to_string(),
        source: "Unresolved".to_string(),
        confidence: 0,
    })
}

fn infer_origin_from_streams(
    title: &str,
    year: Option<u32>,
    streams: &[String],
) -> Option<FilmOrigin> {
    if streams.is_empty() {
        return None;
    }
    let first = normalize_lang_code(&streams[0]);
    if first == "und" {
        return None;
    }
    if streams.iter().all(|s| normalize_lang_code(s) == first) {
        Some(FilmOrigin {
            title: title.to_string(),
            year,
            native_lang_code: first.to_string(),
            native_lang_name: streams[0].clone(),
            source: "Container Stream Consensus".to_string(),
            confidence: 90,
        })
    } else {
        None
    }
}

pub fn parse_title_and_year(path: &Path) -> (String, Option<u32>) {
    let filename = path.file_stem().map_or_else(
        || "Unknown".to_string(),
        |s| s.to_string_lossy().into_owned(),
    );

    let re_tracker = Regex::new(
        r"(?i)^(www\.[a-z0-9\.\-]+\s*-\s*|\[[a-z0-9\.\-]+\]\s*|\d*tamilmv[\.\w\-]*\s*-\s*|tamilblasters[\.\w\-]*\s*-\s*)",
    )
    .unwrap_or_else(|_| Regex::new("$^").expect("fallback"));
    let cleaned = re_tracker.replace(&filename, "");

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

    #[test]
    fn test_film_origin_confidence() {
        let confident = FilmOrigin {
            title: "Test".to_string(),
            year: Some(2026),
            native_lang_code: "tam".to_string(),
            native_lang_name: "Tamil".to_string(),
            source: "Test".to_string(),
            confidence: 85,
        };
        assert!(confident.is_confident());

        let unconfident = FilmOrigin {
            title: "Test".to_string(),
            year: Some(2026),
            native_lang_code: "tam".to_string(),
            native_lang_name: "Tamil".to_string(),
            source: "Test".to_string(),
            confidence: 65,
        };
        assert!(!unconfident.is_confident());
    }

    #[test]
    fn test_parse_title_and_year_mark() {
        let p = Path::new("/torrents/www.1TamilMV.haus - Mark (2026) TRUE WEB-DL - 1080p - AVC - [Tam + Tel + Mal + Kan] - (DD+5.1 - 192Kbps & AAC) - 4.2GB - ESub.mkv");
        let (title, year) = parse_title_and_year(p);
        assert_eq!(title, "Mark");
        assert_eq!(year, Some(2026));
    }
}
