<div align="center">

<img src="src-tauri/icons/app-icon.svg" width="140" height="140" alt="Vibird Browser" />

# Vibird Browser

A lightweight, privacy-oriented desktop browser for Linux x86_64, built with Rust.  
*Trình duyệt desktop nhẹ, tập trung vào quyền riêng tư, viết bằng Rust cho Linux x86_64.*

[![Build](https://img.shields.io/github/actions/workflow/status/LocShadowVN/VibirdBrowser/build.yml?branch=main&style=flat-square&label=build)](https://github.com/LocShadowVN/VibirdBrowser/actions)
[![CI](https://img.shields.io/github/actions/workflow/status/LocShadowVN/VibirdBrowser/ci.yml?branch=main&style=flat-square&label=ci)](https://github.com/LocShadowVN/VibirdBrowser/actions)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20x86__64-lightgrey.svg?style=flat-square)](#installation)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg?style=flat-square)](https://www.rust-lang.org/)
[![WebKitGTK](https://img.shields.io/badge/webview-WebKitGTK%204.1-informational.svg?style=flat-square)](https://webkitgtk.org/)

[Tiếng Việt](#tiếng-việt) · [English](#english)

</div>

---

> **Trạng thái dự án**: Đây là dự án cá nhân đang phát triển, không phải sản phẩm thương mại.
> Chưa qua kiểm toán bảo mật, chưa có test suite toàn diện, và một số tính năng còn ở mức MVP.
> Dùng được cho nhu cầu đọc báo, tra cứu hàng ngày — **không khuyến nghị** dùng làm trình duyệt
> chính cho công việc quan trọng hoặc lưu trữ credential giá trị cao.
>
> **Project status**: Personal project under active development. Not a commercial product.
> Not security-audited, no comprehensive test suite, some features are MVP-level. Usable for
> casual browsing — **not recommended** as a primary browser for critical work or high-value
> credentials.

---

<a name="tiếng-việt"></a>
## Tiếng Việt

### Mục lục

- [Vibird là gì](#vibird-là-gì)
- [Trạng thái tính năng](#trạng-thái-tính-năng)
- [Kiến trúc](#kiến-trúc)
- [Cài đặt](#cài-đặt)
- [Hạn chế đã biết](#hạn-chế-đã-biết)
- [Build từ mã nguồn](#build-từ-mã-nguồn)
- [Ghi chú kỹ thuật](#ghi-chú-kỹ-thuật)
- [Giấy phép](#giấy-phép)

---

### Vibird là gì

Vibird là trình duyệt desktop thử nghiệm cho Linux x86_64, tập trung vào ba thứ:

1. **Nhẹ.** Tận dụng WebKitGTK có sẵn trên hệ điều hành thay vì bundle engine riêng.
   Mục tiêu chạy được trên máy 4GB RAM.
2. **Riêng tư mặc định.** Adblock, WebRTC leak shield, canvas farbling, clean URL —
   không cần cài extension.
3. **Đơn giản.** Không sync, không telemetry, không tài khoản. Dữ liệu lưu local.

**Không phải là**: Chrome killer, Brave replacement, trình duyệt cho Netflix/Spotify/Google Meet.
Đây là trình duyệt phụ, dùng cho đọc báo, tra Google, GitHub.

**Stack**: Tauri v2 · WebKitGTK 4.1 · Leptos 0.6 (CSR/WASM) · SQLite · GPL-3.0.

---

### Trạng thái tính năng

Bảng dưới ghi rõ cái gì **hoàn chỉnh**, cái gì **đang phát triển**, cái gì **chỉ là khung**.

#### Adblock & Privacy

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| Adblock tầng mạng | ✅ Hoạt động | WebKit `UserContentFilter` — chặn request trước khi tải |
| Adblock JS hooks | ✅ Hoạt động | Bổ sung cho lớp mạng, chặn fetch/XHR/WebSocket |
| Cosmetic CSS | ✅ Hoạt động | Ẩn ads, cookie banner, YouTube promoted |
| YouTube auto-skip | ⚠️ Một phần | Hoạt động phần lớn pre-roll, mid-roll không ổn định |
| Per-site exception | ✅ Hoạt động | Qua `vibird://shields` |
| WebRTC leak shield | ✅ Hoạt động | Strip LAN IP khỏi SDP |
| UA spoof Chrome | ✅ Hoạt động | Chrome 132 + client hints |
| Canvas / Audio farbling | ✅ Hoạt động | Nhiễu vi mô, phá hash fingerprint |
| Clean URL | ✅ Hoạt động | Strip 27 tracking params |
| De-AMP | ✅ Hoạt động | Rewrite Google AMP về canonical |

#### Browser core

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| Tab strip, omnibox, navigation | ✅ Hoạt động | |
| Session restore | ✅ Hoạt động | Lưu vào SQLite |
| Tab snoozer | ✅ Hoạt động | Ẩn webview sau 10 phút idle |
| Context menu | ✅ Hoạt động | Tiếng Việt + Anh |
| Find in page | ✅ Hoạt động | CSS Custom Highlight API |
| Zoom per-origin | ✅ Hoạt động | Lưu qua localStorage |
| Omnibox autocomplete | ✅ Hoạt động | Query history + bookmarks |
| Ctrl+click / middle-click | ✅ Hoạt động | Mở tab mới |

#### Vault, Download, Update

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| Password vault | ✅ Hoạt động | Argon2id + AES-256-GCM |
| Multi-thread download | ✅ Hoạt động | 4–16 TCP qua HTTP Range |
| Auto-update | ✅ Hoạt động | `pkexec dpkg` với dialog GUI |
| .deb packaging | ✅ Hoạt động | |
| Flatpak packaging | ✅ Hoạt động | Runtime `org.gnome.Platform` |

#### Chưa hoàn thiện

| Tính năng | Trạng thái | Ghi chú |
|---|---|---|
| Extension runtime | ❌ Chỉ parse manifest | Extension chưa execute |
| Incognito isolated profile | ❌ Chỉ có label | Chưa tách cookie/storage |
| Sync | ❌ Chưa có | Dữ liệu local-only |
| DRM (Widevine) | ❌ Không có | Netflix/Spotify Web không chạy |
| Bookmark folder & edit | ❌ Chưa có | Chỉ add/remove phẳng |
| Download pause/cancel | ❌ Chưa có | Chỉ start |
| PDF viewer | ⚠️ Dùng WebKit built-in | Chưa có UI tùy chỉnh |
| Print | ❌ Chưa có | |
| Reader mode | ❌ Chưa có | |

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
│  DnsResolver · ExtensionEngine · ContentFilterState              │
└──────────────────────────────────────────────────────────────────┘
```

Trang web render trong webview con native của WebKitGTK, không dùng `<iframe>`. Mỗi tab có webview
riêng, `hide()` để snooze hoặc `close()` để giải phóng.

---

### Cài đặt

**Yêu cầu**: Linux x86_64, WebKitGTK 4.1.

#### Từ .deb (khuyến nghị)

```bash
wget https://github.com/LocShadowVN/VibirdBrowser/releases/latest/download/vibird-browser_amd64.deb
sudo apt install ./vibird-browser_amd64.deb
```

`apt install` tự động cài dependencies:
- `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1`
- `bubblewrap` (WebKit sandbox)
- `gstreamer1.0-plugins-{base,good,bad,ugly}`, `gstreamer1.0-libav` (media decode)

#### Từ Flatpak

```bash
flatpak install --user vibird-browser_amd64.flatpak
flatpak run io.github.locshadowvn.Vibird
```

Runtime `org.gnome.Platform` đã có WebKitGTK, GStreamer, bubblewrap.

#### Nâng cấp

Mở app → Settings → About & Updates → **Check for updates** → **Update now**. App tự tải `.deb`,
hỏi password qua dialog, cài, restart.

---

### Hạn chế đã biết

**Giới hạn kiến trúc** (không phải bug, không có kế hoạch fix):

- **Google Meet, Microsoft Teams.** WebKitGTK thiếu WebCodecs và ML pipeline của Chromium.
- **DRM.** Không bundle Widevine CDM. Netflix, Spotify Web không chạy.
- **Chỉ Linux x86_64.** Không có Windows, macOS, ARM.
- **Không sync.** Bookmarks, history, vault lưu local.

**Vấn đề đang gặp**:

- **Wayland**: cần `GDK_BACKEND=x11` (đã set tự động trong app). Native Wayland vẫn không ổn định
  với multi-webview architecture.
- **YouTube**: có thể crash trên một số config GPU. Đã giảm thiểu nhưng chưa fix triệt để.
- **Adblock**: một số site phức tạp (SPA, redirect chain) vẫn lọt ads. Không phải 100%.

**Chưa làm** (xem [Trạng thái tính năng](#trạng-thái-tính-năng)):
- Extension runtime, incognito isolated profile, sync, DRM, bookmark folder, download pause,
  reader mode, print.

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

Nếu có `~/.local/share/caram-browser/` từ bản cũ, DB tự động migrate lần đầu chạy.

#### Bảo mật

- Master password hash bằng Argon2id. Không recover được nếu quên.
- Không hardening chống memory dump. Root attacker đọc được secret trong RAM.
- Content webview chỉ có permission `core:event:default`. Command truy cập file/vault/shell
  chỉ gọi được từ `main` webview.
- Không bundle Widevine CDM.

#### Adblock

Adblock chạy hai lớp:

1. **Network layer** — WebKit `UserContentFilter`. Filter build từ Easylist, cache global
   (build 1 lần/session), áp vào mỗi tab khi tạo. Chặn request trước khi trang tải.
2. **JS layer** — Inject hooks vào mỗi trang. Override `HTMLScriptElement.src`, `fetch`, `XHR`,
   `WebSocket`. MutationObserver scan DOM node mới. Cosmetic CSS ẩn ad container.

Network layer là lớp chính, JS layer là fallback cho request đã lọt qua.

#### Disclaimer

Phần mềm phân phối theo GNU GPL-3.0, "as-is", không bảo hành. Không nên dùng làm nơi lưu
credential giá trị cao.

---

### Giấy phép

GNU General Public License v3.0. Xem [LICENSE](LICENSE).

---

<a name="english"></a>
## English

### Table of Contents

- [What Vibird Is](#what-vibird-is)
- [Feature Status](#feature-status)
- [Architecture](#architecture)
- [Installation](#installation)
- [Known Limitations](#known-limitations)
- [Building from Source](#building-from-source)
- [Technical Notes](#technical-notes)
- [License](#license)

---

### What Vibird Is

Vibird is an experimental desktop browser for Linux x86_64, focused on three things:

1. **Lightweight.** Reuses the OS's WebKitGTK instead of bundling its own engine.
   Targets machines with 4GB RAM.
2. **Private by default.** Adblock, WebRTC leak shield, canvas farbling, clean URLs —
   no extensions required.
3. **Simple.** No sync, no telemetry, no accounts. Data stays local.

**Not**: a Chrome killer, a Brave replacement, or a browser for Netflix/Spotify/Google Meet.
It's a secondary browser for reading news, searching, and GitHub.

**Stack**: Tauri v2 · WebKitGTK 4.1 · Leptos 0.6 (CSR/WASM) · SQLite · GPL-3.0.

---

### Feature Status

The table below distinguishes **complete**, **in progress**, and **stub**.

#### Adblock & Privacy

| Feature | Status | Notes |
|---|---|---|
| Network-level adblock | ✅ Works | WebKit `UserContentFilter` — blocks before page load |
| JS-hook adblock | ✅ Works | Layer two for fetch/XHR/WebSocket |
| Cosmetic CSS | ✅ Works | Hides ads, cookie banners, YouTube promoted |
| YouTube auto-skip | ⚠️ Partial | Mostly works for pre-roll, mid-roll unreliable |
| Per-site exceptions | ✅ Works | Via `vibird://shields` |
| WebRTC leak shield | ✅ Works | Strips LAN IPs from SDP |
| UA spoof | ✅ Works | Chrome 132 + client hints |
| Canvas / Audio farbling | ✅ Works | Micro-noise breaks fingerprint hashes |
| Clean URL | ✅ Works | Strips 27 tracking params |
| De-AMP | ✅ Works | Rewrites Google AMP to canonical |

#### Browser Core

| Feature | Status | Notes |
|---|---|---|
| Tab strip, omnibox, navigation | ✅ Works | |
| Session restore | ✅ Works | Persisted in SQLite |
| Tab snoozer | ✅ Works | Hides webview after 10 min idle |
| Context menu | ✅ Works | Vietnamese + English |
| Find in page | ✅ Works | CSS Custom Highlight API |
| Per-origin zoom | ✅ Works | Via localStorage |
| Omnibox autocomplete | ✅ Works | Queries history + bookmarks |
| Ctrl+click / middle-click | ✅ Works | Opens new tab |

#### Vault, Download, Update

| Feature | Status | Notes |
|---|---|---|
| Password vault | ✅ Works | Argon2id + AES-256-GCM |
| Multi-thread download | ✅ Works | 4–16 TCP via HTTP Range |
| Auto-update | ✅ Works | `pkexec dpkg` with GUI dialog |
| .deb packaging | ✅ Works | |
| Flatpak packaging | ✅ Works | `org.gnome.Platform` runtime |

#### Not Yet Complete

| Feature | Status | Notes |
|---|---|---|
| Extension runtime | ❌ Manifest only | Extensions don't execute |
| Isolated incognito | ❌ Label only | No cookie/storage isolation |
| Sync | ❌ Not started | Data local-only |
| DRM (Widevine) | ❌ Not bundled | Netflix/Spotify Web won't work |
| Bookmark folder & edit | ❌ Not started | Flat add/remove only |
| Download pause/cancel | ❌ Not started | Start only |
| PDF viewer | ⚠️ WebKit built-in | No custom UI |
| Print | ❌ Not started | |
| Reader mode | ❌ Not started | |

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
│  DnsResolver · ExtensionEngine · ContentFilterState              │
└──────────────────────────────────────────────────────────────────┘
```

Web pages render in native WebKitGTK child webviews, not `<iframe>`s. Each tab has its own
webview; `hide()` to snooze, `close()` to free.

---

### Installation

**Requirements**: Linux x86_64, WebKitGTK 4.1.

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

`org.gnome.Platform` runtime ships WebKitGTK, GStreamer, bubblewrap.

#### Upgrading

Open app → Settings → About & Updates → **Check for updates** → **Update now**. Downloads `.deb`,
prompts for password via GUI dialog, installs, restarts.

---

### Known Limitations

**Architectural** (not bugs, no plans to fix):

- **Google Meet, Microsoft Teams.** WebKitGTK lacks Chromium's WebCodecs and ML pipeline.
- **DRM.** No Widevine CDM. Netflix, Spotify Web don't work.
- **Linux x86_64 only.** No Windows, macOS, ARM builds.
- **No sync.** Bookmarks, history, vault are local-only.

**Current issues**:

- **Wayland**: requires `GDK_BACKEND=x11` (set automatically by the app). Native Wayland remains
  unstable with the multi-webview architecture.
- **YouTube**: may crash on some GPU configs. Mitigated but not fully fixed.
- **Adblock**: some complex sites (SPAs, redirect chains) still leak ads. Not 100%.

**Not implemented** (see [Feature Status](#feature-status)):
- Extension runtime, isolated incognito, sync, DRM, bookmark folders, download pause, reader mode,
  print.

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
- Content webviews only have `core:event:default` permission. File/vault/shell commands are only
  callable from the `main` webview.
- No Widevine CDM bundled.

#### Adblock

Two layers:

1. **Network layer** — WebKit `UserContentFilter`. Filter built from Easylist, cached globally
   (built once per session), applied to each tab on creation. Blocks requests before page load.
2. **JS layer** — Injected hooks per page. Overrides `HTMLScriptElement.src`, `fetch`, `XHR`,
   `WebSocket`. MutationObserver scans new DOM nodes. Cosmetic CSS hides ad containers.

The network layer is primary; the JS layer catches what slips through.

#### Disclaimer

Distributed under GNU GPL-3.0 strictly "as-is", no warranty. Not recommended as a sole store for
high-value credentials.

---

### License

GNU General Public License v3.0. See [LICENSE](LICENSE).
