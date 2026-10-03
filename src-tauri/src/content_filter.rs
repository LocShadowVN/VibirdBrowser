//! Network-level adblock via WebKit UserContentFilter.
//!
//! # Bước 2 (revised) — cache global + serialize
//!
//! Khác bản gốc:
//!   1. Filter được cache global — save+load chỉ 1 lần toàn process.
//!      Mọi tab sau chỉ add_filter(manager, cached_filter) sync.
//!   2. Serialize bằng AtomicBool — không cho 2 tab save song song.
//!   3. Filter ID có version (dựa vào mtime JSON) → tự invalidate cache cũ.
//!   4. Caller (commands.rs) transfer full ownership của manager_ptr qua
//!      `to_glib_full()`. Apply unref manager trong MỌI trường hợp.
//!
//! # Ref counting contract
//!
//! `apply_filter_for_manager(manager_ptr, ...)`:
//!   - Caller pass manager_ptr với ref đã +1 (transfer full).
//!   - Apply unref trong mọi đường: cached-sync, skip-busy, fail-sớm, callback.
//!   - Filter cached global giữ 1 ref suốt process; không unref (kernel dọn khi exit).

#![allow(dead_code)]

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

// ============================================================================
// Linux FFI
// ============================================================================

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    use std::path::Path;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

    /// Filter cache global — sống suốt app lifetime.
    /// 1 ref giữ vĩnh viễn, mọi manager dùng chung.
    static GLOBAL_FILTER: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

    /// Chỉ 1 pipeline save+load được chạy tại 1 thời điểm.
    static SAVE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

    #[repr(C)]
    struct GError {
        domain: u32,
        code: c_int,
        message: *mut c_char,
    }

    #[link(name = "webkit2gtk-4.1")]
    extern "C" {
        fn webkit_user_content_filter_store_new(path: *const c_char) -> *mut c_void;
        fn webkit_user_content_filter_store_save(
            store: *mut c_void,
            identifier: *const c_char,
            source: *mut c_void,
            cancellable: *mut c_void,
            callback: GAsyncReadyCallback,
            user_data: *mut c_void,
        );
        fn webkit_user_content_filter_store_save_finish(
            store: *mut c_void,
            result: *mut c_void,
            error: *mut *mut GError,
        ) -> *mut c_void;
        fn webkit_user_content_filter_store_load(
            store: *mut c_void,
            identifier: *const c_char,
            cancellable: *mut c_void,
            callback: GAsyncReadyCallback,
            user_data: *mut c_void,
        );
        fn webkit_user_content_filter_store_load_finish(
            store: *mut c_void,
            result: *mut c_void,
            error: *mut *mut GError,
        ) -> *mut c_void;
        fn webkit_user_content_manager_add_filter(
            manager: *mut c_void,
            filter: *mut c_void,
        );
    }

    #[link(name = "glib-2.0")]
    extern "C" {
        fn g_bytes_new(data: *const c_void, size: usize) -> *mut c_void;
        fn g_bytes_unref(bytes: *mut c_void);
        fn g_error_free(error: *mut GError);
    }

    #[link(name = "gobject-2.0")]
    extern "C" {
        fn g_object_unref(object: *mut c_void);
    }

    type GAsyncReadyCallback = Option<
        unsafe extern "C" fn(
            source_object: *mut c_void,
            res: *mut c_void,
            user_data: *mut c_void,
        ),
    >;

    unsafe fn log_gerror(prefix: &str, error: *mut GError) {
        if error.is_null() {
            log::warn!("{}: (no error detail)", prefix);
            return;
        }
        let msg = if (*error).message.is_null() {
            "(null message)".to_string()
        } else {
            CStr::from_ptr((*error).message)
                .to_string_lossy()
                .to_string()
        };
        log::warn!("{}: {}", prefix, msg);
        g_error_free(error);
    }

    struct SaveJob {
        store: *mut c_void,
        manager: *mut c_void,
        filter_id: CString,
    }

    struct LoadJob {
        store: *mut c_void,
        manager: *mut c_void,
        filter_id: CString,
    }

    /// Cleanup trên fail path: unref manager, clear cờ in-flight.
    unsafe fn fail_apply(manager_ptr: *mut c_void, msg: &str) {
        g_object_unref(manager_ptr);
        SAVE_IN_FLIGHT.store(false, Ordering::Release);
        log::warn!("Content filter: {}", msg);
    }

    unsafe extern "C" fn on_save_done(
        _source: *mut c_void,
        res: *mut c_void,
        user_data: *mut c_void,
    ) {
        if user_data.is_null() {
            log::warn!("on_save_done: user_data null");
            SAVE_IN_FLIGHT.store(false, Ordering::Release);
            return;
        }
        let job = Box::from_raw(user_data as *mut SaveJob);

        let mut error: *mut GError = ptr::null_mut();
        let filter =
            webkit_user_content_filter_store_save_finish(job.store, res, &mut error);

        if filter.is_null() {
            log_gerror("Content filter save failed", error);
            g_object_unref(job.manager);
            g_object_unref(job.store);
            SAVE_IN_FLIGHT.store(false, Ordering::Release);
            return;
        }

        // save_finish trả filter transfer full — ta không dùng, unref ngay.
        g_object_unref(filter);
        log::info!("Content filter: save done, now loading...");

        let store_ptr = job.store;
        let manager_ptr = job.manager;
        let filter_id_c = job.filter_id.clone();

        let load_job = Box::new(LoadJob {
            store: store_ptr,
            manager: manager_ptr,
            filter_id: filter_id_c,
        });
        let filter_id_ptr = load_job.filter_id.as_ptr();
        let load_ptr = Box::into_raw(load_job) as *mut c_void;

        webkit_user_content_filter_store_load(
            store_ptr,
            filter_id_ptr,
            ptr::null_mut(),
            Some(on_load_done),
            load_ptr,
        );
    }

    unsafe extern "C" fn on_load_done(
        _source: *mut c_void,
        res: *mut c_void,
        user_data: *mut c_void,
    ) {
        if user_data.is_null() {
            log::warn!("on_load_done: user_data null");
            SAVE_IN_FLIGHT.store(false, Ordering::Release);
            return;
        }
        let job = Box::from_raw(user_data as *mut LoadJob);

        let mut error: *mut GError = ptr::null_mut();
        let filter =
            webkit_user_content_filter_store_load_finish(job.store, res, &mut error);

        if filter.is_null() {
            log_gerror("Content filter load failed", error);
            g_object_unref(job.manager);
        } else {
            webkit_user_content_manager_add_filter(job.manager, filter);

            // Cache global: giữ ref của filter, không unref.
            // Process exit → kernel dọn.
            let prev = GLOBAL_FILTER.swap(filter, Ordering::AcqRel);
            if !prev.is_null() {
                // Không nên xảy ra (SAVE_IN_FLIGHT ngăn), nhưng phòng:
                g_object_unref(prev);
            }

            g_object_unref(job.manager);
            log::info!("Content filter applied to UserContentManager (cached globally)");
        }

        g_object_unref(job.store);
        SAVE_IN_FLIGHT.store(false, Ordering::Release);
    }

    pub fn probe_content_filter_store() -> Result<(), String> {
        let temp_dir = std::env::temp_dir().join("vibird-filter-probe");
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| format!("Cannot create probe dir: {}", e))?;

        let path_cstr = CString::new(temp_dir.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid path: {}", e))?;

        let store = unsafe { webkit_user_content_filter_store_new(path_cstr.as_ptr()) };
        if store.is_null() {
            return Err("store_new returned null".to_string());
        }
        log::info!("FFI probe: SUCCESS — store pointer = {:p}", store);
        unsafe { g_object_unref(store) };
        Ok(())
    }

    pub fn apply_filter_for_manager(
        manager_ptr: *mut c_void,
        json_path: &Path,
    ) -> Result<(), String> {
        if manager_ptr.is_null() {
            return Err("manager pointer is null".to_string());
        }

        // Fast path: filter đã có trong cache → add sync, unref, xong.
        let cached = GLOBAL_FILTER.load(Ordering::Acquire);
        if !cached.is_null() {
            unsafe {
                webkit_user_content_manager_add_filter(manager_ptr, cached);
                g_object_unref(manager_ptr);
            }
            log::info!("Content filter: applied cached filter to new webview");
            return Ok(());
        }

        // Tab khác đang build filter → skip tab này, unref, về.
        if SAVE_IN_FLIGHT.swap(true, Ordering::AcqRel) {
            unsafe { g_object_unref(manager_ptr); }
            log::warn!(
                "Content filter: another tab is building filter; \
                 skipping this tab (reload tab to apply)"
            );
            return Ok(());
        }

        if !json_path.exists() {
            unsafe { fail_apply(manager_ptr, "JSON not found"); }
            return Err(format!("JSON not found: {}", json_path.display()));
        }

        let json_bytes = match std::fs::read(json_path) {
            Ok(b) => b,
            Err(e) => {
                unsafe { fail_apply(manager_ptr, "cannot read JSON"); }
                return Err(format!("Cannot read JSON: {}", e));
            }
        };

        if json_bytes.is_empty() {
            unsafe { fail_apply(manager_ptr, "JSON is empty"); }
            return Err("JSON file is empty".to_string());
        }

        // Filter ID versioned bằng mtime JSON → invalidate cache khi file thay đổi.
        let version = std::fs::metadata(json_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filter_id_str = format!("vibird-easylist-v{}", version);

        let store_dir = match json_path.parent() {
            Some(p) => p,
            None => {
                unsafe { fail_apply(manager_ptr, "JSON has no parent dir"); }
                return Err("JSON has no parent dir".to_string());
            }
        };

        let store_path_cstr = match CString::new(store_dir.to_string_lossy().as_bytes()) {
            Ok(c) => c,
            Err(e) => {
                unsafe { fail_apply(manager_ptr, "invalid store path"); }
                return Err(format!("Invalid store path: {}", e));
            }
        };

        let store_ptr =
            unsafe { webkit_user_content_filter_store_new(store_path_cstr.as_ptr()) };
        if store_ptr.is_null() {
            unsafe { fail_apply(manager_ptr, "store_new returned null"); }
            return Err("Failed to create UserContentFilterStore".to_string());
        }

        let filter_id_c = match CString::new(filter_id_str.as_str()) {
            Ok(c) => c,
            Err(e) => {
                unsafe {
                    g_object_unref(store_ptr);
                    g_object_unref(manager_ptr);
                }
                SAVE_IN_FLIGHT.store(false, Ordering::Release);
                return Err(format!("Invalid filter id: {}", e));
            }
        };

        let bytes_ptr = unsafe {
            g_bytes_new(json_bytes.as_ptr() as *const c_void, json_bytes.len())
        };
        if bytes_ptr.is_null() {
            unsafe {
                g_object_unref(store_ptr);
                g_object_unref(manager_ptr);
            }
            SAVE_IN_FLIGHT.store(false, Ordering::Release);
            return Err("g_bytes_new returned null".to_string());
        }

        log::info!(
            "Content filter: initiating save (id={}, {} bytes)",
            filter_id_str,
            json_bytes.len()
        );

        let save_job = Box::new(SaveJob {
            store: store_ptr,
            manager: manager_ptr,
            filter_id: filter_id_c,
        });
        let filter_id_ptr = save_job.filter_id.as_ptr();
        let job_ptr = Box::into_raw(save_job) as *mut c_void;

        unsafe {
            webkit_user_content_filter_store_save(
                store_ptr,
                filter_id_ptr,
                bytes_ptr,
                ptr::null_mut(),
                Some(on_save_done),
                job_ptr,
            );
            g_bytes_unref(bytes_ptr);
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub use linux::{apply_filter_for_manager, probe_content_filter_store};

#[cfg(not(target_os = "linux"))]
pub fn probe_content_filter_store() -> Result<(), String> {
    Err("Not supported on non-Linux".to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_filter_for_manager(
    _manager: *mut std::os::raw::c_void,
    _json_path: &std::path::Path,
) -> Result<(), String> {
    Err("Not supported on non-Linux".to_string())
}
