use crate::ai::{normalize_lang_code, FilmOrigin};
use crate::probe::MediaInfo;
use crate::ui::{Action, TrackDecision};

/// Evaluates all audio streams against the resolved native film origin and returns per-track decisions.
pub fn evaluate_audio_streams(media: &MediaInfo, origin: &FilmOrigin) -> Vec<TrackDecision> {
    let mut decisions = Vec::new();
    let total_audio = media.audio_streams.len();

    // Single-track immunity: never strip if only one audio stream exists
    if total_audio <= 1 {
        for s in &media.audio_streams {
            decisions.push(TrackDecision {
                index: s.index,
                language: s.language.clone(),
                codec: s.codec.clone(),
                channels: s.channel_layout_label().to_string(),
                title: s.title.clone(),
                action: Action::Keep,
                reason: "Only audio stream".to_string(),
            });
        }
        return decisions;
    }

    let native_code = origin.native_lang_code.as_str();
    let has_native_match = media
        .audio_streams
        .iter()
        .any(|s| normalize_lang_code(&s.language) == native_code && native_code != "und");

    let mut kept_count = 0;

    for s in &media.audio_streams {
        let norm_stream_lang = normalize_lang_code(&s.language);
        let title_lower = s.title.to_lowercase();

        let (action, reason) = if s.is_und() {
            // Fail-safe: Always preserve 'und' tracks when uncertain
            (Action::Keep, "Unknown tag (Fail-safe preserved)")
        } else if has_native_match && norm_stream_lang == native_code {
            (Action::Keep, "Native theatrical track")
        } else if !has_native_match && (s.is_original || title_lower.contains("original")) {
            (Action::Keep, "Original tag preserved")
        } else if !has_native_match && s.is_default {
            (Action::Keep, "Default stream fallback")
        } else if has_native_match {
            (Action::Strip, "Dubbed audio track")
        } else {
            // If origin is unknown and multiple tracks exist without clear original tags:
            // preserve default and first track, strip other obvious dubs
            if s.index == media.audio_streams[0].index {
                (Action::Keep, "Primary audio fallback")
            } else {
                (Action::Keep, "Preserved (Uncertain origin)")
            }
        };

        if action == Action::Keep {
            kept_count += 1;
        }

        decisions.push(TrackDecision {
            index: s.index,
            language: s.language.clone(),
            codec: s.codec.clone(),
            channels: s.channel_layout_label().to_string(),
            title: s.title.clone(),
            action,
            reason: reason.to_string(),
        });
    }

    // Absolute invariant: At least one audio track must be kept
    if kept_count == 0 && !decisions.is_empty() {
        decisions[0].action = Action::Keep;
        decisions[0].reason = "Emergency single track keeper".to_string();
    }

    decisions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::AudioStream;
    use std::path::PathBuf;

    #[test]
    fn test_single_track_is_immune() {
        let media = MediaInfo {
            path: PathBuf::from("/test.mkv"),
            size_bytes: 1000,
            video_count: 1,
            subtitle_count: 1,
            audio_streams: vec![AudioStream {
                index: 1,
                codec: "aac".to_string(),
                channels: 2,
                language: "hin".to_string(),
                title: "Dub".to_string(),
                is_default: true,
                is_original: false,
            }],
        };
        let origin = FilmOrigin {
            title: "Test".to_string(),
            year: Some(2025),
            native_lang_code: "tel".to_string(),
            native_lang_name: "Telugu".to_string(),
            source: "Test".to_string(),
        };

        let decisions = evaluate_audio_streams(&media, &origin);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].action, Action::Keep);
    }
}
