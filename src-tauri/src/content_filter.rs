//! Network-level adblock via WebKit UserContentFilter (Safari Content Blocker JSON).
//!
//! Khác với JS hooks (chỉ chặn fetch/XHR/JS-set src), content filter chạy trong
//! network layer của WebKit — chặn mọi request bất kể nguồn: static `<img>`,
//! `<script src>` trong HTML, `<link href>`, preload, prefetch, favicon.
//!
//! Cách hoạt động:
//! 1. CI bundle `easylist_content_blocker.json` vào `resources/`.
//! 2. Startup: resolve path, lưu vào `ContentFilterState`.
//! 3. First content webview tạo ra: nạp filter vào `UserContentManager`.
//! 4. Mọi webview con sau đó (chia sẻ cùng `WebContext`) tự động kế thừa filter.

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
    use glib::MainContext;
    use gio::Cancellable;
    use webkit2gtk::{
        UserContentFilterStore, UserContentFilterStoreExt, UserContentManagerExt, WebViewExt,
    };

    let manager = wk_webview
        .user_content_manager()
        .ok_or_else(|| "no user content manager".to_string())?;

    let store_dir = json_path
        .parent()
        .ok_or_else(|| "json path has no parent".to_string())?;
    let store_path = store_dir.to_str().ok_or_else(|| "invalid utf-8 path".to_string())?;
    let store = UserContentFilterStore::new(store_path);

    let raw_json = std::fs::read(json_path).map_err(|e| e.to_string())?;
    let bytes = glib::Bytes::from(&raw_json);

    const FILTER_ID: &str = "vibird-easylist";
    let ctx = MainContext::default();

    // --- Save ---
    // `save` ghi atomic: gọi lại với cùng id sẽ overwrite, không lỗi.
    // WebKitGTK gọi callback trên main thread; ta pump main context cho đến khi
    // callback fire. Vì WebKitUserContentFilterStore parse JSON là CPU-bound +
    // ghi disk nhỏ, thời gian chờ thường < 200ms.
    let (tx_save, rx_save) = std::sync::mpsc::channel::<Result<(), String>>();
    store.save(
        FILTER_ID,
        &bytes,
        None::<&Cancellable>,
        move |result| {
            let _ = tx_save.send(result.map(|_| ()).map_err(|e| e.to_string()));
        },
    );

    let save_result = loop {
        match rx_save.try_recv() {
            Ok(r) => break r,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Pump main loop iteration để callback có cơ hội chạy.
                while ctx.pending() {
                    ctx.iteration(false);
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("save callback channel disconnected".into());
            }
        }
    };
    save_result?;

    // --- Load ---
    let (tx_load, rx_load) = std::sync::mpsc::channel::<Result<webkit2gtk::UserContentFilter, String>>();
    store.load(
        FILTER_ID,
        None::<&Cancellable>,
        move |result| {
            let _ = tx_load.send(result.map_err(|e| e.to_string()));
        },
    );

    let filter = loop {
        match rx_load.try_recv() {
            Ok(Ok(f)) => break f,
            Ok(Err(e)) => return Err(e),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                while ctx.pending() {
                    ctx.iteration(false);
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("load callback channel disconnected".into());
            }
        }
    };

    manager.add_filter(&filter);
    log::info!("Content filter applied to UserContentManager");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_filter_sync(
    _wk_webview: &(),
    _json_path: &std::path::Path,
) -> Result<(), String> {
    Ok(())
}
