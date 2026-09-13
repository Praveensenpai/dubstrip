# 🗡️ 吹き替え除去 (DubStrip) — Smart Native Audio Preserver

> **Lossless, origin-aware audio stream stripper that purges redundant dubs, strictly preserves the native theatrical master track, and enforces 4-gate download safety.**

[![Rust Edition](https://img.shields.io/badge/Rust-2024%20Edition-DEA584?style=flat-square&logo=rust)](Cargo.toml)
[![Platform](https://img.shields.io/badge/Platform-Linux-FCC624?style=flat-square&logo=linux&logoColor=black)](#)
[![License: MIT](https://img.shields.io/badge/License-MIT-89b4fa?style=flat-square)](LICENSE)

> [!IMPORTANT]
> **No Static Whitelists · No English Trash · 4-Gate Download Safety**  
> Dynamic origin resolution ensures that what is original for a Malayalam film is preserved, while Kannada/Telugu/Hindi/English dubs are stripped. Conversely, Kannada originals preserve Kannada and strip Malayalam. Unverified or `und` tracks are fail-safe preserved.

---

## 📦 System Prerequisites

`dubstrip` relies on high-speed media inspection tools (`ffprobe`, `ffmpeg`), container utilities (`mkvmerge`), and Linux process lock checkers (`fuser`, `lsof`).

### 🐧 Ubuntu / Debian
```bash
sudo apt update
sudo apt install -y ffmpeg mkvtoolnix psmisc lsof
```

### 🏹 Arch Linux
```bash
sudo pacman -S --needed ffmpeg mkvtoolnix-cli psmisc lsof
```

> [!NOTE]
> * On Arch Linux, `mkvtoolnix-cli` provides the headless `mkvmerge` & `mkvpropedit` CLI binaries without pulling GUI dependencies.
> * `psmisc` provides `/usr/bin/fuser` for instantaneous kernel write-lock checks.

---

## 🛡️ The 4-Gate Safety Architecture

To guarantee that active torrents or file transfers are never corrupted or remuxed prematurely:

```text
┌──────────────────────────────────────────────────────────────┐
│                  📁 Target Media Candidate                   │
└──────────────────────────────┬───────────────────────────────┘
                               │
                               ▼
  [ Gate 1: Incomplete Name & Directory Filter ]
  Rejects .!qB, .part, .crdownload, .tmp, /incomplete/, /downloading/
                               │
                               ▼
  [ Gate 2: Active Process Write Lock Check ]
  Queries kernel locks via `fuser` (fallback to `lsof`)
                               │
                               ▼
  [ Gate 3: 60-Second Quiescence Settling Window ]
  Rejects files modified within the last 60 seconds
                               │
                               ▼
  [ Gate 4: Container Index & Moov Atom Probe ]
  Validates stream headers and container integrity via `ffprobe`
                               │
                               ▼
 🎬 Safe for Dynamic Origin Resolution & Lossless Stream Copying
```

---

## 🚀 Quick Installation

### Build & Install From Source
```bash
git clone https://github.com/Praveensenpai/dubstrip.git
cd dubstrip
cargo build --release
install -Dm 755 target/release/dubstrip ~/.local/bin/dubstrip
```

Verify the installation:
```bash
dubstrip --help
```

---

## 💻 CLI Usage & Workflows

### 1. Non-Destructive Stream Inspection
Inspect audio streams, codec info, bitrates, channels, and planned actions without touching the file:
```bash
dubstrip inspect "Drishyam 3 (2026) [1080p].mkv"
```

```text
  🎬 Analyzing: Drishyam 3 (2026) [1080p].mkv
  📦 File Size: 10.74 GiB (1 video, 8 audio, 1 subs)
  🧠 Detected Origin: Malayalam (mal) — via Jellyfin OMDb Cache
  ──────────────────────────────────────────────────────────────────────
  #    Lang     Channels     Codec      Track Title              Action  
  ──────────────────────────────────────────────────────────────────────
  1    tam      5.1 Surround eac3       www.1TamilMV.cards - …   STRIP  (Dubbed audio track)
  2    tel      5.1 Surround eac3       www.1TamilMV.cards - …   STRIP  (Dubbed audio track)
  3    mal      5.1 Surround eac3       www.1TamilMV.cards - …   KEEP   (Native theatrical track)
  4    kan      5.1 Surround eac3       www.1TamilMV.cards - …   STRIP  (Dubbed audio track)
  5    tam      2.0 Stereo   aac        www.1TamilMV.cards - …   STRIP  (Dubbed audio track)
  6    tel      2.0 Stereo   aac        www.1TamilMV.cards - …   STRIP  (Dubbed audio track)
  7    mal      2.0 Stereo   aac        www.1TamilMV.cards - …   KEEP   (Native theatrical track)
  8    kan      2.0 Stereo   aac        www.1TamilMV.cards - …   STRIP  (Dubbed audio track)
  ──────────────────────────────────────────────────────────────────────
```

### 2. Interactive Strip
Displays the plan and prompts for explicit confirmation before processing:
```bash
dubstrip strip "Drishyam 3 (2026) [1080p].mkv"
```

### 3. Dry-Run Mode
Test the remuxing logic and plan without making any disk modifications:
```bash
dubstrip strip --dry-run "movie.mkv"
```

### 4. Headless / Automated Mode (for Daemon & Ryoiki Pipeline)
Runs without interactive prompts; fails safely if origin is unresolved:
```bash
dubstrip strip --auto "/path/to/movie.mkv"
```

### 5. Library Batch Sweep
Sweeps an entire movie or series library, skipping already processed or single-audio files:
```bash
dubstrip sweep --dry-run ~/jellyfin/media/movies/
dubstrip sweep --auto ~/jellyfin/media/movies/
```

---

## ⚡ Key Highlights

- ⚡ **100% Lossless Remuxing**: Zero re-encoding. Audio and video streams are copied losslessly (`-c copy`) in seconds.
- 🔒 **Atomic Swap Guarantee**: Outputs to a temporary sibling file (`.tmp.mkv`), validates container size and index integrity, and performs an atomic rename.
- 🎯 **Fail-Safe Unknown Track Handling**: Any audio track lacking standard ISO language tags (`und`) is preserved by default to prevent accidental data loss.
- 🧹 **Zero Junk Preservation**: Secondary dubbed audio tracks (including unwanted English dubs on non-English films) are pruned cleanly.

---

## 📜 License

Licensed under the [MIT License](LICENSE).  
Crafted with 🦀 by Praveen Senpai ([@Praveensenpai](https://github.com/Praveensenpai)).
