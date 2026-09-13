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
                let norm = normalize_lang_code(code);
                return Ok(FilmOrigin {
                    title: title.to_string(),
                    year,
                    native_lang_code: norm.to_string(),
                    native_lang_name: name.to_string(),
                    source: format!("Gemini AI ({model})"),
                });
            }
            Err(e) => {
                last_err = format!("{model}: {e}");
            }
        }
    }

    anyhow::bail!("Gemini request failed: {last_err}")
}

fn build_prompt(title: &str, year: Option<u32>, streams: &[String], raw_filename: &str) -> String {
    format!(
        "You are an expert film researcher. Identify the single original theatrical language of this movie release:\n\
        - Movie Title: \"{title}\"\n\
        - Release Year: {year:?}\n\
        - Audio stream languages in file: {streams:?}\n\
        - Raw release name: \"{raw_filename}\"\n\
        Return strictly JSON with keys: \"language_code\" (3-letter ISO-639-2 e.g. kan, tel, tam, mal, hin, eng) and \"language_name\" (e.g. Kannada, Telugu, Hindi)."
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
