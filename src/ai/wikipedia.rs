use super::{normalize_lang_code, FilmOrigin};
use std::time::Duration;

/// Queries Wikipedia and Wikidata P364 property to identify the original theatrical language.
pub fn query_wikipedia_film_origin(
    title: &str,
    year: Option<u32>,
    stream_langs: &[String],
) -> Option<FilmOrigin> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("dubstrip/1.0 (https://github.com/Praveensenpai/dubstrip)")
        .build()
        .ok()?;

    let mut title_variants = vec![title.to_string()];
    if title.contains(" and ") {
        title_variants.push(title.replace(" and ", " & "));
    }
    if title.contains(" & ") {
        title_variants.push(title.replace(" & ", " and "));
    }

    let mut queries = Vec::new();
    for t in &title_variants {
        if let Some(y) = year {
            queries.push(format!("{t} {y} film"));
            queries.push(format!("{t} {} film", y.saturating_sub(1)));
        }
        queries.push(format!("{t} film"));
        queries.push(t.clone());
    }

    for query in queries {
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
                        confidence: 100,
                    });
                }
            }
        }
    }

    None
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
