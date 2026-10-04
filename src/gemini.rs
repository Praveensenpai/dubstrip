use crate::ai::{normalize_lang_code, FilmOrigin};
use anyhow::Result;
use reqwest::blocking::Client;
use serde_json::json;
use std::thread;
use std::time::Duration;

const RETRY_DELAYS: &[u64] = &[1, 2, 5, 10, 15, 30, 60];

pub fn query_ai_film_origin(
    title: &str,
    year: Option<u32>,
    streams: &[String],
    raw_filename: &str,
    gemini_api_key: Option<&str>,
) -> Result<FilmOrigin> {
    if crate::config::is_deepseek_enabled() {
        match query_deepseek_film_origin(title, year, streams, raw_filename) {
            Ok(origin) => return Ok(origin),
            Err(err) => {
                eprintln!("  ℹ DeepSeek primary attempt failed ({err}); falling back to Gemini...");
            }
        }
    }

    if let Some(key) = gemini_api_key {
        return query_gemini_film_origin(title, year, streams, raw_filename, key);
    }

    anyhow::bail!("No AI provider available (DeepSeek failed and no Gemini API key configured)")
}

pub fn query_deepseek_film_origin(
    title: &str,
    year: Option<u32>,
    streams: &[String],
    raw_filename: &str,
) -> Result<FilmOrigin> {
    let url = crate::config::get_deepseek_url();
    let model = crate::config::get_deepseek_model();
    let api_key = crate::config::get_deepseek_key();
    let prompt = build_prompt(title, year, streams, raw_filename);

    let client = Client::builder().timeout(Duration::from_secs(12)).build()?;
    let payload = json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ],
        "temperature": 0.1
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()?;

    let status = resp.status();
    if !status.is_success() {
        let err_text = resp.text().unwrap_or_default();
        anyhow::bail!("DeepSeek API returned HTTP {status}: {err_text}");
    }

    let body: serde_json::Value = resp.json()?;
    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing message content in DeepSeek response"))?;

    let cleaned = clean_json_text(content);
    let origin_data: serde_json::Value = serde_json::from_str(&cleaned)
        .map_err(|e| anyhow::anyhow!("Failed to parse DeepSeek JSON ({e}): {cleaned}"))?;

    let code = origin_data["language_code"].as_str().unwrap_or("und");
    let name = origin_data["language_name"].as_str().unwrap_or("Unknown");
    let confidence = origin_data["confidence"].as_u64().unwrap_or(80).min(100) as u8;
    let norm = normalize_lang_code(code);

    Ok(FilmOrigin {
        title: title.to_string(),
        year,
        native_lang_code: norm.to_string(),
        native_lang_name: name.to_string(),
        source: format!("DeepSeek ({model})"),
        confidence,
    })
}

fn clean_json_text(raw: &str) -> String {
    let mut s = raw.trim();
    if s.starts_with("```json") {
        s = &s[7..];
    } else if s.starts_with("```") {
        s = &s[3..];
    }
    if s.ends_with("```") {
        s = &s[..s.len() - 3];
    }
    s.trim().to_string()
}

pub fn query_gemini_film_origin(
    title: &str,
    year: Option<u32>,
    streams: &[String],
    raw_filename: &str,
    api_key: &str,
) -> Result<FilmOrigin> {
    let client = Client::builder().timeout(Duration::from_secs(20)).build()?;
    let preferred = crate::config::get_gemini_model();
    let candidates = [
        preferred.as_str(),
        "gemini-3.1-flash-lite",
        "gemini-3.8-flash",
    ];

    let prompt = build_prompt(title, year, streams, raw_filename);
    let body = json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": { "response_mime_type": "application/json" }
    });

    let mut last_err = String::new();
    for &model in &candidates {
        match execute_with_retry(&client, model, api_key, &body) {
            Ok(origin_data) => {
                let code = origin_data["language_code"].as_str().unwrap_or("und");
                let name = origin_data["language_name"].as_str().unwrap_or("Unknown");
                let confidence = origin_data["confidence"].as_u64().unwrap_or(70).min(100) as u8;
                let norm = normalize_lang_code(code);
                return Ok(FilmOrigin {
                    title: title.to_string(),
                    year,
                    native_lang_code: norm.to_string(),
                    native_lang_name: name.to_string(),
                    source: format!("Gemini AI ({model})"),
                    confidence,
                });
            }
            Err(e) => {
                last_err = format!("{model}: {e}");
            }
        }
    }

    anyhow::bail!("Gemini request failed: {last_err}")
}

fn sanitize_filename_for_prompt(raw: &str) -> String {
    let re_domains = regex::Regex::new(
        r"(?i)www\.[a-z0-9\.\-]+\s*-\s*|\[[a-z0-9\.\-]+\]\s*|\d*tamilmv[\.\w\-]*|tamilblasters[\.\w\-]*|tamilrockers[\.\w\-]*",
    )
    .unwrap_or_else(|_| regex::Regex::new("$^").expect("fallback"));
    let re_lang_tags = regex::Regex::new(r"(?i)\[(?:Tam|Tel|Hin|Mal|Kan|Eng|Audio|\+|,|\s|-)+\]")
        .unwrap_or_else(|_| regex::Regex::new("$^").expect("fallback"));
    let s = re_domains.replace_all(raw, "");
    let s = re_lang_tags.replace_all(&s, "");
    s.trim().to_string()
}

fn build_prompt(title: &str, year: Option<u32>, streams: &[String], raw_filename: &str) -> String {
    let clean_raw = sanitize_filename_for_prompt(raw_filename);
    let year_display = year.map_or_else(|| "Unknown".to_string(), |y| y.to_string());
    format!(
        "You are an expert film researcher. Identify the single original theatrical production language of this movie release:\n\
        - Movie Title: \"{title}\"\n\
        - Release Year: {year_display}\n\
        - Audio stream tracks in file: {streams:?}\n\
        - Cleaned release name: \"{clean_raw}\"\n\
        CRITICAL RULES:\n\
        1. Identify the authentic primary production language (e.g. Kannada for Sandalwood / Kichcha Sudeepa films, Tamil for Kollywood, Telugu for Tollywood, Hindi for Bollywood, Malayalam for Mollywood, English for Hollywood, Japanese for Anime).\n\
        2. ANTI-BIAS WARNING: Do NOT assume Track 1 or torrent release ordering indicates the native language. Piracy groups frequently re-order tracks to place Tamil or Hindi first even for Kannada or Malayalam movies. Base your decision solely on the movie's production industry, cast, and director.\n\
        3. Set 'confidence' (0 to 100). If you are uncertain or the title could be an ambiguous remake/dub, set confidence below 70.\n\
        Return strictly JSON with keys:\n\
        - \"language_code\": 3-letter ISO-639-2 (e.g. tam, tel, hin, mal, kan, eng, jpn)\n\
        - \"language_name\": capitalized name (e.g. Tamil, Telugu, Hindi, Malayalam, Kannada, English)\n\
        - \"confidence\": integer between 0 and 100\n\
        - \"reason\": brief explanation"
    )
}

fn execute_with_retry(
    client: &Client,
    model: &str,
    api_key: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}"
    );

    let mut attempt = 0;
    loop {
        match client.post(&url).json(body).send() {
            Ok(resp) if resp.status().is_success() => {
                let text = resp.text()?;
                return parse_candidate_json(&text);
            }
            Ok(resp) => {
                let status = resp.status();
                if is_retryable_status(status.as_u16()) && attempt < RETRY_DELAYS.len() {
                    let delay = RETRY_DELAYS[attempt];
                    attempt += 1;
                    eprintln!(
                        "  ⏳ Gemini API ({model}) returned HTTP {status}. Retrying in {delay}s (attempt {attempt}/{})...",
                        RETRY_DELAYS.len()
                    );
                    thread::sleep(Duration::from_secs(delay));
                    continue;
                }
                anyhow::bail!("HTTP status: {status}");
            }
            Err(err) => {
                if attempt < RETRY_DELAYS.len() {
                    let delay = RETRY_DELAYS[attempt];
                    attempt += 1;
                    eprintln!(
                        "  ⏳ Gemini network error. Retrying in {delay}s (attempt {attempt}/{})...",
                        RETRY_DELAYS.len()
                    );
                    thread::sleep(Duration::from_secs(delay));
                    continue;
                }
                anyhow::bail!("Network error: {err}");
            }
        }
    }
}

fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn parse_candidate_json(text: &str) -> Result<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(text)?;
    let candidate_text = v["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("{}");
    let parsed: serde_json::Value = serde_json::from_str(candidate_text)?;
    Ok(parsed)
}
