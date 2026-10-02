<div align="center">

<img src="src-tauri/icons/app-icon.svg" width="160" height="160" alt="Vibird Browser" />

# Vibird Browser

A lightweight, privacy-oriented desktop browser for Linux x86_64, built with Rust.  
*Trình duyệt desktop cho Linux, viết bằng Rust, tập trung vào quyền riêng tư và mức tiêu thụ tài nguyên thấp.*

[![Build](https://img.shields.io/github/actions/workflow/status/LocShadowVN/VibirdBrowser/build.yml?branch=main&style=flat-square&label=build)](https://github.com/LocShadowVN/VibirdBrowser/actions)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20x86__64-lightgrey.svg?style=flat-square)](#installation)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg?style=flat-square)](https://www.rust-lang.org/)
[![WebKitGTK](https://img.shields.io/badge/webview-WebKitGTK%204.1-informational.svg?style=flat-square)](https://webkitgtk.org/)

[Tiếng Việt](#tiếng-việt) · [English](#english)

</div>

---

<a name="tiếng-việt"></a>
## Tiếng Việt

### Mục lục

- [Giới thiệu](#giới-thiệu)
- [Kiến trúc](#kiến-trúc)
- [Tính năng](#tính-năng)
- [Cài đặt](#cài-đặt)
- [Hạn chế đã biết](#hạn-chế-đã-biết)
- [Build từ mã nguồn](#build-từ-mã-nguồn)
- [Ghi chú kỹ thuật](#ghi-chú-kỹ-thuật)
- [Triết lý thiết kế](#triết-lý-thiết-kế)
- [Ghi nhận](#ghi-nhận)
- [Giấy phép](#giấy-phép)

---

### Giới thiệu

Vibird Browser là trình duyệt desktop thử nghiệm cho Linux x86_64, xây dựng trên:

- **Tauri v2** — runtime và IPC layer.
- **WebKitGTK 4.1** — engine render, dùng thư viện có sẵn trên hệ điều hành.
- **Leptos 0.6 (CSR) + Rust WASM** — toàn bộ UI (tab strip, omnibox, settings, vault, shields).

Mục tiêu: bộ nhớ thấp, không telemetry, có sẵn tính năng bảo vệ quyền riêng tư mà không cần cài extension.

Đây là dự án **thử nghiệm (MVP)**, chưa qua kiểm toán bảo mật. Xem [Ghi chú kỹ thuật](#ghi-chú-kỹ-thuật).

---

### Kiến trúc

```
┌──────────────────────────────────────────────────────────────────┐
│                       Main window (Tauri)                        │
├──────────────────────────────────────────────────────────────────┤
│  UI webview ("main")                                             │
│    Leptos CSR · tab strip · omnibox · settings · vault · shields │
├──────────────────────────────────────────────────────────────────┤
│  Content webviews ("tab_1", "tab_2", …)                          │
│    WebKitGTK native subsurfaces, positioned below chrome bar     │
└──────────────────────────────────────────────────────────────────┘
              ▲                                    ▲
              │ Tauri IPC (async, JSON)            │ shared State<T>
              ▼                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Rust backend (Tauri v2)                       │
│  ShieldEngine · DownloadEngine · VaultSession · DbManager        │
│  DnsResolver · ExtensionEngine                                   │
└──────────────────────────────────────────────────────────────────┘
```

Trang web render trong webview con native của WebKitGTK, không dùng `<iframe>`. Mỗi tab có webview riêng, `hide()` để snooze hoặc `close()` để giải phóng.

---

### Tính năng

#### Vibird Shield (adblock)

- Engine `adblock-rust` của Brave chạy trên OS thread riêng.
- Parse EasyList (~300k rules) thành ~8000 domain blocks + 500 substring rules + whitelist.
- JS hooks + MutationObserver chặn fetch/XHR/WebSocket/script/iframe/img.
- Cosmetic CSS ẩn ads, cookie banners, YouTube promoted content.
- YouTube auto-skip pre-roll + mid-roll.
- Per-site exception qua `vibird://shields`.
- Whitelist cứng cho Google/YouTube/CDN/dev sites để tránh vỡ site.

#### Privacy

- WebRTC leak shield — xoá IP LAN khỏi SDP.
- UA spoof Chrome 132 + client hints.
- `navigator.plugins` 5 PDF entries, WebGL vendor/renderer spoof.
- Canvas + AudioBuffer farbling.
- Clean URL — strip 27 tracking params.
- De-AMP — rewrite Google AMP về canonical.

#### Vault

- Argon2id hash master password.
- AES-256-GCM per record, nonce riêng.
- Auto-lock sau 10 phút idle.
- Rate limit 5 lần sai → lockout.
- Zeroize key khi drop.
- Autofill 1-click.

#### Download

- Multi-thread 4–16 TCP qua HTTP Range.
- Sanitize path traversal.
- Progress shelf real-time.

#### Tab

- Session restore.
- Smart snooze 10 phút.
- Incognito (label, chưa isolated profile).
- Ctrl+Tab cycle.

#### Navigation

- Ctrl+click / middle-click mở tab mới.
- Zoom Ctrl+`+`/`-`/`0`, lưu per-origin.
- Find in page Ctrl+F với highlight + counter.
- Context menu chuột phải đầy đủ.

#### Auto-update

- Kiểm tra GitHub Releases.
- Tải `.deb` về, gọi `pkexec dpkg -i` (dialog password GUI).
- Tự restart sau khi cài xong.
- Fallback `xdg-open` → GUI installer nếu không có `pkexec`.

---

### Cài đặt

#### Cài từ .deb (khuyến nghị)

```bash
wget https://github.com/LocShadowVN/VibirdBrowser/releases/latest/download/vibird-browser_amd64.deb
sudo apt install ./vibird-browser_amd64.deb
```

`apt install` sẽ tự động cài dependencies:
- `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1`
- `bubblewrap` (WebKit sandbox)
- `gstreamer1.0-plugins-{base,good,bad,ugly}`, `gstreamer1.0-libav` (media decode)

#### Cài từ Flatpak

```bash
flatpak install --user vibird-browser_amd64.flatpak
flatpak run io.github.locshadowvn.Vibird
```

Flatpak runtime `org.gnome.Platform` đã có sẵn WebKitGTK, GStreamer, bubblewrap. Không cần cài thêm gì.

#### Nâng cấp

Mở app → Settings → About & Updates → **Check for updates** → **Update now**. App tự tải `.deb`, hỏi password qua dialog, cài, restart.

---

### Hạn chế đã biết

**Giới hạn kiến trúc** (không phải bug):

1. **Google Meet, Microsoft Teams.** WebKitGTK không hỗ trợ đầy đủ WebCodecs + Chromium-only ML pipelines.
2. **DRM (Netflix, Spotify Web).** Không bundle Widevine CDM. Google không cấp license cho dự án cá nhân.
3. **Chrome Web Store.** Chỉ load unpacked extension với `manifest.json`. Runtime chưa execute extension.
4. **Chỉ có Linux x86_64.** Không có Windows, macOS, ARM.
5. **Không sync.** Bookmarks, history, vault lưu local.
6. **UI đa ngôn ngữ giới hạn.** Chỉ VI + EN, một số chuỗi hard-coded.

**Vấn đề đang gặp**:

- **Content webview có thể che UI trên Wayland.** Workaround `GDK_BACKEND=x11` hoặc chuyển sang X11 session.
- **Khoảng đen đầu trang** trên một số site phức tạp.
- **YouTube có thể crash** trên một số cấu hình (đã fix partial).
- **Extension chưa thực sự chạy** — chỉ parse manifest.

---

### Build từ mã nguồn

#### Yêu cầu hệ thống

Debian / Ubuntu / Linux Mint:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl wget file libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

#### Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown

cargo install trunk
cargo install tauri-cli --version "^2.0.0"
```

#### Build

```bash
git clone https://github.com/LocShadowVN/VibirdBrowser.git
cd VibirdBrowser

cargo tauri dev                    # development
cargo tauri build --bundles deb    # release .deb
```

Output: `target/release/bundle/deb/vibird-browser_*_amd64.deb`

---

### Ghi chú kỹ thuật

#### Nguồn dữ liệu

- **Database:** `~/.local/share/vibird-browser/vibird_system.sqlite` (SQLite, WAL)
- **Custom rules:** `~/.local/share/vibird-browser/custom_rules.txt`
- **Downloads:** cấu hình trong Settings (mặc định `/tmp`)

Nếu có `~/.local/share/caram-browser/` cũ, DB tự động migrate lần đầu chạy.

#### Bảo mật

- Master password hash bằng Argon2id. Không recover được nếu quên.
- Không hardening chống memory dump. Attacker có root đọc được secret trong RAM.
- Content webview chỉ có permission `core:event:default`. Command truy cập file/vault/shell chỉ gọi được từ `main` webview.
- Không bundle Widevine CDM.

#### Disclaimer

Phần mềm phân phối theo GNU GPL-3.0, "as-is", không bảo hành. Không nên dùng làm nơi lưu credential giá trị cao.

---

### Triết lý thiết kế

Logo dựa trên họa tiết **Chim Lạc** trống đồng Đông Sơn, tránh các motif phổ biến như shield/neon glow/3D bevel.

- Chim cách điệu thành chữ **V** bằng polygon góc cạnh.
- Đầu ngẩng cao, mỏ dài, ba dải lông mào.
- Bảng màu: vàng đồng cổ (`#D4AF37`) trên nền lacquer đen.

<details>
<summary>Mã nguồn SVG (<code>src-tauri/icons/app-icon.svg</code>)</summary>

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <defs>
    <radialGradient id="vBg" cx="50%" cy="45%" r="65%">
      <stop offset="0%" stop-color="#141923"/>
      <stop offset="100%" stop-color="#080A0F"/>
    </radialGradient>
  </defs>

  <rect width="512" height="512" rx="116" fill="url(#vBg)"/>
  <rect width="506" height="506" x="3" y="3" rx="113" fill="none" stroke="#222A38" stroke-width="2"/>

  <circle cx="256" cy="256" r="176" fill="none" stroke="#D4AF37" stroke-width="1.5" opacity="0.15" stroke-dasharray="8, 8"/>
  <circle cx="256" cy="256" r="140" fill="none" stroke="#D4AF37" stroke-width="1" opacity="0.1"/>

  <polygon points="210,310 145,210 160,150 240,240" fill="#996515" opacity="0.6"/>
  <polygon points="160,150 120,170 190,265" fill="#784E0E" opacity="0.4"/>

  <polygon points="235,395 140,230 170,120 280,270" fill="#D4AF37"/>
  <polygon points="170,120 135,145 220,295 235,395" fill="#B38622"/>

  <path d="M 235 395 
           C 270 330, 310 260, 350 200 
           L 435 125 
           L 365 170 
           C 335 195, 305 240, 275 305 
           Z" 
        fill="#E6B800"/>

  <polygon points="435,125 365,170 375,150" fill="#FFF2B2"/>

  <polygon points="345,185 240,165 315,198" fill="#F3E5AB"/>
  <polygon points="330,200 205,185 295,215" fill="#D4AF37"/>
  <polygon points="310,218 190,208 275,235" fill="#B38622"/>

  <polygon points="235,395 285,390 260,425" fill="#D4AF37" opacity="0.85"/>
  <polygon points="215,380 235,395 200,410" fill="#996515"/>
</svg>
```
</details>

---

### Ghi nhận

Phát triển với hỗ trợ của các mô hình ngôn ngữ:

- **Gemini 3.8 Flash** — sinh mã, gợi ý kiến trúc, review.
- **DeepSeek V4.1 Flash** — sinh mã, rà soát logic.

Mọi quyết định kiến trúc, review cuối cùng, và trách nhiệm phát hành thuộc về maintainer.

---

### Giấy phép

GNU General Public License v3.0. Xem [LICENSE](LICENSE).

---
---

<a name="english"></a>
## English

### Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Features](#features)
- [Installation](#installation)
- [Known Limitations](#known-limitations)
- [Building from Source](#building-from-source)
- [Technical Notes](#technical-notes)
- [Design Philosophy](#design-philosophy)
- [Acknowledgements](#acknowledgements)
- [License](#license)

---

### Overview

Vibird Browser is an experimental desktop browser for Linux x86_64, built on:

- **Tauri v2** — runtime and IPC layer.
- **WebKitGTK 4.1** — rendering engine, reusing the OS library.
- **Leptos 0.6 (CSR) + Rust WASM** — the entire browser chrome.

Goal: low memory, no telemetry, built-in privacy without extensions.

This is an **experimental (MVP)** project. Not audited. See [Technical Notes](#technical-notes).

---

### Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                       Main window (Tauri)                        │
├──────────────────────────────────────────────────────────────────┤
│  UI webview ("main")                                             │
│    Leptos CSR · tab strip · omnibox · settings · vault · shields │
├──────────────────────────────────────────────────────────────────┤
│  Content webviews ("tab_1", "tab_2", …)                          │
│    WebKitGTK native subsurfaces, positioned below chrome bar     │
└──────────────────────────────────────────────────────────────────┘
              ▲                                    ▲
              │ Tauri IPC (async, JSON)            │ shared State<T>
              ▼                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Rust backend (Tauri v2)                       │
│  ShieldEngine · DownloadEngine · VaultSession · DbManager        │
│  DnsResolver · ExtensionEngine                                   │
└──────────────────────────────────────────────────────────────────┘
```

---

### Features

#### Vibird Shield (adblock)

- Brave's `adblock-rust` engine on dedicated OS thread.
- Parses EasyList (~300k rules) into ~8000 domain blocks + 500 substring rules + whitelist.
- JS hooks + MutationObserver block fetch/XHR/WebSocket/script/iframe/img.
- Cosmetic CSS hides ads, cookie banners, YouTube promoted content.
- YouTube auto-skip pre-roll + mid-roll.
- Per-site exceptions via `vibird://shields`.
- Hard whitelist for Google/YouTube/CDN/dev sites.

#### Privacy

- WebRTC leak shield — strips LAN IPs from SDP.
- Chrome 132 UA spoof + client hints.
- `navigator.plugins` 5 PDF entries, WebGL vendor/renderer spoof.
- Canvas + AudioBuffer farbling.
- Clean URL — strips 27 tracking params.
- De-AMP — rewrites Google AMP to canonical.

#### Vault

- Argon2id master password hash.
- AES-256-GCM per record, unique nonce.
- Auto-lock after 10 min idle.
- Rate limit: 5 wrong attempts → lockout.
- Zeroize key on drop.
- 1-click autofill.

#### Download

- Multi-threaded 4–16 TCP via HTTP Range.
- Path traversal sanitization.
- Real-time progress shelf.

#### Tabs

- Session restore.
- Smart snooze at 10 min.
- Incognito (label only, no isolated profile yet).
- Ctrl+Tab cycle.

#### Navigation

- Ctrl+click / middle-click to open new tab.
- Zoom Ctrl+`+`/`-`/`0`, persisted per-origin.
- Find in page Ctrl+F with highlight + counter.
- Full right-click context menu.

#### Auto-update

- Checks GitHub Releases.
- Downloads `.deb`, runs `pkexec dpkg -i` (GUI password dialog).
- Auto-restart after install.
- Fallback to `xdg-open` → GUI installer if `pkexec` missing.

---

### Installation

#### From .deb (recommended)

```bash
wget https://github.com/LocShadowVN/VibirdBrowser/releases/latest/download/vibird-browser_amd64.deb
sudo apt install ./vibird-browser_amd64.deb
```

`apt install` pulls dependencies automatically:
- `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1`
- `bubblewrap` (WebKit sandbox)
- `gstreamer1.0-plugins-{base,good,bad,ugly}`, `gstreamer1.0-libav` (media decode)

#### From Flatpak

```bash
flatpak install --user vibird-browser_amd64.flatpak
flatpak run io.github.locshadowvn.Vibird
```

Flatpak `org.gnome.Platform` runtime provides WebKitGTK, GStreamer, bubblewrap. No system deps needed.

#### Upgrading

Open app → Settings → About & Updates → **Check for updates** → **Update now**. Auto-downloads `.deb`, prompts for password via GUI dialog, installs, restarts.

---

### Known Limitations

**Architectural** (not bugs):

1. **Google Meet, Microsoft Teams.** WebKitGTK lacks full WebCodecs + Chromium-only ML pipelines.
2. **DRM (Netflix, Spotify Web).** No Widevine CDM. Google doesn't license individual projects.
3. **Chrome Web Store.** Only unpacked extension `manifest.json` parsed. Runtime doesn't execute extensions yet.
4. **Linux x86_64 only.** No Windows, macOS, ARM.
5. **No sync.** Bookmarks, history, vault local only.
6. **Limited localization.** VI + EN only, some hard-coded strings.

**Current issues**:

- **Content webview may overlap UI on Wayland.** Workaround: `GDK_BACKEND=x11` or switch to X11 session.
- **Black gap at top of some sites.**
- **YouTube may crash** on some configs (partial fix).
- **Extensions don't execute** — manifest parsed only.

---

### Building from Source

#### System prerequisites

Debian / Ubuntu / Linux Mint:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl wget file libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

#### Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown

cargo install trunk
cargo install tauri-cli --version "^2.0.0"
```

#### Build

```bash
git clone https://github.com/LocShadowVN/VibirdBrowser.git
cd VibirdBrowser

cargo tauri dev                    # development
cargo tauri build --bundles deb    # release .deb
```

Output: `target/release/bundle/deb/vibird-browser_*_amd64.deb`

---

### Technical Notes

#### Data locations

- **Database:** `~/.local/share/vibird-browser/vibird_system.sqlite` (SQLite, WAL)
- **Custom rules:** `~/.local/share/vibird-browser/custom_rules.txt`
- **Downloads:** configurable in Settings (default `/tmp`)

Legacy `~/.local/share/caram-browser/` auto-migrates on first launch.

#### Security

- Master password hashed with Argon2id. No recovery path.
- No memory-dump hardening. Root attacker can read RAM secrets.
- Content webviews only have `core:event:default` permission. File/vault/shell commands only callable from `main` webview.
- No Widevine CDM bundled.

#### Disclaimer

Distributed under GNU GPL-3.0 strictly "as-is", no warranty. Not recommended as sole store for high-value credentials.

---

### Design Philosophy

Logo based on **Chim Lạc** (Lạc bird) motif from Đông Sơn bronze drums. Avoids shield/neon glow/3D bevel clichés.

- Bird stylized into a **V** shape using sharp polygonal facets.
- Head raised, elongated bill, three crest plumes.
- Palette: antique brass gold (`#D4AF37`) on deep lacquer black.

<details>
<summary>Raw SVG source (<code>src-tauri/icons/app-icon.svg</code>)</summary>

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <defs>
    <radialGradient id="vBg" cx="50%" cy="45%" r="65%">
      <stop offset="0%" stop-color="#141923"/>
      <stop offset="100%" stop-color="#080A0F"/>
    </radialGradient>
  </defs>

  <rect width="512" height="512" rx="116" fill="url(#vBg)"/>
  <rect width="506" height="506" x="3" y="3" rx="113" fill="none" stroke="#222A38" stroke-width="2"/>

  <circle cx="256" cy="256" r="176" fill="none" stroke="#D4AF37" stroke-width="1.5" opacity="0.15" stroke-dasharray="8, 8"/>
  <circle cx="256" cy="256" r="140" fill="none" stroke="#D4AF37" stroke-width="1" opacity="0.1"/>

  <polygon points="210,310 145,210 160,150 240,240" fill="#996515" opacity="0.6"/>
  <polygon points="160,150 120,170 190,265" fill="#784E0E" opacity="0.4"/>

  <polygon points="235,395 140,230 170,120 280,270" fill="#D4AF37"/>
  <polygon points="170,120 135,145 220,295 235,395" fill="#B38622"/>

  <path d="M 235 395 
           C 270 330, 310 260, 350 200 
           L 435 125 
           L 365 170 
           C 335 195, 305 240, 275 305 
           Z" 
        fill="#E6B800"/>

  <polygon points="435,125 365,170 375,150" fill="#FFF2B2"/>

  <polygon points="345,185 240,165 315,198" fill="#F3E5AB"/>
  <polygon points="330,200 205,185 295,215" fill="#D4AF37"/>
  <polygon points="310,218 190,208 275,235" fill="#B38622"/>

  <polygon points="235,395 285,390 260,425" fill="#D4AF37" opacity="0.85"/>
  <polygon points="215,380 235,395 200,410" fill="#996515"/>
</svg>
```
</details>

---

### Acknowledgements

Development assisted by:

- **Gemini 3.8 Flash** — code generation, architectural suggestions, reviews.
- **DeepSeek V4.1 Flash** — code generation, logic review.

All architectural decisions, final code review, and release responsibility remain with the project maintainer.

---

### License

GNU General Public License v3.0. See [LICENSE](LICENSE).
