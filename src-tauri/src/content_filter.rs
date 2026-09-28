//! Network-level adblock via WebKit UserContentFilter (Safari Content Blocker JSON).
//!
//! Khác với JS hooks (chỉ chặn fetch/XHR/JS-set src), content filter chạy trong
//! network layer của WebKit — chặn mọi request bất kể nguồn: static `<img>`,
//! `<script src>` trong HTML, `<link href>`, preload, prefetch, favicon.
//!
//! Apply ở **WebContext** level thay vì UserContentManager vì WebContextExt
//! ổn định hơn across crate versions và auto áp dụng cho mọi webview cùng context.

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
        gio, glib, UserContentFilter, UserContentFilterStore, UserContentFilterStoreExt,
        WebContextExt, WebViewExt,
    };

    let raw_json = std::fs::read(json_path).map_err(|e| e.to_string())?;
    let bytes = glib::Bytes::from(&raw_json);

    let store_dir = json_path
        .parent()
        .ok_or_else(|| "json path has no parent".to_string())?;
    let store_path = store_dir
        .to_str()
        .ok_or_else(|| "invalid utf-8 path".to_string())?;
    let store = UserContentFilterStore::new(store_path);

    const FILTER_ID: &str = "vibird-easylist";

    // ------------------------------------------------------------------
    // Save filter to store (async callback → channel sync)
    // ------------------------------------------------------------------
    let (tx_save, rx_save) = std::sync::mpsc::channel::<Result<(), String>>();
    store.save(
        FILTER_ID,
        &bytes,
        None::<&gio::Cancellable>,
        move |result| {
            let _ = tx_save.send(result.map(|_| ()).map_err(|e| e.to_string()));
        },
    );

    rx_save
        .recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|e| format!("filter save timeout: {}", e))??;

    // ------------------------------------------------------------------
    // Load filter từ store
    // ------------------------------------------------------------------
    let (tx_load, rx_load) = std::sync::mpsc::channel::<Result<UserContentFilter, String>>();
    store.load(
        FILTER_ID,
        None::<&gio::Cancellable>,
        move |result| {
            let _ = tx_load.send(result.map_err(|e| e.to_string()));
        },
    );

    let filter = rx_load
        .recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|e| format!("filter load timeout: {}", e))??;

    // ------------------------------------------------------------------
    // Apply filter to WebContext (inherited by every webview in the same context)
    // ------------------------------------------------------------------
    let context = wk_webview
        .context()
        .ok_or_else(|| "webview has no WebContext".to_string())?;

    context.add_filter(&filter);

    log::info!("Content filter applied to WebContext");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_filter_sync(
    _wk_webview: &(),
    _json_path: &std::path::Path,
) -> Result<(), String> {
    Ok(())
}
