# CODEBASE.md: dubstrip Semantic Digest

> **Notice**: This file is an AI-optimized semantic index. Do not write narrative prose. Keep token density high.

## 1. System Topology & Data Flow
```text
CLI (src/main.rs)
  ├──> Probe (src/probe.rs) ──> ffprobe/mkvmerge JSON inspect
  ├──> AI Origin Dispatcher (src/ai.rs)
  │      ├──> Wikidata P364 / Wikipedia API (src/ai/wikipedia.rs)
  │      ├──> OMDb API (src/ai/omdb.rs)
  │      └──> Gemini Flash REST (src/gemini.rs)
  ├──> Decision Engine (src/decide.rs) ──> Confidence safety gate (>=80%)
  ├──> Safety Verification (src/safety.rs) ──> Incomplete ext checks & settling
  ├──> Remux Engine (src/remux.rs) ──> mkvmerge zero-loss remux
  └──> Notification Engine (src/notify.rs) ──> ryoiki notify Telegram alert
```

## 2. Global Constraints & Architecture Patterns
- **Primary Language & Edition**: Rust 2021 edition
- **Architectural Paradigm**: Modular CLI with domain-isolated AI resolution, decision engine, and remux runner
- **Hard Constraints**: <400 lines/file (300 soft), <60 lines/fn (40 soft), zero production unwrap(), 0 warnings
- **Target Distribution**: Linux x86_64 standalone binary via GitHub Releases

## 3. Module & Interface Skeleton

### `src/main.rs` (Role: cli, Lines: 240)
- **Responsibility**: Clap CLI definition, subcommand dispatching, and exit handling.
- **Imports**: `clap`, `colored`, `crate::ai`, `crate::config`, `crate::decide`, `crate::notify`, `crate::probe`, `crate::remux`, `crate::safety`, `crate::sweep`, `crate::ui`
- **Types & Enums**:
  ```rust
  pub struct Cli { pub command: Commands, pub dry_run: bool, pub verbose: bool }
  pub enum Commands { Probe { path: PathBuf }, Inspect { path: PathBuf }, Strip { path: PathBuf, force: bool }, Sweep { dir: PathBuf, recursive: bool }, Config { key: Option<String>, value: Option<String> } }
  ```
- **Public Functions & Signatures**:
  ```rust
  fn main() -> anyhow::Result<()>
  ```
- **Consumers**: Process entrypoint
- **Side Effects / I/O**: stdout/stderr, invokes child commands

### `src/ai.rs` (Role: domain, Lines: 207)
- **Responsibility**: High-level film origin determination, tracker sanitization regex, language normalization.
- **Imports**: `crate::ai::omdb`, `crate::ai::wikipedia`, `crate::gemini`, `regex::Regex`
- **Types & Enums**:
  ```rust
  pub struct FilmOrigin { pub language_code: String, pub language_name: String, pub confidence: u8, pub source: String }
  impl FilmOrigin { pub fn is_confident(&self) -> bool }
  ```
- **Public Functions & Signatures**:
  ```rust
  pub fn parse_title_and_year(path: &Path) -> (String, Option<u32>)
  pub fn determine_film_origin(title: &str, year: Option<u32>, raw_filename: &str, streams: &[String], api_key: Option<&str>) -> anyhow::Result<FilmOrigin>
  pub fn normalize_lang_code(code: &str) -> String
  ```
- **Consumers**: `src/main.rs`, `src/sweep.rs`
- **Side Effects / I/O**: Dispatches network requests to Wikipedia, OMDb, and Gemini

### `src/ai/wikipedia.rs` (Role: infra, Lines: 150)
- **Responsibility**: Dynamic film origin resolution via Wikidata property P364 and Wikipedia REST summary extract fallback.
- **Imports**: `reqwest::blocking::Client`, `serde_json::Value`
- **Public Functions & Signatures**:
  ```rust
  pub fn query_wikipedia_film_origin(title: &str, year: Option<u32>, streams: &[String]) -> anyhow::Result<FilmOrigin>
  ```
- **Consumers**: `src/ai.rs`
- **Side Effects / I/O**: HTTP requests to `en.wikipedia.org` and `www.wikidata.org`

### `src/ai/omdb.rs` (Role: infra, Lines: 93)
- **Responsibility**: Film origin lookup via OMDb API fallback.
- **Imports**: `reqwest::blocking::Client`, `serde_json::Value`
- **Public Functions & Signatures**:
  ```rust
  pub fn query_omdb_film_origin(title: &str, year: Option<u32>, streams: &[String], api_key: &str) -> anyhow::Result<FilmOrigin>
  ```
- **Consumers**: `src/ai.rs`
- **Side Effects / I/O**: HTTP requests to `www.omdbapi.com`

### `src/gemini.rs` (Role: infra, Lines: 149)
- **Responsibility**: Film origin inference via Gemini REST API with tracker sanitization and piracy anti-bias rules.
- **Imports**: `reqwest::blocking::Client`, `serde_json::Value`, `regex::Regex`
- **Public Functions & Signatures**:
  ```rust
  pub fn query_gemini_film_origin(title: &str, year: Option<u32>, streams: &[String], raw_filename: &str, api_key: &str) -> anyhow::Result<FilmOrigin>
  ```
- **Consumers**: `src/ai.rs`
- **Side Effects / I/O**: HTTP requests to `generativelanguage.googleapis.com`

### `src/decide.rs` (Role: domain, Lines: 181)
- **Responsibility**: Stream retention decision engine; gates stripping on `>= 80%` origin confidence.
- **Imports**: `crate::ai::FilmOrigin`, `crate::probe::MediaProbe`
- **Types & Enums**:
  ```rust
  pub struct StripDecision { pub target_audio_tracks: Vec<AudioTrackDecision>, pub can_strip: bool, pub space_reclaim_estimate: u64 }
  pub struct AudioTrackDecision { pub id: u32, pub language: String, pub title: Option<String>, pub channels: u32, pub action: Action, pub reason: String }
  pub enum Action { Keep, Strip }
  ```
- **Public Functions & Signatures**:
  ```rust
  pub fn make_strip_decision(probe: &MediaProbe, origin: &FilmOrigin) -> StripDecision
  ```
- **Consumers**: `src/main.rs`, `src/sweep.rs`

### `src/probe.rs` (Role: infra, Lines: 160)
- **Responsibility**: Video container inspection using `ffprobe` / `mkvmerge` JSON outputs.
- **Imports**: `serde::Deserialize`, `std::process::Command`
- **Types & Enums**:
  ```rust
  pub struct MediaProbe { pub file_path: PathBuf, pub file_size_bytes: u64, pub audio_tracks: Vec<AudioTrackInfo>, pub subtitle_tracks: Vec<SubtitleTrackInfo> }
  pub struct AudioTrackInfo { pub id: u32, pub language: String, pub title: Option<String>, pub codec: String, pub channels: u32, pub bitrate: Option<u64> }
  ```
- **Public Functions & Signatures**:
  ```rust
  pub fn probe_file(path: &Path) -> anyhow::Result<MediaProbe>
  ```
- **Consumers**: `src/main.rs`, `src/sweep.rs`
- **Side Effects / I/O**: Executes `ffprobe` and `mkvmerge -J`

### `src/remux.rs` (Role: infra, Lines: 98)
- **Responsibility**: Executes lossless `mkvmerge` track extraction and atomic file replacement.
- **Imports**: `crate::decide::StripDecision`, `std::process::Command`
- **Public Functions & Signatures**:
  ```rust
  pub fn execute_remux(input: &Path, output: &Path, decision: &StripDecision) -> anyhow::Result<()>
  ```
- **Consumers**: `src/main.rs`, `src/sweep.rs`
- **Side Effects / I/O**: Invokes `mkvmerge`, writes new MKV, atomically renames file

### `src/safety.rs` (Role: domain, Lines: 130)
- **Responsibility**: Pre-flight safety: rejects incomplete torrent extensions, verifies file settling, checks disk headroom.
- **Imports**: `std::fs`, `std::path::Path`
- **Public Functions & Signatures**:
  ```rust
  pub fn verify_file_safety(path: &Path) -> anyhow::Result<()>
  pub fn check_free_space(path: &Path, required_bytes: u64) -> anyhow::Result<()>
  ```
- **Consumers**: `src/main.rs`, `src/sweep.rs`

### `src/notify.rs` (Role: infra, Lines: 121)
- **Responsibility**: Sends rich Telegram alert cards for stripped dubs or preserved multi files via `ryoiki notify`.
- **Imports**: `std::process::Command`
- **Public Functions & Signatures**:
  ```rust
  pub fn notify_dub_stripped(path: &Path, origin: &FilmOrigin, kept_tracks: &[String], stripped_tracks: &[String], bytes_saved: u64) -> anyhow::Result<()>
  pub fn notify_preserved_multi(path: &Path, origin: &FilmOrigin, reason: &str) -> anyhow::Result<()>
  ```
- **Consumers**: `src/main.rs`, `src/sweep.rs`
- **Side Effects / I/O**: Invokes `ryoiki notify` CLI

### `src/sweep.rs` (Role: api, Lines: 189)
- **Responsibility**: Batch directory traversal and automated stripping across movie folders.
- **Imports**: `crate::ai`, `crate::decide`, `crate::probe`, `crate::remux`, `crate::safety`
- **Public Functions & Signatures**:
  ```rust
  pub fn run_sweep(dir: &Path, recursive: bool, dry_run: bool) -> anyhow::Result<()>
  ```
- **Consumers**: `src/main.rs`

### `src/config.rs` (Role: infra, Lines: 169)
- **Responsibility**: Manages JSON configuration file at `~/.config/dubstrip/config.json`.
- **Imports**: `serde::{Deserialize, Serialize}`
- **Public Functions & Signatures**:
  ```rust
  pub fn load_config() -> anyhow::Result<DubstripConfig>
  pub fn save_config(config: &DubstripConfig) -> anyhow::Result<()>
  ```

### `src/ui.rs` (Role: cli, Lines: 186)
- **Responsibility**: Terminal presentation, tables, and Indicatif progress bars.

## 4. Execution Lifecycle Trace
1. **Startup**: `main.rs` parses arguments via `clap`.
2. **Safety Check**: `safety.rs` validates file extension and stability.
3. **Probe**: `probe.rs` runs `mkvmerge -J` to parse container track hierarchy.
4. **AI Origin Resolution**:
   - `ai/wikipedia.rs` queries Wikidata P364 / Wikipedia API.
   - `gemini.rs` or `omdb.rs` act as fallbacks with anti-bias filtering.
5. **Decision**: `decide.rs` evaluates confidence ($\ge 80\%$) and tags tracks as `Keep` or `Strip`.
6. **Execution**: If confident and dub tracks present, `remux.rs` remuxes without quality loss. If unconfident, keeps Multi.
7. **Notification**: `notify.rs` sends status card via `ryoiki notify`.

## 5. Verification Commands
```bash
# Build
cargo build --release --target x86_64-unknown-linux-gnu

# Test
cargo test --all-targets

# Lint & Format
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## 6. Recent Iteration Changes
- **2026-09-23**:
  - `src/ai.rs`: Added tracker domain regex `re_tracker` to strip dynamic tracker prefixes (`www.1TamilMV.haus`, `[1TamilMV.lat]`).
  - `src/gemini.rs`: Added prompt sanitization to strip audio language tag brackets and added explicit anti-bias rules against torrent track ordering.
  - Bumped version to `v0.2.2`.
