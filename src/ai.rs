use anyhow::Result;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilmOrigin {
    pub title: String,
    pub year: Option<u32>,
    pub native_lang_code: String,
    pub native_lang_name: String,
    pub source: String,
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

    if let Some(origin) = search_local_jellyfin_cache(&raw_title, year, stream_langs) {
        return Ok(origin);
    }
    if let Some(origin) = query_wikipedia_film_origin(&raw_title, year, stream_langs) {
        return Ok(origin);
    }
    if let Some(origin) = infer_origin_from_context(&raw_title, year) {
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
    })
}

fn search_local_jellyfin_cache(
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
        },
    ))
}

fn map_wikidata_qid(qid: &str) -> Option<(&'static str, &'static str)> {
    match qid {
        "Q1568" => Some(("Hindi", "hin")),
        "Q36186" => Some(("Kannada", "kan")),
        "Q5885" => Some(("Tamil", "tam")),
        "Q8097" => Some(("Telugu", "tel")),
        "Q36236" => Some(("Malayalam", "mal")),
        "Q1860" => Some(("English", "eng")),
        "Q5287" => Some(("Japanese", "jpn")),
        "Q7026" => Some(("Korean", "kor")),
        "Q1362" => Some(("Spanish", "spa")),
        "Q150" => Some(("French", "fre")),
        "Q7737" => Some(("Russian", "rus")),
        "Q188" => Some(("German", "ger")),
        "Q652" => Some(("Italian", "ita")),
        "Q9610" => Some(("Bengali", "ben")),
        "Q1571" => Some(("Marathi", "mar")),
        "Q5146" => Some(("Portuguese", "por")),
        _ => None,
    }
}

fn query_wikidata_lang(
    client: &reqwest::blocking::Client,
    qid: &str,
) -> Option<(&'static str, &'static str)> {
    let url = format!("https://www.wikidata.org/wiki/Special:EntityData/{qid}.json");
    let resp: serde_json::Value = client.get(&url).send().ok()?.json().ok()?;
    let claims = &resp["entities"][qid]["claims"];
    let p364 = claims.get("P364")?.as_array()?;
    let lang_qid = p364.first()?["mainsnak"]["datavalue"]["value"]["id"].as_str()?;
    map_wikidata_qid(lang_qid)
}

fn query_wikipedia_film_origin(
    title: &str,
    year: Option<u32>,
    stream_langs: &[String],
) -> Option<FilmOrigin> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(4))
        .user_agent("dubstrip/1.0")
        .build()
        .ok()?;

    let queries = [
        year.map(|y| format!("{title} {y} film")),
        year.map(|y| format!("{title} {} film", y.saturating_sub(1))),
        Some(format!("{title} film")),
        Some(title.to_string()),
    ];

    for query in queries.into_iter().flatten() {
        let search_url = format!(
            "https://en.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&format=json",
            query.replace(' ', "+")
        );

        let Ok(resp) = client.get(&search_url).send() else {
            continue;
        };
        let Ok(json) = resp.json::<serde_json::Value>() else {
            continue;
        };
        let Some(results) = json["query"]["search"].as_array() else {
            continue;
        };

        for item in results.iter().take(3) {
            let Some(page_title) = item["title"].as_str() else {
                continue;
            };
            let summary_url = format!(
                "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
                page_title.replace(' ', "_")
            );

            let Ok(s_resp) = client.get(&summary_url).send() else {
                continue;
            };
            let Ok(s_json) = s_resp.json::<serde_json::Value>() else {
                continue;
            };

            let mut detected: Option<(&'static str, &'static str)> = None;

            // 1. Try Wikidata P364 (Structured Property for Original Language)
            if let Some(qid) = s_json["wikibase_item"].as_str() {
                detected = query_wikidata_lang(&client, qid);
            }

            // 2. Fallback to extract text scanning if Wikidata claim missing
            if detected.is_none() {
                let extract = s_json["extract"].as_str().unwrap_or("");
                let langs = [
                    ("Kannada", "kan"),
                    ("Hindi", "hin"),
                    ("Tamil", "tam"),
                    ("Telugu", "tel"),
                    ("Malayalam", "mal"),
                    ("English", "eng"),
                ];
                for (name, code) in langs {
                    if extract.contains(&format!("{name}-language"))
                        || extract.contains(&format!("{name} language"))
                    {
                        detected = Some((name, code));
                        break;
                    }
                }
            }

            if let Some((lang_name, lang_code)) = detected {
                let stream_matched = stream_langs.is_empty()
                    || stream_langs.iter().any(|s| {
                        let norm = normalize_lang_code(s);
                        norm == lang_code || s.eq_ignore_ascii_case(lang_code)
                    });

                if stream_matched {
                    return Some(FilmOrigin {
                        title: title.to_string(),
                        year,
                        native_lang_code: lang_code.to_string(),
                        native_lang_name: lang_name.to_string(),
                        source: "Wikidata P364 / Wikipedia API".to_string(),
                    });
                }
            }
        }
    }

    None
}

fn infer_origin_from_context(_title: &str, _year: Option<u32>) -> Option<FilmOrigin> {
    None
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
        return Some(FilmOrigin {
            title: title.to_string(),
            year,
            native_lang_code: first.to_string(),
            native_lang_name: streams[0].clone(),
            source: "Container Stream Consensus".to_string(),
        });
    }
    None
}

pub fn parse_title_and_year(path: &Path) -> (String, Option<u32>) {
    let filename = path.file_stem().map_or_else(
        || "Unknown".to_string(),
        |s| s.to_string_lossy().into_owned(),
    );

    let cleaned = filename
        .replace("[1TamilMV.day]", "")
        .replace("www.1TamilMV.day -", "")
        .replace("www.1TamilMV.day", "")
        .replace("www.1TamilMV.pink -", "")
        .replace("www.1TamilMV.pink", "")
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
