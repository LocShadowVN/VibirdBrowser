<div align="center">

<img src="src-tauri/icons/app-icon.svg" width="140" height="140" alt="Vibird Browser" />

# Vibird Browser

Trình duyệt desktop nhẹ, riêng tư, viết bằng Rust cho Linux x86_64.
Reuses the system's WebKitGTK — no bundled engine, no telemetry.

[![CI](https://img.shields.io/github/actions/workflow/status/LocShadowVN/VibirdBrowser/ci.yml?branch=main&style=flat-square&label=ci)](https://github.com/LocShadowVN/VibirdBrowser/actions)
[![Release](https://img.shields.io/github/v/release/LocShadowVN/VibirdBrowser?style=flat-square&label=release)](https://github.com/LocShadowVN/VibirdBrowser/releases)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20x86__64-lightgrey.svg?style=flat-square)](#cài-đặt)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg?style=flat-square)](https://www.rust-lang.org/)
[![WebKitGTK](https://img.shields.io/badge/webview-WebKitGTK%204.1-informational.svg?style=flat-square)](https://webkitgtk.org/)

[Tiếng Việt](#tiếng-việt) · [English](#english)

</div>

---

> Dự án cá nhân, đang phát triển. Chưa kiểm toán bảo mật. Dùng hàng ngày cho đọc báo,
> tra cứu — không khuyến nghị làm trình duyệt chính cho công việc quan trọng hoặc lưu
> credential giá trị cao.
>
> Personal project, under development. Not security-audited. Fine for daily browsing —
> not recommended as a primary browser for critical work or high-value credentials.

---

<a name="tiếng-việt"></a>
## Tiếng Việt

### Mục lục

- [Giới thiệu](#giới-thiệu)
- [Tính năng](#tính-năng)
- [Trạng thái](#trạng-thái)
- [Kiến trúc](#kiến-trúc)
- [Cài đặt](#cài-đặt)
- [Hạn chế](#hạn-chế)
- [Build từ mã nguồn](#build-từ-mã-nguồn)
- [Ghi chú kỹ thuật](#ghi-chú-kỹ-thuật)

---

### Giới thiệu

Vibird là trình duyệt desktop cho Linux x86_64, làm 3 việc:

1. **Nhẹ.** Dùng WebKitGTK của hệ thống thay vì bundle engine riêng. Chạy được trên máy 4GB RAM.
2. **Riêng tư.** Adblock nhiều lớp, WebRTC leak shield, canvas farbling, clean URL — cài sẵn, không cần extension.
3. **Đơn giản.** Không sync, không telemetry, không tài khoản. Dữ liệu lưu local.

Đây là trình duyệt phụ. Không phải Chrome killer, không phải Brave thay thế. Không chạy Netflix,
Spotify Web, Google Meet. Dùng để đọc báo, tra Google, vào GitHub.

**Stack:** Tauri v2 · WebKitGTK 4.1 · Leptos 0.6 (CSR/WASM) · SQLite · GPL-3.0.

---

### Tính năng

#### Adblock

Ba lớp độc lập:

- **Network layer** — WebKit `UserContentFilter`, chạy 3 bộ lọc song song:
  EasyList (quảng cáo), EasyPrivacy (tracker), Fanboy Annoyance (cookie banner, popup).
  Chặn request trước khi trang tải. Exception rules `@@` của mỗi bộ được giữ nguyên.
- **JS hooks** — Override `HTMLScriptElement.src`, `HTMLIFrameElement.src`, `HTMLImageElement.src`,
  `setAttribute`, `document.write`, `innerHTML`, `createElement`, `window.open`, `fetch`,
  `XMLHttpRequest.open`, `navigator.sendBeacon`, `WebSocket`. MutationObserver quét node mới.
- **Cosmetic CSS** — Ẩn ad container, cookie banner, YouTube promoted content.
  Scriptlet stub cho Google Analytics, GTM, Facebook Pixel, và các CMP phổ biến
  (`__tcfapi`, `OneTrust`, `Cookiebot`).

Có whitelist bảo vệ site: URL thuộc OAuth, payment (Stripe, PayPal, VNPay, MoMo, ZaloPay),
captcha (reCAPTCHA, hCaptcha, Cloudflare Turnstile), CDN/font phổ biến — không bao giờ bị chặn,
kể cả khi match rule.

Per-site exception qua `vibird://shields`.

#### Privacy

- WebRTC leak shield — xoá IP LAN khỏi SDP
- UA spoof Chrome 132 + client hints
- WebGL vendor/renderer spoof
- Canvas + AudioBuffer farbling
- Clean URL — strip 27 tracking params
- De-AMP — rewrite Google AMP về canonical

#### Vault

- Argon2id hash master password
- AES-256-GCM per record, nonce riêng
- Auto-lock sau 10 phút idle
- Rate limit 5 lần sai → lockout tăng dần
- Zeroize key khi drop

#### Download

- Multi-thread 4–16 TCP qua HTTP Range
- Pause / resume / cancel
- Progress shelf real-time
- Sanitize path traversal

#### Khác

- Session restore, tab snoozer sau 10 phút idle
- Omnibox autocomplete từ history + bookmarks
- Find in page với CSS Custom Highlight
- Zoom per-origin
- Context menu (VI + EN)
- Auto-update qua GitHub Releases, `pkexec dpkg`, tự restart

---

### Trạng thái

#### Đang hoạt động

| Tính năng | Ghi chú |
|---|---|
| Adblock 3 network filter | EasyList + EasyPrivacy + Fanboy |
| Adblock JS hooks + scriptlet | ~10 hook chính + 12 scriptlet stub |
| Cosmetic CSS | Ẩn ad container, cookie banner |
| Per-site exception | `vibird://shields` |
| WebRTC leak shield | SDP sanitize |
| UA + WebGL spoof | Chrome 132 |
| Canvas + Audio farbling | Nhiễu vi mô |
| Clean URL + De-AMP | 27 param, AMP rewrite |
| Vault | Argon2id + AES-256-GCM |
| Download | Multi-thread + pause/resume/cancel |
| Auto-update | `setsid` restart |
| Session restore | SQLite |
| Tab snoozer | Ẩn webview sau 10 phút |
| Find in page | CSS Custom Highlight |
| Omnibox autocomplete | Query history + bookmarks |
| Context menu | VI + EN |

#### Đang phát triển

| Tính năng | Ghi chú |
|---|---|
| YouTube auto-skip | Pre-roll ổn, mid-roll không đều |
| Adblock coverage | Site SPA, redirect chain vẫn lọt |

#### Chưa có

| Tính năng | Ghi chú |
|---|---|
| Extension runtime | Chỉ parse manifest, chưa execute |
| Incognito isolated profile | Chỉ có label, chưa tách cookie/storage |
| Sync | Không có |
| DRM (Widevine) | Không bundle |
| Bookmark folder & edit | Chỉ add/remove phẳng |
| Reader mode | Chưa có |
| Print | Chưa có |
| PDF viewer | Dùng WebKit built-in, chưa có UI tùy chỉnh |

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
│    WebKitGTK native, positioned below chrome bar                 │
└──────────────────────────────────────────────────────────────────┘
              ▲                                    ▲
              │ Tauri IPC (async, JSON)            │ shared State<T>
              ▼                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Rust backend (Tauri v2)                       │
│  ShieldEngine · DownloadEngine · VaultSession · DbManager        │
│  DnsResolver · ExtensionEngine · ContentFilterState              │
└──────────────────────────────────────────────────────────────────┘
```

Trang web render trong webview con native của WebKitGTK, không dùng `<iframe>`.
Mỗi tab có webview riêng. `hide()` để snooze, `close()` để giải phóng.

---

### Cài đặt

**Yêu cầu**: Linux x86_64, WebKitGTK 4.1.

#### Từ `.deb` (khuyến nghị)

```bash
wget https://github.com/LocShadowVN/VibirdBrowser/releases/latest/download/vibird-browser_amd64.deb
sudo apt install ./vibird-browser_amd64.deb
```

`apt install` tự cài dependencies:
- `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1`
- `bubblewrap`
- `gstreamer1.0-plugins-{base,good,bad,ugly}`, `gstreamer1.0-libav`

#### Từ Flatpak

```bash
flatpak install --user vibird-browser_amd64.flatpak
flatpak run io.github.locshadowvn.Vibird
```

Runtime `org.gnome.Platform` đã có WebKitGTK, GStreamer, bubblewrap.

#### Nâng cấp

Mở app → Settings → About & Updates → **Check for updates** → **Update now**.
App tải `.deb`, hỏi password, cài, tự restart.

---

### Hạn chế

#### Không sửa được

- **Google Meet, Microsoft Teams.** WebKitGTK thiếu WebCodecs và ML pipeline của Chromium.
- **DRM.** Không bundle Widevine CDM. Netflix, Spotify Web không chạy.
- **Chỉ Linux x86_64.** Không có Windows, macOS, ARM.
- **Không sync.** Dữ liệu local.

#### Đang gặp

- **Wayland.** App set `GDK_BACKEND=x11` để chạy qua XWayland. Native Wayland không ổn định
  với kiến trúc multi-webview.
- **YouTube.** Có thể crash trên một số cấu hình GPU.
- **Adblock.** Site phức tạp (SPA, redirect chain) vẫn lọt một số ads.

#### Chưa làm

Extension runtime, incognito isolated, sync, DRM, bookmark folder, reader mode, print,
PDF viewer tùy chỉnh.

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

#### Vị trí dữ liệu

- **Database**: `~/.local/share/vibird-browser/vibird_system.sqlite` (SQLite, WAL)
- **Custom rules**: `~/.local/share/vibird-browser/custom_rules.txt`
- **Downloads**: cấu hình trong Settings, mặc định `/tmp`

Nếu có `~/.local/share/caram-browser/` từ bản cũ, DB tự migrate lần đầu chạy.

#### Bảo mật

- Master password hash Argon2id. Không recover nếu quên.
- Không hardening chống memory dump. Root attacker đọc được secret trong RAM.
- Content webview chỉ có permission `core:event:default`. Command truy cập file/vault/shell
  chỉ gọi được từ `main` webview.
- Không bundle Widevine CDM.

#### Adblock

Network layer dùng WebKit `UserContentFilter`, chạy 3 bộ lọc song song. Mỗi bộ cache riêng,
build một lần/session. Cache invalidate khi file JSON thay đổi (dựa trên mtime).

JS layer inject vào mỗi trang, override các setter/fetch/XHR/WebSocket. Có URL cache
500 entry để tránh lặp rule check trên page nặng.

Whitelist cứng: OAuth, payment, captcha, CDN/font phổ biến — không bao giờ chặn.

#### Disclaimer

Phân phối theo GNU GPL-3.0, "as-is", không bảo hành. Không nên dùng làm nơi lưu
credential giá trị cao.

---

### Giấy phép

GNU General Public License v3.0. Xem [LICENSE](LICENSE).

---

<a name="english"></a>
## English

### Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Status](#status)
- [Architecture](#architecture)
- [Installation](#installation)
- [Known Limitations](#known-limitations)
- [Building from Source](#building-from-source)
- [Technical Notes](#technical-notes)
- [License](#license)

---

### Overview

Vibird is a desktop browser for Linux x86_64 that does three things:

1. **Lightweight.** Reuses the system's WebKitGTK instead of bundling its own engine.
   Runs on machines with 4GB RAM.
2. **Private.** Multi-layer adblock, WebRTC leak shield, canvas farbling, clean URLs —
   built in, no extensions required.
3. **Simple.** No sync, no telemetry, no accounts. Data stays local.

It's a secondary browser. Not a Chrome killer, not a Brave replacement. Netflix, Spotify Web,
and Google Meet don't work. It's for reading news, searching Google, browsing GitHub.

**Stack:** Tauri v2 · WebKitGTK 4.1 · Leptos 0.6 (CSR/WASM) · SQLite · GPL-3.0.

---

### Features

#### Adblock

Three independent layers:

- **Network layer** — WebKit `UserContentFilter`, runs three filter lists in parallel:
  EasyList (ads), EasyPrivacy (trackers), Fanboy Annoyance (cookie banners, popups).
  Blocks requests before the page loads. Each list's `@@` exception rules are preserved.
- **JS hooks** — Overrides `HTMLScriptElement.src`, `HTMLIFrameElement.src`, `HTMLImageElement.src`,
  `setAttribute`, `document.write`, `innerHTML`, `createElement`, `window.open`, `fetch`,
  `XMLHttpRequest.open`, `navigator.sendBeacon`, `WebSocket`. MutationObserver scans new nodes.
- **Cosmetic CSS** — Hides ad containers, cookie banners, YouTube promoted content.
  Scriptlet stubs for Google Analytics, GTM, Facebook Pixel, and common CMPs
  (`__tcfapi`, `OneTrust`, `Cookiebot`).

Site-safe whitelist: URLs under OAuth, payment (Stripe, PayPal, VNPay, MoMo, ZaloPay),
captcha (reCAPTCHA, hCaptcha, Cloudflare Turnstile), and common CDN/fonts are never blocked,
even if they match a rule.

Per-site exceptions via `vibird://shields`.

#### Privacy

- WebRTC leak shield — strips LAN IPs from SDP
- Chrome 132 UA spoof + client hints
- WebGL vendor/renderer spoof
- Canvas + AudioBuffer farbling
- Clean URL — strips 27 tracking params
- De-AMP — rewrites Google AMP to canonical

#### Vault

- Argon2id master password hash
- AES-256-GCM per record, unique nonce
- Auto-lock after 10 min idle
- Rate limit: 5 failed attempts → escalating lockout
- Zeroize key on drop

#### Downloads

- Multi-threaded 4–16 TCP via HTTP Range
- Pause / resume / cancel
- Real-time progress shelf
- Path traversal sanitization

#### Other

- Session restore, tab snoozer after 10 min idle
- Omnibox autocomplete from history + bookmarks
- Find in page with CSS Custom Highlight
- Per-origin zoom
- Context menu (VI + EN)
- Auto-update via GitHub Releases, `pkexec dpkg`, self-restart

---

### Status

#### Working

| Feature | Notes |
|---|---|
| Adblock — 3 network filters | EasyList + EasyPrivacy + Fanboy |
| Adblock — JS hooks + scriptlets | ~10 hooks + 12 scriptlet stubs |
| Cosmetic CSS | Hides ad containers, cookie banners |
| Per-site exceptions | `vibird://shields` |
| WebRTC leak shield | SDP sanitize |
| UA + WebGL spoof | Chrome 132 |
| Canvas + Audio farbling | Micro-noise |
| Clean URL + De-AMP | 27 params, AMP rewrite |
| Vault | Argon2id + AES-256-GCM |
| Downloads | Multi-thread + pause/resume/cancel |
| Auto-update | `setsid` restart |
| Session restore | SQLite |
| Tab snoozer | Hide webview after 10 min |
| Find in page | CSS Custom Highlight |
| Omnibox autocomplete | History + bookmarks |
| Context menu | VI + EN |

#### In progress

| Feature | Notes |
|---|---|
| YouTube auto-skip | Pre-roll works, mid-roll unreliable |
| Adblock coverage | SPA and redirect-chain sites still leak |

#### Not implemented

| Feature | Notes |
|---|---|
| Extension runtime | Manifest parsed, not executed |
| Isolated incognito | Label only |
| Sync | Not started |
| DRM (Widevine) | Not bundled |
| Bookmark folders & edit | Flat add/remove only |
| Reader mode | Not started |
| Print | Not started |
| PDF viewer | WebKit built-in only |

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
│    WebKitGTK native, positioned below chrome bar                 │
└──────────────────────────────────────────────────────────────────┘
              ▲                                    ▲
              │ Tauri IPC (async, JSON)            │ shared State<T>
              ▼                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Rust backend (Tauri v2)                       │
│  ShieldEngine · DownloadEngine · VaultSession · DbManager        │
│  DnsResolver · ExtensionEngine · ContentFilterState              │
└──────────────────────────────────────────────────────────────────┘
```

Web pages render in native WebKitGTK child webviews, not `<iframe>`s. Each tab has its
own webview. `hide()` to snooze, `close()` to free.

---

### Installation

**Requirements**: Linux x86_64, WebKitGTK 4.1.

#### From `.deb` (recommended)

```bash
wget https://github.com/LocShadowVN/VibirdBrowser/releases/latest/download/vibird-browser_amd64.deb
sudo apt install ./vibird-browser_amd64.deb
```

`apt install` pulls dependencies automatically:
- `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1`
- `bubblewrap`
- `gstreamer1.0-plugins-{base,good,bad,ugly}`, `gstreamer1.0-libav`

#### From Flatpak

```bash
flatpak install --user vibird-browser_amd64.flatpak
flatpak run io.github.locshadowvn.Vibird
```

`org.gnome.Platform` runtime ships WebKitGTK, GStreamer, bubblewrap.

#### Upgrading

Open app → Settings → About & Updates → **Check for updates** → **Update now**.
The app downloads the `.deb`, prompts for password, installs, restarts.

---

### Known Limitations

#### Architectural

- **Google Meet, Microsoft Teams.** WebKitGTK lacks Chromium's WebCodecs and ML pipeline.
- **DRM.** No Widevine CDM. Netflix, Spotify Web don't work.
- **Linux x86_64 only.** No Windows, macOS, ARM builds.
- **No sync.** Local data only.

#### Current issues

- **Wayland.** App sets `GDK_BACKEND=x11` to run through XWayland. Native Wayland is
  unstable with the multi-webview architecture.
- **YouTube.** May crash on some GPU configs.
- **Adblock.** Complex sites (SPAs, redirect chains) still leak some ads.

#### Not implemented

Extension runtime, isolated incognito, sync, DRM, bookmark folders, reader mode, print,
custom PDF viewer.

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

- **Database**: `~/.local/share/vibird-browser/vibird_system.sqlite` (SQLite, WAL)
- **Custom rules**: `~/.local/share/vibird-browser/custom_rules.txt`
- **Downloads**: configurable in Settings, default `/tmp`

Legacy `~/.local/share/caram-browser/` auto-migrates on first launch.

#### Security

- Master password hashed with Argon2id. No recovery path.
- No memory-dump hardening. A root attacker can read secrets from RAM.
- Content webviews only have `core:event:default` permission. File/vault/shell commands are
  only callable from the `main` webview.
- No Widevine CDM bundled.

#### Adblock

Network layer uses WebKit `UserContentFilter`, running three filter lists in parallel.
Each list is cached independently and built once per session. Cache invalidates when the
JSON file changes (based on mtime).

JS layer injects into each page, overriding setters/fetch/XHR/WebSocket. A 500-entry URL
cache avoids repeated rule checks on heavy pages.

Hard whitelist: OAuth, payment, captcha, common CDN/fonts — never blocked.

#### Disclaimer

Distributed under GNU GPL-3.0, "as-is", no warranty. Not recommended as the sole store
for high-value credentials.

---

### License

GNU General Public License v3.0. See [LICENSE](LICENSE).
