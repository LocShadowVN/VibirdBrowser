<div align="center">

<img src="src-tauri/icons/app-icon.svg" width="160" height="160" alt="Vibird Browser" />

# Vibird Browser

A lightweight, privacy-oriented desktop browser for Linux x86_64, built with Rust.  
*Trình duyệt desktop cho Linux, viết bằng Rust, tập trung vào quyền riêng tư và mức tiêu thụ tài nguyên thấp.*

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
- [Hạn chế đã biết](#hạn-chế-đã-biết)
- [So sánh với các trình duyệt khác](#so-sánh-với-các-trình-duyệt-khác)
- [Build từ mã nguồn](#build-từ-mã-nguồn)
- [Cài đặt](#cài-đặt)
- [Ghi chú kỹ thuật](#ghi-chú-kỹ-thuật)
- [Triết lý thiết kế](#triết-lý-thiết-kế)
- [Ghi nhận](#ghi-nhận)
- [Giấy phép](#giấy-phép)

---

### Giới thiệu

Vibird Browser là một trình duyệt desktop thử nghiệm dành cho Linux, được xây dựng trên ba thành phần chính:

- **Tauri v2** làm lớp runtime và IPC giữa UI và backend.
- **WebKitGTK 4.1** làm engine render, tận dụng thư viện có sẵn của hệ điều hành thay vì bundle một engine riêng.
- **Leptos 0.6 (CSR) + Rust WASM** cho toàn bộ phần giao diện điều khiển (tab bar, omnibox, settings, downloads, vault).

Mục tiêu của dự án là cung cấp một trình duyệt có mức tiêu thụ bộ nhớ thấp, không có telemetry, và có sẵn một số tính năng bảo vệ quyền riêng tư cơ bản mà không cần cài thêm extension.

Đây là dự án **thử nghiệm (MVP)**, chưa qua kiểm toán bảo mật độc lập. Xem [Ghi chú kỹ thuật](#ghi-chú-kỹ-thuật) trước khi sử dụng cho mục đích quan trọng.

---

### Kiến trúc

Hai lớp độc lập, giao tiếp qua Tauri IPC:

1. **UI layer — Leptos CSR + Rust WASM.** Chạy trong webview của cửa sổ `main`. Chịu trách nhiệm render toàn bộ browser chrome: tab strip, navigation toolbar, bookmarks strip, download shelf, các view nội bộ (`vibird://newtab`, `settings`, `vault`, `history`, `downloads`, `extensions`).

2. **Backend — Rust (Tauri v2 + Tokio).** Chạy các subsystem: adblock worker thread, download engine, SQLite persistence, Argon2id/AES-GCM vault, DoH resolver, và quản lý các webview con.

Trang web được render trong các **webview con native của WebKitGTK**, không dùng `<iframe>`. Mỗi tab có một webview riêng biệt, gắn vào subsurface của cửa sổ chính, có thể `hide()` để tạm dừng (snooze) hoặc `close()` để giải phóng tài nguyên.

```
┌──────────────────────────────────────────────────────────────────┐
│                       Main window (Tauri)                        │
├──────────────────────────────────────────────────────────────────┤
│  UI webview ("main")                                             │
│    Leptos CSR · tab strip · omnibox · settings · vault · …       │
├──────────────────────────────────────────────────────────────────┤
│  Content webviews ("tab_1", "tab_2", …)                          │
│    WebKitGTK native subsurfaces, positioned below chrome bar     │
└──────────────────────────────────────────────────────────────────┘
              ▲                                    ▲
              │ Tauri IPC (async, JSON)            │ shared State<T>
              ▼                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Rust backend (Tauri v2)                       │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐   │
│  │ ShieldEngine     │  │ DownloadEngine   │  │ VaultSession  │   │
│  │ adblock-rust     │  │ 4–16 TCP range   │  │ Argon2id +    │   │
│  │ worker thread    │  │ workers + merge  │  │ AES-256-GCM   │   │
│  └──────────────────┘  └──────────────────┘  └───────────────┘   │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐   │
│  │ DbManager        │  │ DnsResolver      │  │ ExtensionEngine│  │
│  │ rusqlite         │  │ reqwest + DoH    │  │ manifest.json │   │
│  └──────────────────┘  └──────────────────┘  └───────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

---

### Tính năng

#### Lõi chặn quảng cáo (Vibird Shield)

- Sử dụng crate [`adblock-rust`](https://github.com/brave/adblock-rust) của Brave, chạy trên một OS thread riêng để không block UI.
- Bundle sẵn EasyList, EasyPrivacy và Fanboy's Annoyance. File rules được tải và ghép tại CI, đóng gói vào installer qua `resources/rules.txt`.
- Người dùng có thể thêm rule cá nhân tại `~/.local/share/vibird-browser/custom_rules.txt`.
- Hook tầng DOM (`HTMLScriptElement.src`, `HTMLIFrameElement.src`, `WebSocket`, `fetch`, `XMLHttpRequest`) để chặn các request tới domain tracking phổ biến.
- Farbling: thêm nhiễu nhỏ vào `Canvas.toDataURL`, `getImageData`, và `AudioBuffer.getChannelData` để vô hiệu hoá một số script fingerprinting cơ bản.
- Ẩn banner cookie/GDPR bằng CSS và stub các API (`__tcfapi`, `__cmp`, `OneTrust`, `Cookiebot`).
- Cho phép bật/tắt shield theo từng domain, lưu trong bảng `site_shield_exceptions`.

#### WebRTC & tương thích

- Loại bỏ địa chỉ IP nội bộ (RFC 1918, link-local IPv6) khỏi SDP candidate trong `RTCPeerConnection.createOffer/createAnswer`.
- Gửi `User-Agent` Chrome 130 trên Linux và cung cấp `navigator.userAgentData` hợp lệ.
- Polyfill `window.chrome.runtime`, `chrome.csi()`, `chrome.loadTimes()`.

#### Tab snoozer

- Tab không hoạt động quá 10 phút sẽ bị `hide()` webview để giải phóng tài nguyên. Khác với unload, webview vẫn giữ state; chọn lại tab sẽ hiển thị lại mà không cần tải lại trang.
- Có thể snooze thủ công bằng nút `Z` trên tab chip.

#### URL cleaning & De-AMP

- Loại bỏ các tham số tracking phổ biến trước khi navigate: `utm_*`, `fbclid`, `gclid`, `gbraid`, `wbraid`, `msclkid`, `mc_eid`, `_ga`, `_gl`, `igshid`, `si`, `spm`, `mkt_tok`, và một số khác.
- Viết lại URL Google AMP (`google.com/amp/s/…`, `*.cdn.ampproject.org`) về URL canonical.

#### Download engine

- Đọc `Accept-Ranges: bytes` và `Content-Length`. Nếu server hỗ trợ, chia file thành 4–16 phần và tải song song qua HTTP Range, rồi ghép lại.
- Bắt sự kiện download của WebKitGTK và chuyển qua engine của Vibird.
- Sanitize filename (loại bỏ `..`, `/`, `\`, ký tự điều khiển), validate path đích nằm trong thư mục download sau khi canonicalize.
- Báo cáo tiến độ lên UI qua event `download-progress` mỗi 500ms.

#### Password vault

- Master password được hash bằng **Argon2id** (salt ngẫu nhiên 128-bit, tham số memory-hard mặc định của crate `argon2`).
- Derived key được giữ trong RAM trong suốt session sau khi unlock; secret của từng credential được mã hoá bằng **AES-256-GCM** (nonce ngẫu nhiên 96-bit cho mỗi record).
- Autofill một chạm khi domain khớp với record trong vault.

#### DNS-over-HTTPS

- Hỗ trợ DoH theo RFC 8484 với `application/dns-json`.
- Có công cụ đo latency DoH trong Settings.

---

### Hạn chế đã biết

Các giới hạn dưới đây là do lựa chọn kiến trúc, không phải bug:

1. **Google Meet, Microsoft Teams.** WebKitGTK không hỗ trợ đầy đủ các API WebCodecs và pipeline ML chỉ có trên Chromium (ví dụ: xoá phông nền, một số codec cụ thể). Video call có thể hoạt động nhưng chất lượng thấp hơn Chromium.
2. **DRM (Netflix, Spotify Web, Disney+).** Vibird không bundle Google Widevine CDM. Các dịch vụ DRM sẽ không phát được nếu không tự cấu hình Widevine từ nguồn ngoài.
3. **Chrome Web Store.** Chỉ hỗ trợ load extension unpacked từ thư mục có `manifest.json`. Chưa hỗ trợ cài trực tiếp từ store.
4. **Chỉ có Linux x86_64.** Chưa có build cho Windows, macOS, hay ARM.
5. **Không có sync.** Bookmarks, history, vault đều lưu local trong SQLite, không sync giữa các máy.
6. **UI đa ngôn ngữ giới hạn.** Hiện chỉ có Tiếng Việt và English, một số chuỗi vẫn hard-coded.

Bên cạnh đó, các hạn chế chung của một MVP:

- Chưa qua kiểm toán bảo mật độc lập.
- Chưa có fuzzing cho parser (adblock rules, manifest.json, HTML từ fetch).
- Chưa có test suite tự động cho các command của Tauri.

---

### So sánh với các trình duyệt khác

Bảng dưới so sánh Vibird với Brave, Chrome và Firefox trên các tiêu chí mà người dùng Linux thường quan tâm. Các đánh giá là **định tính** — không đưa số đo RAM cụ thể vì chúng phụ thuộc vào trang web, cấu hình máy và phương pháp đo, và rất dễ gây hiểu sai nếu tách khỏi ngữ cảnh.

Vibird **không** thắng ở mọi tiêu chí. Cột "Vibird" trong bảng có cả điểm mạnh và điểm yếu.

| Tiêu chí | Vibird | Brave | Chrome | Firefox |
|---|---|---|---|---|
| **Engine render** | WebKitGTK 4.1 | Blink | Blink | Gecko |
| **UI layer** | Rust (Leptos WASM) | C++ | C++ | C++/XUL |
| **RAM khi idle** | Thấp | Cao | Cao nhất | Trung bình |
| **Tương thích web** | Hạn chế | Rất tốt | Rất tốt | Tốt |
| **Hệ sinh thái extension** | Không có | Chrome Web Store | Chrome Web Store | addons.mozilla.org |
| **Sync bookmark / password** | Không | Có | Có (Google) | Có (Firefox account) |
| **DRM Widevine** | Không | Có | Có | Có (qua plugin) |
| **Adblock tích hợp** | Có (adblock-rust) | Có (Shields) | Không | Không (cần uBlock) |
| **Tải đa luồng** | Có (4–16 TCP) | Không | Không | Không |
| **Password manager** | Vault nội bộ (Argon2id + AES-GCM) | OS keychain + sync | Google Password Manager | OS keychain + sync |
| **Telemetry** | Không | Có (opt-out) | Có (khá nhiều) | Có (opt-out) |
| **Kiểm toán bảo mật độc lập** | Chưa | Có | Có | Có |
| **Số năm phát triển** | < 1 | ~8 | ~17 | ~22 |
| **Số contributor chính** | 1 | Hàng trăm | Hàng nghìn | Hàng nghìn |
| **Nền tảng hỗ trợ** | Linux x86_64 | Đa nền tảng | Đa nền tảng | Đa nền tảng |

#### Chi tiết các tiêu chí

**RAM khi idle thấp hơn.** WebKitGTK là engine dùng ít bộ nhớ hơn Blink ở trạng thái idle, chủ yếu vì Chromium spawn nhiều process con cho mỗi tab và có nhiều service chạy nền. Mức chênh lệch thực tế **không cố định** — với trang nặng (Google Docs, Figma, YouTube), khoảng cách thu hẹp đáng kể. Đừng kỳ vọng "1/5 RAM của Chrome" trong mọi trường hợp.

**Không có telemetry.** Kiểm chứng được từ source. Chrome gửi dữ liệu sử dụng về Google mặc định; Brave và Firefox có telemetry nhưng ở dạng opt-out và minh bạch hơn.

**Adblock tích hợp.** Brave và Vibird có engine adblock built-in. Chrome không có, và bị giới hạn thêm bởi Manifest V3. Firefox cần cài uBlock Origin (nhưng uBlock Origin trên Firefox vẫn mạnh hơn nhiều so với adblock built-in của Vibird).

**Tải đa luồng.** Chrome, Brave, Firefox mặc định tải 1 luồng cho mỗi file. Vibird chia thành 4–16 luồng khi server hỗ trợ `Accept-Ranges`. Đây là lợi thế rõ rệt khi tải file lớn từ server chậm, nhưng không có ý nghĩa gì khi server không hỗ trợ Range hoặc khi file nhỏ.

**Không có extension.** Đây là hạn chế lớn nhất. Không có uBlock Origin, Bitwarden, Dark Reader, React DevTools, SponsorBlock, … Người dùng phụ thuộc extension nặng sẽ thấy Vibird gần như không dùng được cho workflow hàng ngày.

**Không có sync.** Bookmark, history, password chỉ lưu local. Không đồng bộ giữa laptop và desktop. Đây là tính năng cơ bản mà cả 3 trình duyệt còn lại đều có.

**Không có DRM.** Netflix, Spotify Web, Disney+ sẽ không phát được. Không có cách workaround trong app; phải cấu hình Widevine CDM từ nguồn ngoài.

**Tương thích web kém hơn.** Các trang dùng API đặc thù Chromium (WebCodecs đầy đủ, một số API WebGPU, một số WebRTC extension, background blur, client-side ML) có thể không hoạt động hoặc hoạt động không đầy đủ. Google Meet có thể hiện warning. Một số SPA nặng render sai. Đây là trade-off cố hữu của WebKitGTK, không phải bug.

**Chưa qua kiểm toán bảo mật.** Brave, Chrome, Firefox đều có bug bounty, security team riêng, và audit định kỳ từ bên thứ ba. Vibird chưa có bất kỳ thứ nào trong số đó. Vault Argon2id + AES-GCM của Vibird về mặt cryptographic design là hợp lý, nhưng **chưa được ai kiểm tra độc lập**, và đó là khác biệt rất lớn khi so với 3 trình duyệt còn lại.

**Số lượng contributor.** 1 người vs hàng nghìn. Điều này ảnh hưởng trực tiếp đến tốc độ fix bug, độ ổn định, độ bao phủ test, và khả năng duy trì dài hạn.

#### Tóm tắt thực tế

Nếu bạn cần một trong các thứ sau, hãy dùng trình duyệt khác:

- **Extension phong phú** → Brave hoặc Firefox.
- **Sync giữa nhiều máy** → Firefox hoặc Brave.
- **DRM (Netflix, Spotify Web)** → Chrome, Brave hoặc Firefox.
- **Tương thích web tối đa, đặc biệt với Google services** → Chrome hoặc Brave.
- **Password manager tích hợp OS keychain** → Brave, Chrome, Firefox.
- **Security track record đã được chứng minh** → bất kỳ trình duyệt nào ở trên.

Vibird có thể phù hợp nếu bạn:

- Cần một trình duyệt **phụ**, nhẹ, trên máy Linux cấu hình thấp.
- Chủ yếu đọc tài liệu kỹ thuật, xem trang tĩnh, tra cứu.
- Muốn **không có telemetry** mà không phải qua các bước opt-out.
- Cần tải file lớn từ server chậm (multi-thread có lợi thế).
- Tò mò về kiến trúc Tauri + WebKitGTK + Rust và muốn thử nghiệm.

Vibird **không** được thiết kế để thay thế trình duyệt chính của bạn. Nó là một lựa chọn thay thế nhẹ, có trade-off rõ ràng và được ghi lại trung thực.

---

### Build từ mã nguồn

#### Yêu cầu hệ thống

Debian / Ubuntu / Linux Mint:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Fedora / RHEL (nếu bạn tự xử lý tên package tương ứng):

- `webkit2gtk4.1-devel`
- `openssl-devel`
- `libappindicator-gtk3-devel`
- `librsvg2-devel`

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
cargo tauri build                  # release (.deb + .AppImage)
```

Build output:

- `target/release/bundle/deb/vibird-browser_*_amd64.deb`
- `target/release/bundle/appimage/vibird-browser_*_amd64.AppImage`

Để debug rules loading:

```bash
RUST_LOG=info ./target/release/vibird-browser
```

Log kỳ vọng:

```
[INFO] Vibird Shield: loaded XXXXX external rules from ".../resources/rules.txt"
[INFO] Vibird Shield: engine initialized with XXXXX total rules
```

---

### Cài đặt

#### Debian / Ubuntu / Linux Mint (`.deb`)

```bash
wget https://github.com/LocShadowVN/VibirdBrowser/releases/latest/download/vibird-browser_amd64.deb
sudo dpkg -i vibird-browser_amd64.deb
sudo apt-get install -f
```

#### AppImage (mọi distro Linux x86_64)

```bash
wget https://github.com/LocShadowVN/VibirdBrowser/releases/latest/download/vibird-browser_amd64.AppImage
chmod +x vibird-browser_amd64.AppImage
./vibird-browser_amd64.AppImage
```

---

### Ghi chú kỹ thuật

#### Phiên bản

Dự án đang ở giai đoạn MVP. Chưa có release ổn định. Mọi API nội bộ (cấu trúc state, event name, command name của Tauri) có thể thay đổi giữa các commit.

#### Nguồn dữ liệu local

- **Database:** `~/.local/share/vibird-browser/vibird_system.sqlite` (SQLite, WAL)
- **Custom rules:** `~/.local/share/vibird-browser/custom_rules.txt`
- **Downloads:** thư mục trong Settings (mặc định `/tmp`)

Nếu bản cũ hơn dùng `caram-browser` từng tồn tại, database sẽ được copy một lần sang đường dẫn mới khi khởi động lần đầu.

#### Bảo mật

- Master password của vault được hash bằng Argon2id. Không có cách recover nếu quên.
- Không có hardening chống memory dump. Nếu kẻ tấn công có quyền root trên máy, có thể đọc được secret trong RAM.
- Content webview chạy trên các domain bên ngoài chỉ được cấp permission tối thiểu (`core:event:default`). Tất cả các command truy cập file, vault, shell chỉ gọi được từ webview `main`.
- Không bundle Widevine CDM.

#### Disclaimer

Phần mềm được phân phối theo giấy phép GNU GPL-3.0, "as-is", không kèm bảo hành. Tác giả không chịu trách nhiệm cho bất kỳ tổn thất dữ liệu nào. Không nên dùng làm nơi lưu trữ duy nhất cho credential có giá trị cao.

---

### Triết lý thiết kế

Biểu trưng của Vibird được thiết kế dựa trên họa tiết **Chim Lạc** trên trống đồng Đông Sơn, thay vì các motif phổ biến như shield/neon glow/gradient 3D.

- Chim được cách điệu thành chữ **V** bằng các polygon góc cạnh, thể hiện chuyển động hướng về phía trước.
- Đầu chim ngẩng cao, mỏ dài và nhọn, có ba dải lông mào kéo ngược về sau — theo phong cách chạm khắc trên cổ vật Đông Sơn.
- Bảng màu: vàng đồng cổ (`#D4AF37`) trên nền lacquer đen, không dùng gradient rực rỡ.

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

  <!-- Đông Sơn solar halo -->
  <circle cx="256" cy="256" r="176" fill="none" stroke="#D4AF37" stroke-width="1.5" opacity="0.15" stroke-dasharray="8, 8"/>
  <circle cx="256" cy="256" r="140" fill="none" stroke="#D4AF37" stroke-width="1" opacity="0.1"/>

  <!-- Cánh phụ -->
  <polygon points="210,310 145,210 160,150 240,240" fill="#996515" opacity="0.6"/>
  <polygon points="160,150 120,170 190,265" fill="#784E0E" opacity="0.4"/>

  <!-- Cánh chính (vế trái chữ V) -->
  <polygon points="235,395 140,230 170,120 280,270" fill="#D4AF37"/>
  <polygon points="170,120 135,145 220,295 235,395" fill="#B38622"/>

  <!-- Thân, ngực, đầu (vế phải chữ V) -->
  <path d="M 235 395 
           C 270 330, 310 260, 350 200 
           L 435 125 
           L 365 170 
           C 335 195, 305 240, 275 305 
           Z" 
        fill="#E6B800"/>

  <!-- Mỏ chim -->
  <polygon points="435,125 365,170 375,150" fill="#FFF2B2"/>

  <!-- Ba dải lông mào -->
  <polygon points="345,185 240,165 315,198" fill="#F3E5AB"/>
  <polygon points="330,200 205,185 295,215" fill="#D4AF37"/>
  <polygon points="310,218 190,208 275,235" fill="#B38622"/>

  <!-- Đuôi -->
  <polygon points="235,395 285,390 260,425" fill="#D4AF37" opacity="0.85"/>
  <polygon points="215,380 235,395 200,410" fill="#996515"/>
</svg>
```
</details>

---

### Ghi nhận

Dự án được phát triển với sự hỗ trợ của các mô hình ngôn ngữ:

- **Gemini 3.8 Flash** — hỗ trợ sinh mã, gợi ý kiến trúc và review.
- **DeepSeek v4.1 Flash** — hỗ trợ sinh mã và rà soát logic.

Mọi quyết định kiến trúc, review cuối cùng, và trách nhiệm phát hành thuộc về maintainer của dự án.

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
- [Known Limitations](#known-limitations)
- [Comparison with Other Browsers](#comparison-with-other-browsers)
- [Building from Source](#building-from-source)
- [Installation](#installation)
- [Technical Notes](#technical-notes)
- [Design Philosophy](#design-philosophy)
- [Acknowledgements](#acknowledgements)
- [License](#license)

---

### Overview

Vibird Browser is an experimental desktop browser for Linux x86_64, built on three pillars:

- **Tauri v2** as the runtime and IPC layer between the UI and the backend.
- **WebKitGTK 4.1** as the rendering engine, reusing the OS-provided library instead of shipping a bundled engine.
- **Leptos 0.6 (CSR) + Rust WASM** for the entire browser chrome (tab strip, omnibox, settings, downloads, vault).

The goal is to provide a browser with a low memory footprint, no telemetry, and a set of privacy features available out of the box without requiring third-party extensions.

This is an **experimental (MVP)** project. It has not undergone an independent security audit. See [Technical Notes](#technical-notes) before relying on it for sensitive use cases.

---

### Architecture

Two independent layers communicating over Tauri IPC:

1. **UI layer — Leptos CSR + Rust WASM.** Runs inside the `main` window's webview. Renders the browser chrome: tab strip, navigation toolbar, bookmarks strip, download shelf, and internal views (`vibird://newtab`, `settings`, `vault`, `history`, `downloads`, `extensions`).

2. **Backend — Rust (Tauri v2 + Tokio).** Hosts the adblock worker thread, download engine, SQLite persistence, Argon2id/AES-GCM vault, DoH resolver, and content webview management.

Web pages are rendered in **native WebKitGTK child webviews**, not `<iframe>`s. Each tab owns a separate webview pinned to the main window's subsurface, which can be `hide()`-den to pause (snooze) or `close()`-d to reclaim resources.

```
┌──────────────────────────────────────────────────────────────────┐
│                       Main window (Tauri)                        │
├──────────────────────────────────────────────────────────────────┤
│  UI webview ("main")                                             │
│    Leptos CSR · tab strip · omnibox · settings · vault · …       │
├──────────────────────────────────────────────────────────────────┤
│  Content webviews ("tab_1", "tab_2", …)                          │
│    WebKitGTK native subsurfaces, positioned below chrome bar     │
└──────────────────────────────────────────────────────────────────┘
              ▲                                    ▲
              │ Tauri IPC (async, JSON)            │ shared State<T>
              ▼                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Rust backend (Tauri v2)                       │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐   │
│  │ ShieldEngine     │  │ DownloadEngine   │  │ VaultSession  │   │
│  │ adblock-rust     │  │ 4–16 TCP range   │  │ Argon2id +    │   │
│  │ worker thread    │  │ workers + merge  │  │ AES-256-GCM   │   │
│  └──────────────────┘  └──────────────────┘  └───────────────┘   │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐   │
│  │ DbManager        │  │ DnsResolver      │  │ ExtensionEngine│  │
│  │ rusqlite         │  │ reqwest + DoH    │  │ manifest.json │   │
│  └──────────────────┘  └──────────────────┘  └───────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

---

### Features

#### Adblock core (Vibird Shield)

- Uses Brave's [`adblock-rust`](https://github.com/brave/adblock-rust) crate on a dedicated OS thread, so UI is not blocked during network request classification.
- Bundles EasyList, EasyPrivacy, and Fanboy's Annoyance. Rules are fetched and merged at CI time, packaged into the installer via `resources/rules.txt`.
- Custom rules supported at `~/.local/share/vibird-browser/custom_rules.txt`.
- DOM-level hooks (`HTMLScriptElement.src`, `HTMLIFrameElement.src`, `WebSocket`, `fetch`, `XMLHttpRequest`) block requests to well-known tracking domains.
- Farbling: injects imperceptible noise into `Canvas.toDataURL`, `getImageData`, and `AudioBuffer.getChannelData` to invalidate naive fingerprinting scripts.
- Hides cookie/GDPR banners via CSS and stubs consent APIs (`__tcfapi`, `__cmp`, `OneTrust`, `Cookiebot`).
- Per-domain shield toggle, persisted in the `site_shield_exceptions` table.

#### WebRTC & compatibility

- Strips RFC 1918 and link-local IPv6 addresses from SDP candidates in `RTCPeerConnection.createOffer/createAnswer`.
- Reports a Chrome 130 on Linux `User-Agent` string and exposes a valid `navigator.userAgentData`.
- Polyfills `window.chrome.runtime`, `chrome.csi()`, `chrome.loadTimes()`.

#### Tab snoozer

- Tabs idle for more than 10 minutes have their webview `hide()`-den to reclaim resources. Unlike unload, state is preserved; selecting the tab again simply re-shows the existing webview without reloading.
- Manual snooze available via the `Z` button on the tab chip.

#### URL cleaning & De-AMP

- Strips common tracking parameters before navigating: `utm_*`, `fbclid`, `gclid`, `gbraid`, `wbraid`, `msclkid`, `mc_eid`, `_ga`, `_gl`, `igshid`, `si`, `spm`, `mkt_tok`, and others.
- Rewrites Google AMP URLs (`google.com/amp/s/…`, `*.cdn.ampproject.org`) to their canonical origin.

#### Download engine

- Reads `Accept-Ranges: bytes` and `Content-Length`. If supported, splits the file into 4–16 segments and fetches in parallel via HTTP Range requests, then concatenates.
- Intercepts WebKitGTK download events and reroutes them through the Vibird engine.
- Sanitizes filenames (strips `..`, `/`, `\`, control characters) and validates the destination path against a canonicalized download directory.
- Reports progress to the UI via the `download-progress` event every 500ms.

#### Password vault

- Master password hashed with **Argon2id** (128-bit random salt, default memory-hard parameters from the `argon2` crate).
- The derived key is held in memory for the session after unlock; each stored secret is encrypted with **AES-256-GCM** (96-bit random nonce per record).
- One-click autofill when the current domain matches a vault entry.

#### DNS-over-HTTPS

- DoH support per RFC 8484 with `application/dns-json`.
- DoH latency diagnostic tool in Settings.

---

### Known Limitations

The following are architectural constraints, not bugs:

1. **Google Meet, Microsoft Teams.** WebKitGTK does not implement the full set of WebCodecs and Chromium-only ML pipelines (e.g., background blur, certain codecs). Video calls may work, but with reduced quality compared to Chromium.
2. **DRM (Netflix, Spotify Web, Disney+).** Vibird does not bundle Google Widevine CDM. DRM-protected streams will not play unless you configure Widevine from an external source.
3. **Chrome Web Store.** Only unpacked extensions with a `manifest.json` folder are supported. Direct install from the store is not implemented.
4. **Linux x86_64 only.** No Windows, macOS, or ARM builds.
5. **No sync.** Bookmarks, history, and vault are stored locally in SQLite and never synchronized across machines.
6. **Limited localization.** Only Vietnamese and English strings are provided; some strings remain hard-coded.

Additionally, as an MVP:

- No independent security audit.
- No fuzzing for parsers (adblock rules, manifest.json, fetched HTML).
- No automated test suite for Tauri commands.

---

### Comparison with Other Browsers

The table below compares Vibird against Brave, Chrome, and Firefox on criteria commonly relevant to Linux users. Assessments are **qualitative** — no specific memory numbers are given because they depend on the site, the machine, and the measurement method, and are easily misread out of context.

Vibird does **not** win on every criterion. The "Vibird" column honestly contains both strengths and weaknesses.

| Criterion | Vibird | Brave | Chrome | Firefox |
|---|---|---|---|---|
| **Rendering engine** | WebKitGTK 4.1 | Blink | Blink | Gecko |
| **UI layer** | Rust (Leptos WASM) | C++ | C++ | C++/XUL |
| **Idle RAM** | Low | High | Highest | Medium |
| **Web compatibility** | Limited | Very good | Very good | Good |
| **Extension ecosystem** | None | Chrome Web Store | Chrome Web Store | addons.mozilla.org |
| **Bookmark/password sync** | No | Yes | Yes (Google) | Yes (Firefox account) |
| **Widevine DRM** | No | Yes | Yes | Yes (via plugin) |
| **Built-in adblock** | Yes (adblock-rust) | Yes (Shields) | No | No (needs uBlock) |
| **Multi-threaded downloads** | Yes (4–16 TCP) | No | No | No |
| **Password manager** | Internal vault (Argon2id + AES-GCM) | OS keychain + sync | Google Password Manager | OS keychain + sync |
| **Telemetry** | None | Yes (opt-out) | Yes (extensive) | Yes (opt-out) |
| **Independent security audit** | None | Yes | Yes | Yes |
| **Years in development** | < 1 | ~8 | ~17 | ~22 |
| **Core contributors** | 1 | Hundreds | Thousands | Thousands |
| **Supported platforms** | Linux x86_64 | Multi-platform | Multi-platform | Multi-platform |

#### Criterion details

**Lower idle RAM.** WebKitGTK uses less memory than Blink at idle, primarily because Chromium spawns multiple child processes per tab plus background services. The actual gap is **not constant** — with heavy pages (Google Docs, Figma, YouTube), the difference narrows considerably. Do not expect "1/5 of Chrome's RAM" across the board.

**No telemetry.** Verifiable from source. Chrome sends usage data to Google by default; Brave and Firefox have telemetry but in an opt-out, more transparent form.

**Built-in adblock.** Brave and Vibird have built-in adblock engines. Chrome does not, and is further constrained by Manifest V3. Firefox requires installing uBlock Origin — which, on Firefox, is still considerably more powerful than Vibird's built-in engine.

**Multi-threaded downloads.** Chrome, Brave, and Firefox default to a single stream per file. Vibird splits into 4–16 streams when the server supports `Accept-Ranges`. This is a clear advantage for large files from slow servers, but gives no benefit when the server does not support Range or the file is small.

**No extensions.** This is the biggest limitation. No uBlock Origin, Bitwarden, Dark Reader, React DevTools, SponsorBlock, … Users with heavy extension workflows will find Vibird nearly unusable for daily use.

**No sync.** Bookmarks, history, and passwords stay local. No sync between laptop and desktop. A basic feature that all three alternatives provide.

**No DRM.** Netflix, Spotify Web, Disney+ will not play. There is no in-app workaround; Widevine CDM must be configured from an external source.

**Lower web compatibility.** Pages using Chromium-specific APIs (full WebCodecs, certain WebGPU APIs, some WebRTC extensions, background blur, client-side ML) may not work or may work incompletely. Google Meet may show a browser warning. Some heavy SPAs render incorrectly. This is an inherent trade-off of WebKitGTK, not a bug.

**No security audit.** Brave, Chrome, and Firefox all have bug bounties, dedicated security teams, and regular third-party audits. Vibird has none of these. Vibird's Argon2id + AES-GCM vault is reasonable from a cryptographic design standpoint, but has **not been independently reviewed**, and that is a large difference from the other three.

**Contributor count.** 1 person vs thousands. This directly affects fix speed, stability, test coverage, and long-term maintenance.

#### Honest summary

If you need any of the following, use another browser:

- **Rich extensions** → Brave or Firefox.
- **Sync across machines** → Firefox or Brave.
- **DRM (Netflix, Spotify Web)** → Chrome, Brave, or Firefox.
- **Maximum web compatibility, especially Google services** → Chrome or Brave.
- **Password manager integrated with OS keychain** → Brave, Chrome, or Firefox.
- **A proven security track record** → any of the above.

Vibird may be a fit if you:

- Need a **secondary** browser that is lightweight, on a low-spec Linux machine.
- Mostly read technical docs, static pages, references.
- Want **no telemetry** without having to opt out.
- Download large files from slow servers (multi-thread helps).
- Are curious about the Tauri + WebKitGTK + Rust stack and want to experiment.

Vibird is **not** designed to replace your primary browser. It is a lightweight alternative with clear trade-offs, documented honestly.

---

### Building from Source

#### System prerequisites

Debian / Ubuntu / Linux Mint:

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Fedora / RHEL (adjust package names as needed):

- `webkit2gtk4.1-devel`
- `openssl-devel`
- `libappindicator-gtk3-devel`
- `librsvg2-devel`

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
cargo tauri build                  # release (.deb + .AppImage)
```

Build output:

- `target/release/bundle/deb/vibird-browser_*_amd64.deb`
- `target/release/bundle/appimage/vibird-browser_*_amd64.AppImage`

To inspect rules loading:

```bash
RUST_LOG=info ./target/release/vibird-browser
```

Expected log:

```
[INFO] Vibird Shield: loaded XXXXX external rules from ".../resources/rules.txt"
[INFO] Vibird Shield: engine initialized with XXXXX total rules
```

---

### Installation

#### Debian / Ubuntu / Linux Mint (`.deb`)

```bash
wget https://github.com/LocShadowVN/VibirdBrowser/releases/latest/download/vibird-browser_amd64.deb
sudo dpkg -i vibird-browser_amd64.deb
sudo apt-get install -f
```

#### AppImage (any Linux x86_64 distro)

```bash
wget https://github.com/LocShadowVN/VibirdBrowser/releases/latest/download/vibird-browser_amd64.AppImage
chmod +x vibird-browser_amd64.AppImage
./vibird-browser_amd64.AppImage
```

---

### Technical Notes

#### Versioning

The project is in MVP stage. There are no stable releases. Internal APIs (state shape, event names, Tauri command names) may change between commits.

#### Local data locations

- **Database:** `~/.local/share/vibird-browser/vibird_system.sqlite` (SQLite, WAL)
- **Custom rules:** `~/.local/share/vibird-browser/custom_rules.txt`
- **Downloads:** configurable in Settings (defaults to `/tmp`)

If a legacy `caram-browser` directory exists, the database is copied once to the new path on first launch.

#### Security

- The vault master password is hashed with Argon2id. There is no recovery path if forgotten.
- No hardening against memory dumps. An attacker with root on the machine can read decrypted secrets from RAM.
- Content webviews (untrusted domains) are granted only minimal permissions (`core:event:default`). File system access, vault access, and shell commands are only callable from the `main` webview.
- No Widevine CDM is bundled.

#### Disclaimer

The software is distributed under GNU GPL-3.0 strictly "as-is", without warranty of any kind. The authors are not liable for any data loss. Not recommended as the sole store of high-value credentials.

---

### Design Philosophy

The Vibird emblem is based on the **Chim Lạc** (Lạc bird) motif found on **Đông Sơn bronze drums**, deliberately avoiding common tech-logo clichés such as shields, neon glows, and 3D bevels.

- The bird is stylized into a **V** shape using sharp polygonal facets, conveying forward motion.
- The head is raised with an elongated, spear-like bill and three crest plumes swept backward, echoing Dong Son bronze casting.
- Palette: antique brass gold (`#D4AF37`) on a deep lacquer black background, no bright gradients.

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

Development was assisted by the following language models:

- **Gemini 3.8 Flash** — code generation, architectural suggestions, and reviews.
- **DeepSeek v4.1 Flash** — code generation and logic review.

All architectural decisions, final code review, and release responsibility remain with the project maintainer.

---

### License

GNU General Public License v3.0. See [LICENSE](LICENSE).
