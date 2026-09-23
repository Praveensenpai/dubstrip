use crate::ai::{normalize_lang_code, FilmOrigin};
use anyhow::Result;
use reqwest::blocking::Client;
use serde_json::json;
use std::thread;
use std::time::Duration;

const RETRY_DELAYS: &[u64] = &[1, 2, 5, 10, 15, 30, 60];

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
        "gemini-3.6-flash",
        "gemini-flash-latest",
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
    let re = regex::Regex::new(r"(?i)www\.\d*(tamilmv|tamilblasters|tamilrockers|desiscenics|1tamilmv)\.[a-z]+|1tamilmv|tamilblasters|tamilrockers").unwrap_or_else(|_| regex::Regex::new("$^").expect("fallback"));
    re.replace_all(raw, "").trim().to_string()
}

fn build_prompt(title: &str, year: Option<u32>, streams: &[String], raw_filename: &str) -> String {
    let clean_raw = sanitize_filename_for_prompt(raw_filename);
    let year_display = year.map_or_else(|| "Unknown".to_string(), |y| y.to_string());
    format!(
        "You are an expert film researcher. Identify the single original theatrical production language of this movie release:\n\
        - Movie Title: \"{title}\"\n\
        - Release Year: {year_display}\n\
        - Audio stream tracks in file: {streams:?}\n\
        - Raw release name: \"{clean_raw}\"\n\
        CRITICAL RULES:\n\
        1. Identify the authentic primary production language (e.g. Tamil for Kollywood, Telugu for Tollywood, Hindi for Bollywood, Malayalam for Mollywood, Kannada for Sandalwood, English for Hollywood, Japanese for Anime).\n\
        2. Set 'confidence' (0 to 100). If you are uncertain or the title could be an ambiguous remake/dub, set confidence below 70.\n\
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
