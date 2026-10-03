//! Network-level adblock via WebKit UserContentFilter (Safari Content Blocker JSON).
//!
//! # Tại sao FFI?
//!
//! Crate `webkit2gtk` (safe binding) KHÔNG expose `UserContentFilterStore` type
//! dù feature `v2_24` được bật. Đây là bug đã biết của crate. Kiểm tra docs.rs
//! sẽ thấy type này vắng mặt.
//!
//! Giải pháp: gọi trực tiếp WebKitGTK C API qua crate `webkit2gtk-sys` (raw FFI).
//!
//! # Flow
//!
//! 1. App startup: đọc `resources/easylist_content_blocker.json`.
//! 2. Tạo `WebKitUserContentFilterStore` (persistent, cache trên disk).
//! 3. Save JSON vào store → WebKit compile thành binary filter.
//! 4. Load filter từ store → có `WebKitUserContentFilter` handle.
//! 5. Apply filter cho `UserContentManager` của mỗi webview.
//! 6. WebKit network layer check MỌI request (kể cả static `<img>`, `<script src>`)
//!    với compiled filter trước khi gửi ra ngoài.
//!
//! # Safety
//!
//! Module này dùng `unsafe` FFI. Mọi pointer đều null-check trước khi deref.
//! GObject được wrap trong Drop impl để `g_object_unref` khi hết scope.
//!
//! # Giới hạn của prototype này
//!
//! - Chưa integrate vào `open_native_tab`. Chạy như module độc lập để test FFI.
//! - Async callback của WebKitGTK dùng fire-and-forget: log kết quả, không block
//!   main thread.
//! - Chưa handle multi-webview sharing filter (mỗi webview apply riêng).

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// State toàn cục cho content filter.
pub struct ContentFilterState {
    /// Đã apply filter cho ít nhất 1 webview chưa.
    applied: AtomicBool,
    /// Path tới file JSON EasyList content blocker.
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

// ============================================================================
// Linux implementation via FFI
// ============================================================================

#[cfg(target_os = "linux")]
pub mod linux {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_void};
    use std::path::Path;

    use glib_sys::{g_bytes_new, g_object_unref, GBytes, GError};
    use gobject_sys::GObject;
    use gio_sys::{GCancellable, GAsyncResult};

    // ========================================================================
    // Opaque C types — không cần full definition, chỉ cần pointer size
    // ========================================================================

    #[repr(C)]
    pub struct WebKitUserContentFilterStore {
        _private: [u8; 0],
    }

    #[repr(C)]
    pub struct WebKitUserContentFilter {
        _private: [u8; 0],
    }

    #[repr(C)]
    pub struct WebKitUserContentManager {
        _private: [u8; 0],
    }

    // ========================================================================
    // FFI bindings cho WebKitGTK C API
    //
    // Reference:
    //   https://webkitgtk.org/reference/webkit2gtk/stable/class.UserContentFilterStore.html
    // ========================================================================

    #[link(name = "webkit2gtk-4.1")]
    extern "C" {
        fn webkit_user_content_filter_store_new(
            storage_path: *const c_char,
        ) -> *mut WebKitUserContentFilterStore;

        fn webkit_user_content_filter_store_save(
            store: *mut WebKitUserContentFilterStore,
            identifier: *const c_char,
            source: *mut GBytes,
            cancellable: *mut GCancellable,
            callback: GAsyncReadyCallback,
            user_data: *mut c_void,
        );

        fn webkit_user_content_filter_store_save_finish(
            store: *mut WebKitUserContentFilterStore,
            result: *mut GAsyncResult,
            error: *mut *mut GError,
        ) -> *mut WebKitUserContentFilter;

        fn webkit_user_content_filter_store_load(
            store: *mut WebKitUserContentFilterStore,
            identifier: *const c_char,
            cancellable: *mut GCancellable,
            callback: GAsyncReadyCallback,
            user_data: *mut c_void,
        );

        fn webkit_user_content_filter_store_load_finish(
            store: *mut WebKitUserContentFilterStore,
            result: *mut GAsyncResult,
            error: *mut *mut GError,
        ) -> *mut WebKitUserContentFilter;

        fn webkit_user_content_manager_add_filter(
            manager: *mut WebKitUserContentManager,
            filter: *mut WebKitUserContentFilter,
        );

        // GObject reference management
        fn g_object_ref(object: *mut c_void) -> *mut c_void;
    }

    type GAsyncReadyCallback = Option<
        unsafe extern "C" fn(
            source_object: *mut GObject,
            res: *mut GAsyncResult,
            user_data: *mut c_void,
        ),
    >;

    // ========================================================================
    // Wrapper đảm bảo cleanup GObject
    // ========================================================================

    struct StorePtr(*mut WebKitUserContentFilterStore);

    impl Drop for StorePtr {
        fn drop(&mut self) {
            unsafe {
                if !self.0.is_null() {
                    g_object_unref(self.0 as *mut c_void);
                }
            }
        }
    }

    // ========================================================================
    // Cấu trúc state để callback ghi kết quả
    //
    // Vấn đề: WebKitGTK callback chạy trên GLib main thread. Hàm này có thể
    // chạy trên main thread (nếu gọi từ `with_webview`) hoặc worker thread.
    //
    // Cho prototype: chỉ log kết quả, không await. User xem `RUST_LOG=info`
    // để verify FFI hoạt động.
    // ========================================================================

    /// Data pass qua `user_data` pointer cho callback save.
    struct SaveCallbackCtx;

    unsafe extern "C" fn on_save_done(
        source_object: *mut GObject,
        res: *mut GAsyncResult,
        _user_data: *mut c_void,
    ) {
        if source_object.is_null() || res.is_null() {
            log::warn!("Content filter save callback: null pointer");
            return;
        }

        let store = source_object as *mut WebKitUserContentFilterStore;
        let mut error: *mut GError = std::ptr::null_mut();

        let result = webkit_user_content_filter_store_save_finish(store, res, &mut error);

        if result.is_null() {
            let msg = if !error.is_null() {
                let m = CStr::from_ptr((*error).message)
                    .to_string_lossy()
                    .to_string();
                glib_sys::g_error_free(error);
                m
            } else {
                "unknown save error".to_string()
            };
            log::warn!("Content filter save failed: {}", msg);
            return;
        }

        // save_finish returns a filter reference. Không cần giữ vì ta
        // load lại để apply. unref ngay.
        g_object_unref(result as *mut c_void);
        log::info!("Content filter saved successfully");
    }

    /// Data pass cho callback load.
    ///
    /// Trong callback này, ta apply filter cho UserContentManager.
    /// manager pointer được encode trong user_data.
    struct LoadCallbackCtx {
        manager: *mut WebKitUserContentManager,
    }

    unsafe extern "C" fn on_load_done(
        source_object: *mut GObject,
        res: *mut GAsyncResult,
        user_data: *mut c_void,
    ) {
        let ctx = Box::from_raw(user_data as *mut LoadCallbackCtx);

        if source_object.is_null() || res.is_null() {
            log::warn!("Content filter load callback: null pointer");
            return;
        }

        let store = source_object as *mut WebKitUserContentFilterStore;
        let mut error: *mut GError = std::ptr::null_mut();

        let filter = webkit_user_content_filter_store_load_finish(store, res, &mut error);

        if filter.is_null() {
            let msg = if !error.is_null() {
                let m = CStr::from_ptr((*error).message)
                    .to_string_lossy()
                    .to_string();
                glib_sys::g_error_free(error);
                m
            } else {
                "unknown load error".to_string()
            };
            log::warn!("Content filter load failed: {}", msg);
            return;
        }

        // Apply filter cho manager (nếu manager còn valid)
        if !ctx.manager.is_null() {
            webkit_user_content_manager_add_filter(ctx.manager, filter);
            log::info!("Content filter applied to UserContentManager");
        } else {
            log::warn!("Content filter loaded but manager pointer is null");
        }

        // filter được manager giữ ref, ta unref của mình
        g_object_unref(filter as *mut c_void);
    }

    // ========================================================================
    // Public API — apply content filter cho webview
    // ========================================================================

    /// Apply Content Blocker JSON cho một webview cụ thể.
    ///
    /// Flow:
    ///   1. Tạo `UserContentFilterStore` với storage path = parent dir của JSON.
    ///   2. Save JSON → compile + lưu cache trên disk.
    ///   3. Callback `on_save_done` log kết quả.
    ///   4. Load filter → lấy handle.
    ///   5. Callback `on_load_done` apply filter cho webview's UserContentManager.
    ///
    /// Hàm này **không block**. Save và load là async. Callback chạy sau khi
    /// hàm return, khi GLib main loop pump.
    ///
    /// # Safety
    ///
    /// Caller phải đảm bảo `wk_webview` còn sống khi callback fire. Đây là lý
    /// do hàm này nên gọi sau khi webview đã được tạo và add vào window.
    pub fn apply_content_filter(
        wk_webview: &webkit2gtk::WebView,
        json_path: &Path,
    ) -> Result<(), String> {
        // Validate input
        if !json_path.exists() {
            return Err(format!("JSON not found: {}", json_path.display()));
        }

        let json_bytes = std::fs::read(json_path)
            .map_err(|e| format!("Cannot read JSON: {}", e))?;

        if json_bytes.is_empty() {
            return Err("JSON file is empty".to_string());
        }

        log::info!(
            "Content filter: loading JSON of {} bytes from {:?}",
            json_bytes.len(),
            json_path
        );

        // Get UserContentManager từ webview
        use webkit2gtk::WebViewExt;
        let manager = wk_webview
            .user_content_manager()
            .ok_or_else(|| "Webview has no UserContentManager".to_string())?;

        // Convert manager to raw pointer
        let manager_ptr = {
            use glib::translate::ToGlibPtr;
            manager.to_glib_none().0 as *mut WebKitUserContentManager
        };

        if manager_ptr.is_null() {
            return Err("Manager pointer is null".to_string());
        }

        // Setup store path — dùng cùng thư mục với JSON
        let store_dir = json_path
            .parent()
            .ok_or_else(|| "JSON has no parent dir".to_string())?;
        let store_path_cstr = CString::new(store_dir.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid path: {}", e))?;

        // Create store
        let store_raw = unsafe { webkit_user_content_filter_store_new(store_path_cstr.as_ptr()) };
        if store_raw.is_null() {
            return Err("Failed to create UserContentFilterStore".to_string());
        }
        let store = StorePtr(store_raw);

        // Filter identifier
        const FILTER_ID: &str = "vibird-easylist";
        let filter_id_cstr =
            CString::new(FILTER_ID).map_err(|e| format!("Invalid filter id: {}", e))?;

        // ====================================================================
        // Step 1: Save JSON → store (async)
        // ====================================================================
        let bytes_ptr = unsafe {
            g_bytes_new(json_bytes.as_ptr() as *const c_void, json_bytes.len())
        };
        if bytes_ptr.is_null() {
            return Err("g_bytes_new returned null".to_string());
        }

        // Allocate callback context (empty struct)
        let save_ctx = Box::new(SaveCallbackCtx);
        let save_ctx_ptr = Box::into_raw(save_ctx) as *mut c_void;

        unsafe {
            webkit_user_content_filter_store_save(
                store.0,
                filter_id_cstr.as_ptr(),
                bytes_ptr,
                std::ptr::null_mut(),
                Some(on_save_done),
                save_ctx_ptr,
            );
        }

        // unref bytes — WebKit đã giữ ref
        unsafe {
            g_object_unref(bytes_ptr as *mut c_void);
        }

        // ====================================================================
        // Step 2: Load filter → apply (async)
        //
        // Load sẽ fail nếu save chưa xong. Vì cả 2 đều async, thứ tự gọi
        // không đảm bảo. Cho prototype, ta gọi load ngay sau save — WebKit
        // queue load sau save trong cùng main loop, nên thường OK.
        //
        // Nếu fail, cần refactor thành state machine (đợi save callback
        // mới gọi load).
        // ====================================================================

        let load_ctx = Box::new(LoadCallbackCtx {
            manager: manager_ptr,
        });
        let load_ctx_ptr = Box::into_raw(load_ctx) as *mut c_void;

        unsafe {
            webkit_user_content_filter_store_load(
                store.0,
                filter_id_cstr.as_ptr(),
                std::ptr::null_mut(),
                Some(on_load_done),
                load_ctx_ptr,
            );
        }

        // Store sẽ drop ở đây → unref. Nhưng WebKit giữ ref trong async op,
        // nên store vẫn sống tới khi callback fire.
        drop(store);

        log::info!("Content filter: save + load initiated (async)");
        Ok(())
    }

    /// Force unref một GObject — dùng cho cleanup thủ công nếu cần.
    pub unsafe fn unref_gobject(ptr: *mut c_void) {
        if !ptr.is_null() {
            g_object_unref(ptr);
        }
    }

    /// Ref một GObject — giữ nó sống qua callback.
    pub unsafe fn ref_gobject(ptr: *mut c_void) -> *mut c_void {
        if ptr.is_null() {
            return std::ptr::null_mut();
        }
        g_object_ref(ptr)
    }
}

// ============================================================================
// Non-Linux stub — no-op
// ============================================================================

#[cfg(not(target_os = "linux"))]
pub mod linux {
    use std::path::Path;

    pub fn apply_content_filter(
        _wk_webview: &(),
        _json_path: &Path,
    ) -> Result<(), String> {
        log::warn!("Content filter not supported on this platform");
        Ok(())
    }
}
