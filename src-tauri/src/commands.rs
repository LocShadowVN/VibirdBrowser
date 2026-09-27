use crate::adblock::ShieldEngine;
use crate::crypto::CryptoEngine;
use crate::database::DbManager;
use crate::dns::DnsResolver;
use crate::downloader::DownloadEngine;
use crate::extensions::ExtensionEngine;
use serde::{Deserialize, Serialize};
use shared::{
    AppConfig, BookmarkRecord, DecryptedVaultRecord, DnsTestResult, DownloadRecord, ExtensionItem,
    HistoryRecord, PageContentResponse, ShieldLevel, ShieldStats, ShieldVerdict, SiteCredential,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    webview::{DownloadEvent, PageLoadEvent, WebviewBuilder},
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalSize, State, Webview,
    WebviewUrl,
};
use zeroize::Zeroize;

// tabs-strip 42 + nav-bar 48 + bookmarks-strip 28 = 118px
pub const NAV_BAR_HEIGHT: f64 = 118.0;

// ============================================================================
// SECURITY: UI-chrome permission guard
// ============================================================================

fn ensure_ui_chrome(webview: &Webview) -> Result<(), String> {
    if webview.label() != "main" {
        return Err(format!(
            "Forbidden: this command is UI-chrome only (caller: {})",
            webview.label()
        ));
    }
    Ok(())
}

// ============================================================================
// VAULT SESSION (rate limit + auto-lock + zeroize)
// ============================================================================

pub const VAULT_LOCK_TIMEOUT_SECS: u64 = 600;
pub const VAULT_MAX_FAILED_ATTEMPTS: u32 = 5;
pub const VAULT_LOCKOUT_BASE_SECS: u64 = 30;
pub const AUTOFILL_MIN_INTERVAL_MS: u128 = 300;

#[derive(Default)]
struct VaultInner {
    derived_key: Option<[u8; 32]>,
    last_activity: Option<Instant>,
    failed_attempts: u32,
    locked_until: Option<Instant>,
}

impl VaultInner {
    fn clear_key(&mut self) {
        if let Some(ref mut key) = self.derived_key {
            key.zeroize();
        }
        self.derived_key = None;
        self.last_activity = None;
    }
}

impl Drop for VaultInner {
    fn drop(&mut self) {
        self.clear_key();
    }
}

pub struct VaultSession {
    inner: Mutex<VaultInner>,
    last_autofill: Mutex<Instant>,
}

impl VaultSession {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(VaultInner::default()),
            last_autofill: Mutex::new(Instant::now() - Duration::from_secs(1)),
        }
    }

    fn auto_lock_check(inner: &mut VaultInner) {
        if let Some(last) = inner.last_activity {
            if Instant::now().duration_since(last).as_secs() > VAULT_LOCK_TIMEOUT_SECS {
                inner.clear_key();
            }
        }
    }

    pub fn is_locked_out(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "vault mutex poisoned".to_string())?;

        if let Some(until) = inner.locked_until {
            if Instant::now() < until {
                let rem = until.duration_since(Instant::now()).as_secs() + 1;
                return Err(format!(
                    "Vault temporarily locked after too many failed attempts. Try again in {}s",
                    rem
                ));
            }
            inner.locked_until = None;
            inner.failed_attempts = 0;
        }
        Ok(())
    }

    pub fn get_key(&self) -> Option<[u8; 32]> {
        let mut inner = self.inner.lock().ok()?;
        Self::auto_lock_check(&mut inner);
        let key = inner.derived_key;
        if key.is_some() {
            inner.last_activity = Some(Instant::now());
        }
        key
    }

    pub fn mark_success(&self, key: [u8; 32]) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.clear_key();
            inner.derived_key = Some(key);
            inner.last_activity = Some(Instant::now());
            inner.failed_attempts = 0;
            inner.locked_until = None;
        }
    }

    pub fn mark_failure(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.failed_attempts = inner.failed_attempts.saturating_add(1);
            if inner.failed_attempts >= VAULT_MAX_FAILED_ATTEMPTS {
                let cooldown = VAULT_LOCKOUT_BASE_SECS * (inner.failed_attempts as u64);
                inner.locked_until = Some(Instant::now() + Duration::from_secs(cooldown));
            }
        }
    }

    pub fn lock(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.clear_key();
        }
    }

    pub fn throttle_autofill(&self) -> Result<(), String> {
        let mut last = self
            .last_autofill
            .lock()
            .map_err(|_| "autofill throttle poisoned".to_string())?;
        let elapsed = last.elapsed().as_millis();
        if elapsed < AUTOFILL_MIN_INTERVAL_MS {
            return Err(format!(
                "Too many autofill requests. Wait {}ms.",
                AUTOFILL_MIN_INTERVAL_MS.saturating_sub(elapsed)
            ));
        }
        *last = Instant::now();
        Ok(())
    }
}

// ============================================================================
// VIEWPORT MANAGER
// ============================================================================

pub struct ViewportManager {
    pub active_tab: Mutex<String>,
    pub is_internal: Mutex<bool>,
    pub menu_expanded: Mutex<bool>,
}

impl ViewportManager {
    pub fn new() -> Self {
        Self {
            active_tab: Mutex::new(String::new()),
            is_internal: Mutex::new(true),
            menu_expanded: Mutex::new(false),
        }
    }
}

// ============================================================================
// SERIALISED PAYLOADS
// ============================================================================

#[derive(Clone, Serialize)]
pub struct PageNavigationState {
    pub tab_id: String,
    pub url: String,
    pub title: Option<String>,
    pub is_loading: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_notes: String,
    pub download_url: String,
    pub asset_name: String,
    pub is_appimage: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SessionTab {
    pub url: String,
    pub title: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SessionSnapshot {
    pub tabs: Vec<SessionTab>,
    pub active_index: usize,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GitHubAsset>,
}

// ============================================================================
// PURE HELPERS
// ============================================================================

fn is_newer_version(latest: &str, current: &str) -> bool {
    let parse_v = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };
    parse_v(latest) > parse_v(current)
}

pub fn de_amp_url(url_str: &str) -> String {
    if let Ok(u) = url::Url::parse(url_str) {
        if u.host_str() == Some("www.google.com") && u.path().starts_with("/amp/s/") {
            let real_url = &u.path()["/amp/s/".len()..];
            let scheme = if real_url.starts_with("http") { "" } else { "https://" };
            return format!("{}{}", scheme, real_url);
        }
        if let Some(host) = u.host_str() {
            if host.ends_with(".cdn.ampproject.org") {
                if let Some(pos) = u.path().find("/s/") {
                    let real_url = &u.path()[pos + 3..];
                    return format!("https://{}", real_url);
                }
            }
        }
    }
    url_str.to_string()
}

pub fn strip_tracking_parameters(url_str: &str) -> String {
    let de_amped = de_amp_url(url_str);
    let Ok(mut parsed_url) = url::Url::parse(&de_amped) else {
        return de_amped;
    };

    if parsed_url.query().is_none() {
        return parsed_url.to_string();
    }

    const TRACKING_KEYS: &[&str] = &[
        "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content",
        "utm_id", "utm_source_platform", "utm_creative",
        "fbclid", "gclid", "gbraid", "wbraid", "msclkid",
        "mc_eid", "_ga", "_gl", "yclid", "igshid", "si", "ref_src", "ref_url",
        "dclid", "twclid", "spm", "_hsenc", "_hsmi", "mkt_tok",
    ];

    let clean_pairs: Vec<(String, String)> = parsed_url
        .query_pairs()
        .filter(|(k, _)| !TRACKING_KEYS.contains(&k.as_ref()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    parsed_url.set_query(None);
    if !clean_pairs.is_empty() {
        let mut serializer = parsed_url.query_pairs_mut();
        for (k, v) in clean_pairs {
            serializer.append_pair(&k, &v);
        }
    }

    parsed_url.to_string()
}

fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// ============================================================================
// WINDOW RESIZE
// ============================================================================

pub async fn handle_window_resize(
    app: &AppHandle,
    phys_size: PhysicalSize<u32>,
) -> Result<(), String> {
    let window = app.get_window("main").ok_or("Main window not found")?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let logical = phys_size.to_logical::<f64>(scale);

    let vp_state = app.state::<ViewportManager>();
    let is_internal = *vp_state.is_internal.lock().unwrap();
    let menu_expanded = *vp_state.menu_expanded.lock().unwrap();
    let active_id = vp_state.active_tab.lock().unwrap().clone();

    if let Some(ui_wv) = app.get_webview("main").or_else(|| app.get_webview("ui_chrome")) {
        let ui_height = if is_internal || menu_expanded {
            logical.height
        } else {
            NAV_BAR_HEIGHT
        };
        let _ = ui_wv.set_size(LogicalSize::new(logical.width, ui_height));
    }

    if !is_internal && !active_id.is_empty() {
        if let Some(content_wv) = app.get_webview(&active_id) {
            let content_height = (logical.height - NAV_BAR_HEIGHT).max(100.0);
            let _ = content_wv.set_position(LogicalPosition::new(0.0, NAV_BAR_HEIGHT));
            let _ = content_wv.set_size(LogicalSize::new(logical.width, content_height));
        }
    }

    Ok(())
}

// ============================================================================
// COMMANDS
// ============================================================================

#[tauri::command]
pub fn get_app_version(webview: Webview, app: AppHandle) -> Result<String, String> {
    ensure_ui_chrome(&webview)?;
    Ok(app.package_info().version.to_string())
}

#[tauri::command]
pub async fn check_for_updates(webview: Webview, app: AppHandle) -> Result<UpdateInfo, String> {
    ensure_ui_chrome(&webview)?;
    let current_version = app.package_info().version.to_string();
    let is_appimage = std::env::var("APPIMAGE").is_ok();

    let client = reqwest::Client::builder()
        .user_agent("VibirdBrowser-Updater")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get("https://api.github.com/repos/LocShadowVN/VibirdBrowser/releases/latest")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let release: GitHubRelease = resp.json().await.map_err(|e| e.to_string())?;
    let latest_version = release.tag_name.trim_start_matches('v').to_string();
    let has_update = is_newer_version(&latest_version, &current_version);

    let target_ext = if is_appimage { ".AppImage" } else { ".deb" };
    let matched_asset = release.assets.into_iter().find(|a| a.name.ends_with(target_ext));

    let (download_url, asset_name) = match matched_asset {
        Some(a) => (a.browser_download_url, a.name),
        None => (String::new(), String::new()),
    };

    Ok(UpdateInfo {
        current_version,
        latest_version,
        has_update,
        release_notes: release.body.unwrap_or_default(),
        download_url,
        asset_name,
        is_appimage,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn apply_update(
    webview: Webview,
    db: State<'_, DbManager>,
    download_url: String,
    asset_name: String,
) -> Result<String, String> {
    ensure_ui_chrome(&webview)?;
    if download_url.is_empty() {
        return Err("ERR_NO_URL".into());
    }

    let client = reqwest::Client::builder()
        .user_agent("VibirdBrowser-Updater")
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(&download_url).send().await.map_err(|e| e.to_string())?;
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;

    if let Ok(appimage_path) = std::env::var("APPIMAGE") {
        let current_path = PathBuf::from(&appimage_path);
        let temp_path = current_path.with_extension("new");

        tokio::fs::write(&temp_path, &bytes)
            .await
            .map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755));
        }

        std::fs::rename(&temp_path, &current_path).map_err(|e| e.to_string())?;
        return Ok("SUCCESS_APPIMAGE".into());
    }

    let cfg = db.load_config();
    let save_dir = PathBuf::from(&cfg.download_path);
    tokio::fs::create_dir_all(&save_dir)
        .await
        .map_err(|e| e.to_string())?;
    let target_file = save_dir.join(&asset_name);

    tokio::fs::write(&target_file, &bytes)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("SUCCESS_DEB:{}", target_file.display()))
}

#[tauri::command]
pub fn restart_browser(webview: Webview, app: AppHandle) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    if let Ok(appimage_path) = std::env::var("APPIMAGE") {
        let _ = Command::new(appimage_path).spawn();
    }
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn start_multithread_download(
    webview: Webview,
    app: AppHandle,
    db: State<'_, DbManager>,
    url: String,
    connections: Option<usize>,
) -> Result<String, String> {
    ensure_ui_chrome(&webview)?;
    let config = db.load_config();
    let save_dir = PathBuf::from(&config.download_path);
    let _ = tokio::fs::create_dir_all(&save_dir).await;

    DownloadEngine::start_download(app, url, save_dir, None, connections.unwrap_or(8)).await
}

#[tauri::command]
pub fn check_vault_credentials_for_domain(
    webview: Webview,
    db: State<'_, DbManager>,
    session: State<'_, VaultSession>,
    domain: String,
) -> Result<Vec<SiteCredential>, String> {
    ensure_ui_chrome(&webview)?;

    let Some(key) = session.get_key() else {
        return Ok(Vec::new());
    };

    let rows = db.list_vault_rows().map_err(|e| e.to_string())?;
    let mut matches = Vec::new();

    for r in rows {
        if r.website.to_lowercase().contains(&domain.to_lowercase()) {
            if let Ok(secret) = CryptoEngine::decrypt_with_derived_key(&key, &r.ciphertext, &r.nonce)
            {
                matches.push(SiteCredential {
                    username: r.username,
                    secret,
                });
            }
        }
    }

    Ok(matches)
}

#[tauri::command]
pub async fn execute_autofill(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
    session: State<'_, VaultSession>,
    username: String,
    secret: String,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    session.throttle_autofill()?;

    let active_id = vp.active_tab.lock().unwrap().clone();
    if active_id.is_empty() {
        return Err("No active tab".into());
    }

    let Some(wv) = app.get_webview(&active_id) else {
        return Err("Webview not found".into());
    };

    let payload = serde_json::json!({ "u": username, "p": secret }).to_string();

    let eval_script = format!(
        r#"(function(){{
            if (typeof window.__VIBIRD_AUTOFILL !== 'function') return false;
            const data = {};
            return window.__VIBIRD_AUTOFILL(data.u, data.p);
        }})()"#,
        payload
    );

    wv.eval(&eval_script).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn webview_go_back(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    let active_id = vp.active_tab.lock().unwrap().clone();
    if let Some(wv) = app.get_webview(&active_id) {
        wv.eval("window.history.back()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn webview_go_forward(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    let active_id = vp.active_tab.lock().unwrap().clone();
    if let Some(wv) = app.get_webview(&active_id) {
        wv.eval("window.history.forward()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn webview_reload(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
    hard: bool,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    let _ = hard;
    let active_id = vp.active_tab.lock().unwrap().clone();
    if let Some(wv) = app.get_webview(&active_id) {
        wv.eval("window.location.reload()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn webview_zoom_by(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
    delta: f64,
    reset: bool,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;

    let active_id = vp.active_tab.lock().unwrap().clone();
    if active_id.is_empty() {
        return Err("No active tab".into());
    }

    let Some(wv) = app.get_webview(&active_id) else {
        return Err("Webview not found".into());
    };

    let reset_js = if reset { "true" } else { "false" };
    let delta_js = format!("{:.4}", delta);

    let script = format!(
        r#"(function(){{
            try {{
                var raw = localStorage.getItem('__vibird_zoom') || '1';
                var cur = parseFloat(raw);
                if (isNaN(cur) || cur < 0.3 || cur > 3.0) cur = 1;
                var next = {reset_js} ? 1.0 : Math.max(0.3, Math.min(3.0, cur + {delta_js}));
                next = Math.round(next * 10) / 10;
                document.documentElement.style.zoom = String(next);
                localStorage.setItem('__vibird_zoom', String(next));
            }} catch (e) {{}}
        }})();"#,
        reset_js = reset_js,
        delta_js = delta_js,
    );

    wv.eval(&script).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn find_in_page(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
    query: String,
    forward: bool,
) -> Result<bool, String> {
    ensure_ui_chrome(&webview)?;
    let active_id = vp.active_tab.lock().unwrap().clone();
    let Some(wv) = app.get_webview(&active_id) else {
        return Ok(false);
    };

    let safe_query = serde_json::to_string(&query).map_err(|e| e.to_string())?;
    let backwards = !forward;

    let eval_script = format!(
        r#"window.find({}, false, {}, true, false, true, false);"#,
        safe_query, backwards
    );

    wv.eval(&eval_script).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub async fn clear_site_data(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    let active_id = vp.active_tab.lock().unwrap().clone();
    let Some(wv) = app.get_webview(&active_id) else {
        return Err("No active webview".into());
    };

    let script = r#"
        try {
            localStorage.clear();
            sessionStorage.clear();
            document.cookie.split(";").forEach(function(c) {
                document.cookie = c.replace(/^ +/, "").replace(/=.*/, "=;expires=" + new Date().toUTCString() + ";path=/");
            });
            window.location.reload();
        } catch (e) {}
    "#;

    wv.eval(script).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn report_tab_title(
    webview: Webview,
    app: AppHandle,
    db: State<'_, DbManager>,
    tab_id: String,
    title: String,
    url: String,
) -> Result<(), String> {
    let caller = webview.label();
    if caller != tab_id {
        log::warn!(
            "report_tab_title spoof attempt: caller={}, claimed={}",
            caller,
            tab_id
        );
        return Err(format!(
            "Forbidden: tab_id mismatch (caller: {}, claimed: {})",
            caller, tab_id
        ));
    }

    let title_t = title.trim();
    let url_t = url.trim();
    if title_t.is_empty() || url_t.is_empty() {
        return Ok(());
    }

    let is_http = url_t.starts_with("http://") || url_t.starts_with("https://");
    let title_trunc = truncate_utf8(title_t, 512);
    let url_trunc = truncate_utf8(url_t, 4096);

    if is_http {
        let _ = db.update_history_title(url_trunc, title_trunc);
    }

    let _ = app.emit(
        "tab-navigation-state",
        PageNavigationState {
            tab_id,
            url: url_trunc.to_string(),
            title: Some(title_trunc.to_string()),
            is_loading: false,
        },
    );
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_native_tab(
    webview: Webview,
    app: AppHandle,
    shield: State<'_, ShieldEngine>,
    vp: State<'_, ViewportManager>,
    db: State<'_, DbManager>,
    tab_id: String,
    url: String,
    is_incognito: Option<bool>,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;

    let window = app.get_window("main").ok_or("Main window not found")?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let phys_size = window.inner_size().unwrap_or(PhysicalSize::new(1400, 900));
    let logical = phys_size.to_logical::<f64>(scale);

    let clean_url = strip_tracking_parameters(&url);
    let parsed_url = url::Url::parse(&clean_url).map_err(|e| e.to_string())?;

    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err("Blocked insecure URL protocol".into());
    }

    let domain = parsed_url.host_str().unwrap_or("").to_string();
    let shield_enabled = db.get_site_shield_status(&domain).unwrap_or(true);
    let incognito = is_incognito.unwrap_or(false);

    {
        let mut act = vp.active_tab.lock().unwrap();
        *act = tab_id.clone();
        let mut internal = vp.is_internal.lock().unwrap();
        *internal = false;
        let mut menu = vp.menu_expanded.lock().unwrap();
        *menu = false;
    }

    if let Some(ui_wv) = app.get_webview("main").or_else(|| app.get_webview("ui_chrome")) {
        let _ = ui_wv.set_size(LogicalSize::new(logical.width, NAV_BAR_HEIGHT));
    }

    let content_height = (logical.height - NAV_BAR_HEIGHT).max(100.0);
    let content_pos = LogicalPosition::new(0.0, NAV_BAR_HEIGHT);
    let content_size = LogicalSize::new(logical.width, content_height);

    if let Some(wv) = app.get_webview(&tab_id) {
        let _ = wv.set_position(content_pos);
        let _ = wv.set_size(content_size);
        let _ = wv.show();
        let _ = wv.set_focus();
        let current = wv.url().map(|u| u.to_string()).unwrap_or_default();
        if current != clean_url {
            wv.navigate(parsed_url).map_err(|e| e.to_string())?;
        }
    } else {
        let mut combined = format!(
            "window.__VIBIRD_TAB_ID = {};\n{}\n{}\n",
            serde_json::to_string(&tab_id).unwrap_or_else(|_| "\"\"".into()),
            crate::bridge::get_webbridge_script(),
            crate::bridge::get_autofill_script(),
        );
        if shield_enabled {
            combined.push_str(&shield.get_injected_script());
        }

        combined.push_str(
            r#"
(function() {
    // Zoom restore (per-origin via localStorage)
    function initZoom() {
        try {
            var z = localStorage.getItem('__vibird_zoom');
            if (z && z !== '1' && z !== '1.0') {
                document.documentElement.style.zoom = z;
            }
        } catch (e) {}
    }
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initZoom);
    } else {
        initZoom();
    }

    // Ctrl+Click / Cmd+Click -> open new foreground tab
    document.addEventListener('click', function(e) {
        if (!(e.ctrlKey || e.metaKey)) return;
        if (e.button !== 0) return;
        var t = e.target;
        if (!t || !t.closest) return;
        var a = t.closest('a[href]');
        if (!a) return;
        e.preventDefault();
        e.stopPropagation();
        try {
            if (window.__TAURI__ && window.__TAURI__.event) {
                window.__TAURI__.event.emit('open-new-tab', { url: a.href, background: false });
            }
        } catch (err) {}
    }, true);

    // Middle-click -> open new background tab
    document.addEventListener('auxclick', function(e) {
        if (e.button !== 1) return;
        var t = e.target;
        if (!t || !t.closest) return;
        var a = t.closest('a[href]');
        if (!a) return;
        e.preventDefault();
        e.stopPropagation();
        try {
            if (window.__TAURI__ && window.__TAURI__.event) {
                window.__TAURI__.event.emit('open-new-tab', { url: a.href, background: true });
            }
        } catch (err) {}
    }, true);
})();
"#,
        );

        let tab_id_json = serde_json::to_string(&tab_id).unwrap_or_else(|_| "\"\"".into());

        let init_script = format!(
            r#"
            {}
            (function() {{
                const TAB_ID = {};
                function reportTitle() {{
                    if (window.__TAURI__ && window.__TAURI__.core) {{
                        window.__TAURI__.core.invoke('report_tab_title', {{
                            tabId: TAB_ID,
                            title: document.title || window.location.hostname,
                            url: window.location.href
                        }}).catch(() => {{}});
                    }}
                }}
                const startObserver = () => {{
                    const t = document.querySelector('title') || document.head || document.documentElement;
                    if (!t) return;
                    try {{
                        new MutationObserver(reportTitle).observe(t, {{
                            subtree: true, characterData: true, childList: true
                        }});
                    }} catch (e) {{}}
                }};
                if (document.readyState === 'loading') {{
                    document.addEventListener('DOMContentLoaded', function() {{
                        reportTitle();
                        startObserver();
                    }});
                }} else {{
                    reportTitle();
                    startObserver();
                }}
                window.addEventListener('load', reportTitle);
            }})();
            "#,
            combined, tab_id_json,
        );

        let app_handle_for_events = app.clone();
        let tab_id_for_events = tab_id.clone();
        let app_handle_for_dl = app.clone();

        let wv_builder = WebviewBuilder::new(&tab_id, WebviewUrl::External(parsed_url))
            .user_agent(crate::bridge::CHROME_USER_AGENT)
            .initialization_script(&init_script)
            .on_page_load(move |_wv, payload| {
                let current_url = payload.url().to_string();
                let is_loading = payload.event() == PageLoadEvent::Started;
                let _ = app_handle_for_events.emit(
                    "tab-navigation-state",
                    PageNavigationState {
                        tab_id: tab_id_for_events.clone(),
                        url: current_url,
                        title: None,
                        is_loading,
                    },
                );
            })
            .on_download(move |_wv, event| {
                match event {
                    DownloadEvent::Requested { url, .. } => {
                        let dl_url = url.to_string();
                        let app_c = app_handle_for_dl.clone();
                        tauri::async_runtime::spawn(async move {
                            let db_c = app_c.state::<DbManager>();
                            let cfg = db_c.load_config();
                            let s_dir = PathBuf::from(&cfg.download_path);
                            let _ =
                                DownloadEngine::start_download(app_c, dl_url, s_dir, None, 8).await;
                        });
                        false
                    }
                    _ => true,
                }
            });

        let wv = window
            .add_child(wv_builder, content_pos, content_size)
            .map_err(|e| e.to_string())?;
        let _ = wv.set_focus();
    }

    if !incognito {
        let _ = db.insert_history(&clean_url, &clean_url);
    }

    Ok(())
}

#[tauri::command]
pub fn get_site_shield(
    webview: Webview,
    db: State<'_, DbManager>,
    domain: String,
) -> Result<bool, String> {
    ensure_ui_chrome(&webview)?;
    db.get_site_shield_status(&domain).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn toggle_site_shield(
    webview: Webview,
    app: AppHandle,
    db: State<'_, DbManager>,
    vp: State<'_, ViewportManager>,
    domain: String,
    enabled: bool,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.set_site_shield_status(&domain, enabled)
        .map_err(|e| e.to_string())?;

    let active_id = vp.active_tab.lock().unwrap().clone();
    if !active_id.is_empty() {
        if let Some(wv) = app.get_webview(&active_id) {
            if let Ok(cur_url) = wv.url() {
                if cur_url.host_str() == Some(&domain) {
                    let _ = wv.navigate(cur_url);
                }
            }
        }
    }

    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn switch_tab_view(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
    active_tab_id: String,
    is_internal: bool,
    all_tab_ids: Vec<String>,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;

    let window = app.get_window("main").ok_or("Main window not found")?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let phys_size = window.inner_size().unwrap_or(PhysicalSize::new(1400, 900));
    let logical = phys_size.to_logical::<f64>(scale);

    {
        let mut act = vp.active_tab.lock().unwrap();
        *act = active_tab_id.clone();
        let mut internal = vp.is_internal.lock().unwrap();
        *internal = is_internal;
        let mut menu = vp.menu_expanded.lock().unwrap();
        *menu = false;
    }

    if let Some(ui_wv) = app.get_webview("main").or_else(|| app.get_webview("ui_chrome")) {
        let ui_height = if is_internal { logical.height } else { NAV_BAR_HEIGHT };
        let _ = ui_wv.set_size(LogicalSize::new(logical.width, ui_height));
    }

    for id in all_tab_ids {
        if let Some(wv) = app.get_webview(&id) {
            if !is_internal && id == active_tab_id {
                let content_height = (logical.height - NAV_BAR_HEIGHT).max(100.0);
                let _ = wv.set_position(LogicalPosition::new(0.0, NAV_BAR_HEIGHT));
                let _ = wv.set_size(LogicalSize::new(logical.width, content_height));
                let _ = wv.show();
                let _ = wv.set_focus();
            } else {
                let _ = wv.hide();
            }
        }
    }

    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn close_native_tab(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
    tab_id: String,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    if let Some(wv) = app.get_webview(&tab_id) {
        let _ = wv.close();
    }
    let mut act = vp.active_tab.lock().unwrap();
    if *act == tab_id {
        act.clear();
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn snooze_tab(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
    tab_id: String,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    let active_id = vp.active_tab.lock().unwrap().clone();
    if active_id == tab_id {
        return Err("Cannot snooze the active tab".into());
    }

    if let Some(wv) = app.get_webview(&tab_id) {
        let _ = wv.hide();
    }

    Ok(())
}

#[tauri::command]
pub async fn expand_ui_for_menu(
    webview: Webview,
    app: AppHandle,
    vp: State<'_, ViewportManager>,
    expanded: bool,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;

    let window = app.get_window("main").ok_or("Main window not found")?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let phys_size = window.inner_size().unwrap_or(PhysicalSize::new(1400, 900));
    let logical = phys_size.to_logical::<f64>(scale);

    let is_internal = *vp.is_internal.lock().unwrap();
    {
        let mut menu = vp.menu_expanded.lock().unwrap();
        *menu = expanded;
    }

    if let Some(ui_wv) = app.get_webview("main").or_else(|| app.get_webview("ui_chrome")) {
        let ui_height = if is_internal || expanded {
            logical.height
        } else {
            NAV_BAR_HEIGHT
        };
        let _ = ui_wv.set_size(LogicalSize::new(logical.width, ui_height));
    }

    Ok(())
}

#[tauri::command]
pub async fn check_shield(
    webview: Webview,
    shield: State<'_, ShieldEngine>,
    target: String,
    host: String,
) -> Result<ShieldVerdict, String> {
    ensure_ui_chrome(&webview)?;
    Ok(shield.inspect_url(&target, &host).await)
}

#[tauri::command]
pub fn set_shield_level(
    webview: Webview,
    shield: State<'_, ShieldEngine>,
    level: String,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    let mode = match level.as_str() {
        "Off" => ShieldLevel::Off,
        "Aggressive" => ShieldLevel::Aggressive,
        _ => ShieldLevel::Standard,
    };
    shield.set_level(mode);
    Ok(())
}

#[tauri::command]
pub fn resolve_url(webview: Webview, raw: String, engine: String) -> Result<String, String> {
    ensure_ui_chrome(&webview)?;
    let input = raw.trim();
    if input.is_empty() {
        return Ok("vibird://newtab".to_string());
    }
    if input.starts_with("vibird://")
        || input.starts_with("caram://")
        || input.starts_with("about:")
    {
        return Ok(input.to_string());
    }
    if input.starts_with("http://") || input.starts_with("https://") {
        return Ok(strip_tracking_parameters(input));
    }
    if input.starts_with("localhost") || input.starts_with("127.0.0.1") {
        return Ok(format!("http://{}", input));
    }
    let looks_like_domain = input.contains('.')
        && !input.contains(' ')
        && input
            .split('.')
            .last()
            .map(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
            .unwrap_or(false);
    if looks_like_domain {
        return Ok(strip_tracking_parameters(&format!("https://{}", input)));
    }
    let encoded = url::form_urlencoded::byte_serialize(input.as_bytes()).collect::<String>();
    if engine.contains("%s") {
        Ok(engine.replace("%s", &encoded))
    } else {
        Ok(format!("{}{}", engine, encoded))
    }
}

#[tauri::command]
pub async fn fetch_web_page(
    webview: Webview,
    shield: State<'_, ShieldEngine>,
    db: State<'_, DbManager>,
    url: String,
) -> Result<PageContentResponse, String> {
    ensure_ui_chrome(&webview)?;
    let verdict = shield.inspect_url(&url, &url).await;
    if verdict.blocked {
        return Ok(PageContentResponse {
            final_url: url,
            title: "Blocked by Vibird Shield".into(),
            html: "<h1>Blocked</h1>".into(),
            blocked_count: 1,
            status: 403,
        });
    }

    let client = reqwest::Client::builder()
        .user_agent(crate::bridge::CHROME_USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let final_url = resp.url().to_string();
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let _ = db.insert_history(&final_url, &final_url);

    Ok(PageContentResponse {
        final_url,
        title: "Web Resource".into(),
        html: text,
        blocked_count: 0,
        status,
    })
}

#[tauri::command]
pub fn record_history(
    webview: Webview,
    db: State<'_, DbManager>,
    url: String,
    title: String,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.insert_history(&url, &title).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn fetch_history(
    webview: Webview,
    db: State<'_, DbManager>,
) -> Result<Vec<HistoryRecord>, String> {
    ensure_ui_chrome(&webview)?;
    db.fetch_history().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history(webview: Webview, db: State<'_, DbManager>) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.wipe_history().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_bookmark(
    webview: Webview,
    db: State<'_, DbManager>,
    url: String,
    title: String,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.insert_bookmark(&url, &title).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn fetch_bookmarks(
    webview: Webview,
    db: State<'_, DbManager>,
) -> Result<Vec<BookmarkRecord>, String> {
    ensure_ui_chrome(&webview)?;
    db.fetch_bookmarks().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_bookmark(
    webview: Webview,
    db: State<'_, DbManager>,
    id: i64,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.delete_bookmark(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn fetch_downloads(
    webview: Webview,
    db: State<'_, DbManager>,
) -> Result<Vec<DownloadRecord>, String> {
    ensure_ui_chrome(&webview)?;
    db.fetch_downloads().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_downloads(webview: Webview, db: State<'_, DbManager>) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.wipe_downloads().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_download(
    webview: Webview,
    db: State<'_, DbManager>,
    id: i64,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.delete_download(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_file_manager(webview: Webview, path: String) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    let p = Path::new(&path);
    let canonical = p.canonicalize().map_err(|e| e.to_string())?;

    let target_dir = if canonical.is_file() {
        canonical.parent().unwrap_or(&canonical)
    } else {
        &canonical
    };

    Command::new("xdg-open")
        .arg(target_dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn fetch_extensions(
    webview: Webview,
    db: State<'_, DbManager>,
) -> Result<Vec<ExtensionItem>, String> {
    ensure_ui_chrome(&webview)?;
    db.fetch_extensions().map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn load_unpacked_extension(
    webview: Webview,
    db: State<'_, DbManager>,
    folder_path: String,
) -> Result<ExtensionItem, String> {
    ensure_ui_chrome(&webview)?;
    let item = ExtensionEngine::parse_manifest(&folder_path)?;
    db.save_extension(&item).map_err(|e| e.to_string())?;
    Ok(item)
}

#[tauri::command]
pub fn toggle_extension(
    webview: Webview,
    db: State<'_, DbManager>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.set_extension_state(&id, enabled).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_extension(
    webview: Webview,
    db: State<'_, DbManager>,
    id: String,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.remove_extension(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_doh(webview: Webview, url: String) -> Result<DnsTestResult, String> {
    ensure_ui_chrome(&webview)?;
    Ok(DnsResolver::ping_test(&url).await)
}

#[tauri::command]
pub fn vault_is_configured(webview: Webview, db: State<'_, DbManager>) -> Result<bool, String> {
    ensure_ui_chrome(&webview)?;
    Ok(db.get_master_hash().is_some())
}

#[tauri::command(rename_all = "snake_case")]
pub fn vault_setup(
    webview: Webview,
    db: State<'_, DbManager>,
    master_pass: String,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;

    if db.get_master_hash().is_some() {
        return Err("Vault is already initialized".into());
    }
    if master_pass.len() < 8 {
        return Err("Password must be at least 8 characters".into());
    }

    let hash = CryptoEngine::hash_master_password(&master_pass)?;
    db.set_master_hash(&hash).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn vault_save_credential(
    webview: Webview,
    db: State<'_, DbManager>,
    session: State<'_, VaultSession>,
    master_pass: String,
    website: String,
    username: String,
    secret: String,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    session.is_locked_out()?;

    let hash = db.get_master_hash().ok_or("Vault not initialized")?;
    if !CryptoEngine::verify_master_password(&master_pass, &hash) {
        session.mark_failure();
        return Err("Authentication failed: Wrong password".into());
    }

    let salt_bytes = b"vibird_vault_global_salt_v1";
    let key = CryptoEngine::derive_key(&master_pass, salt_bytes)?;
    let (cipher, nonce) = CryptoEngine::encrypt_with_derived_key(&key, &secret)?;

    db.insert_vault_row(&website, &username, &cipher, &nonce, "v1")
        .map_err(|e| e.to_string())?;

    session.mark_success(key);
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn vault_read_all(
    webview: Webview,
    db: State<'_, DbManager>,
    session: State<'_, VaultSession>,
    master_pass: String,
) -> Result<Vec<DecryptedVaultRecord>, String> {
    ensure_ui_chrome(&webview)?;
    session.is_locked_out()?;

    let hash = db.get_master_hash().ok_or("Vault not initialized")?;
    if !CryptoEngine::verify_master_password(&master_pass, &hash) {
        session.mark_failure();
        return Err("Authentication failed: Wrong password".into());
    }

    let salt_bytes = b"vibird_vault_global_salt_v1";
    let key = CryptoEngine::derive_key(&master_pass, salt_bytes)?;

    let rows = db.list_vault_rows().map_err(|e| e.to_string())?;
    let mut list = Vec::new();

    for r in rows {
        if let Ok(secret) = CryptoEngine::decrypt_with_derived_key(&key, &r.ciphertext, &r.nonce) {
            list.push(DecryptedVaultRecord {
                id: r.id,
                website: r.website,
                username: r.username,
                secret,
                created_at: r.created_at,
            });
        }
    }

    session.mark_success(key);
    Ok(list)
}

#[tauri::command]
pub fn vault_delete(
    webview: Webview,
    db: State<'_, DbManager>,
    session: State<'_, VaultSession>,
    id: i64,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    if session.get_key().is_none() {
        return Err("Vault is locked".into());
    }
    db.delete_vault_row(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vault_lock(webview: Webview, session: State<'_, VaultSession>) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    session.lock();
    Ok(())
}

#[tauri::command]
pub fn generate_password(webview: Webview, length: usize) -> Result<String, String> {
    ensure_ui_chrome(&webview)?;
    Ok(CryptoEngine::generate_strong_password(length))
}

#[tauri::command]
pub fn get_settings(webview: Webview, db: State<'_, DbManager>) -> Result<AppConfig, String> {
    ensure_ui_chrome(&webview)?;
    Ok(db.load_config())
}

#[tauri::command]
pub fn update_setting(
    webview: Webview,
    db: State<'_, DbManager>,
    key: String,
    value: String,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.save_config_item(&key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_shield_stats(
    webview: Webview,
    db: State<'_, DbManager>,
) -> Result<ShieldStats, String> {
    ensure_ui_chrome(&webview)?;
    let total = db.get_total_blocked();
    Ok(ShieldStats {
        total_blocked: total,
        trackers_blocked: total,
        bandwidth_saved_mb: (total as f64 * 0.08).round(),
        time_saved_secs: (total as f64 * 0.02).round(),
    })
}

#[tauri::command]
pub fn increment_blocked_stat(
    webview: Webview,
    db: State<'_, DbManager>,
    count: u64,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    db.increment_blocked_stat(count);
    Ok(())
}

#[tauri::command]
pub fn toggle_devtools(webview: Webview, app: AppHandle) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    if let Some(w) = app.get_webview_window("main") {
        if w.is_devtools_open() {
            w.close_devtools();
        } else {
            w.open_devtools();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn save_session(
    webview: Webview,
    db: State<'_, DbManager>,
    snapshot: SessionSnapshot,
) -> Result<(), String> {
    ensure_ui_chrome(&webview)?;
    let json = serde_json::to_string(&snapshot).map_err(|e| e.to_string())?;
    db.save_config_item("session_snapshot", &json)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn load_session(
    webview: Webview,
    db: State<'_, DbManager>,
) -> Result<Option<SessionSnapshot>, String> {
    ensure_ui_chrome(&webview)?;
    let Some(json) = db.load_config_item("session_snapshot") else {
        return Ok(None);
    };
    match serde_json::from_str::<SessionSnapshot>(&json) {
        Ok(snap) => Ok(Some(snap)),
        Err(e) => {
            log::warn!("Corrupted session snapshot: {}", e);
            Ok(None)
        }
    }
}
