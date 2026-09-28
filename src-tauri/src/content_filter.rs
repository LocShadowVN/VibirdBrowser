//! Network-level adblock via WebKit UserContentFilter (Safari Content Blocker JSON).
//!
//! Khác với JS hooks (chỉ chặn fetch/XHR/JS-set src), content filter chạy trong
//! network layer của WebKit — chặn mọi request bất kể nguồn: static `<img>`,
//! `<script src>` trong HTML, `<link href>`, preload, prefetch, favicon.
//!
//! Filter được save vào UserContentFilterStore (persistent) và apply vào
//! UserContentManager của **từng webview**. Không có API global.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct ContentFilterState {
    applied: AtomicBool,
    resource_path: Option<PathBuf>,
}

impl ContentFilterState {
    pub fn new(resource_path: Option<PathBuf>) -> Self {
        Self {
            applied: AtomicBool::new(false),
            resource_path,
        }
    }

    pub fn is_applied(&self) -> bool {
        self.applied.load(Ordering::Relaxed)
    }

    pub fn mark_applied(&self) {
        self.applied.store(true, Ordering::Relaxed);
    }

    pub fn resource_path(&self) -> Option<&PathBuf> {
        self.resource_path.as_ref()
    }
}

#[cfg(target_os = "linux")]
pub fn apply_filter_sync(
    wk_webview: &webkit2gtk::WebView,
    json_path: &std::path::Path,
) -> Result<(), String> {
    use webkit2gtk::{
        gio, glib,
        UserContentFilter, UserContentFilterStore, UserContentFilterStoreExt,
        UserContentManagerExt, WebViewExt,
    };

    // ------------------------------------------------------------------
    // Resolve UserContentManager của webview
    // ------------------------------------------------------------------
    let manager = wk_webview
        .user_content_manager()
        .ok_or_else(|| "webview has no UserContentManager".to_string())?;

    // ------------------------------------------------------------------
    // Store nằm cùng thư mục với JSON
    // ------------------------------------------------------------------
    let store_dir = json_path
        .parent()
        .ok_or_else(|| "json path has no parent".to_string())?;
    let store_path = store_dir
        .to_str()
        .ok_or_else(|| "invalid utf-8 path".to_string())?;
    let store = UserContentFilterStore::new(store_path);

    const FILTER_ID: &str = "vibird-easylist";

    // ------------------------------------------------------------------
    // Save filter vào store (JSON → compiled binary, atomic write)
    // ------------------------------------------------------------------
    let raw_json = std::fs::read(json_path).map_err(|e| e.to_string())?;
    let bytes = glib::Bytes::from(&raw_json);

    let (tx_save, rx_save) = std::sync::mpsc::channel::<Result<(), String>>();
    store.save(
        FILTER_ID,
        &bytes,
        None::<&gio::Cancellable>,
        move |result| {
            let _ = tx_save.send(result.map(|_| ()).map_err(|e| e.to_string()));
        },
    );
    pump_until(&rx_save, 30)??;

    // ------------------------------------------------------------------
    // Load compiled filter từ store
    // ------------------------------------------------------------------
    let (tx_load, rx_load) =
        std::sync::mpsc::channel::<Result<UserContentFilter, String>>();
    store.load(FILTER_ID, None::<&gio::Cancellable>, move |result| {
        let _ = tx_load.send(result.map_err(|e| e.to_string()));
    });
    let filter = pump_until(&rx_load, 30)??;

    // ------------------------------------------------------------------
    // Apply filter cho webview này
    // ------------------------------------------------------------------
    manager.add_filter(&filter);

    log::info!("Content filter applied to webview");
    Ok(())
}

/// Pump GTK main context cho đến khi nhận được message hoặc timeout.
///
/// Cần thiết vì WebKitGTK gọi callback trên **main thread** — nếu block main
/// thread bằng `recv_timeout` thuần, callback không bao giờ fire → deadlock.
#[cfg(target_os = "linux")]
fn pump_until<T>(
    rx: &std::sync::mpsc::Receiver<T>,
    timeout_secs: u64,
) -> Result<T, String> {
    use webkit2gtk::glib::MainContext;

    let ctx = MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        match rx.try_recv() {
            Ok(v) => return Ok(v),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("callback channel disconnected".into());
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if std::time::Instant::now() > deadline {
                    return Err("operation timed out".into());
                }
                while ctx.pending() {
                    ctx.iteration(false);
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn apply_filter_sync(
    _wk_webview: &(),
    _json_path: &std::path::Path,
) -> Result<(), String> {
    Ok(())
}
