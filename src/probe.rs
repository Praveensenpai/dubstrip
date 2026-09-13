use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub video_count: usize,
    pub audio_streams: Vec<AudioStream>,
    pub subtitle_count: usize,
}

#[derive(Debug, Clone)]
pub struct AudioStream {
    pub index: u32,
    pub codec: String,
    pub channels: u32,
    pub language: String,
    pub title: String,
    pub is_default: bool,
    pub is_original: bool,
}

impl AudioStream {
    #[must_use]
    pub fn is_und(&self) -> bool {
        self.language.is_empty()
            || self.language.eq_ignore_ascii_case("und")
            || self.language.eq_ignore_ascii_case("unknown")
    }

    #[must_use]
    pub fn channel_layout_label(&self) -> &'static str {
        match self.channels {
            1 => "1.0 Mono",
            2 => "2.0 Stereo",
            6 => "5.1 Surround",
            8 => "7.1 Surround",
            _ => "Multi-Channel",
        }
    }
}

// ── FFprobe JSON deserialization models ───────────────────────────────────────

#[derive(Deserialize)]
struct FfprobeOutput {
    streams: Vec<FfprobeStream>,
}

#[derive(Deserialize)]
struct FfprobeStream {
    index: u32,
    codec_name: Option<String>,
    codec_type: String,
    channels: Option<u32>,
    disposition: Option<FfprobeDisposition>,
    tags: Option<FfprobeTags>,
}

#[derive(Deserialize)]
struct FfprobeDisposition {
    default: Option<u32>,
    original: Option<u32>,
}

#[derive(Deserialize)]
struct FfprobeTags {
    language: Option<String>,
    title: Option<String>,
}

/// Probes a media file and returns its structured streams and audio metadata.
pub fn probe_file(path: &Path) -> Result<MediaInfo> {
    if !path.exists() {
        bail!("File not found: {}", path.display());
    }

    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to read file size for {}", path.display()))?;

    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=index,codec_name,codec_type,channels:stream_tags=language,title:stream_disposition=default,original",
            "-of",
            "json",
            &path.display().to_string(),
        ])
        .output()
        .with_context(|| format!("Failed to run ffprobe on {}", path.display()))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        bail!("ffprobe failed: {err}");
    }

    let parsed: FfprobeOutput =
        serde_json::from_slice(&output.stdout).context("Failed to parse ffprobe JSON output")?;

    let mut video_count = 0;
    let mut subtitle_count = 0;
    let mut audio_streams = Vec::new();

    for s in parsed.streams {
        match s.codec_type.as_str() {
            "video" => video_count += 1,
            "subtitle" => subtitle_count += 1,
            "audio" => {
                let channels = s.channels.unwrap_or(2);
                let is_default = s
                    .disposition
                    .as_ref()
                    .and_then(|d| d.default)
                    .is_some_and(|v| v == 1);
                let is_original = s
                    .disposition
                    .as_ref()
                    .and_then(|d| d.original)
                    .is_some_and(|v| v == 1);

                let language = s
                    .tags
                    .as_ref()
                    .and_then(|t| t.language.clone())
                    .unwrap_or_else(|| "und".to_string());

                let title = s
                    .tags
                    .as_ref()
                    .and_then(|t| t.title.clone())
                    .unwrap_or_default();

                audio_streams.push(AudioStream {
                    index: s.index,
                    codec: s.codec_name.unwrap_or_else(|| "unknown".to_string()),
                    channels,
                    language,
                    title,
                    is_default,
                    is_original,
                });
            }
            _ => {}
        }
    }

    Ok(MediaInfo {
        path: path.to_path_buf(),
        size_bytes: metadata.len(),
        video_count,
        audio_streams,
        subtitle_count,
    })
}
