//! Network-level adblock via WebKit UserContentFilter.
//!
//! # Kiến trúc (đã fix bug regression v2.1.0)
//!
//! - Filter build **1 lần** cho toàn process (global cache).
//! - Tab đến khi filter đang build → **queue** manager ptr, callback
//!   load_done sẽ apply cho tất cả.
//! - `FILTER_READY` là static AtomicBool cho commands.rs check trước khi
//!   navigate URL thật (tránh ads load trước khi filter sẵn sàng).

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// True khi filter đã build xong và cache sẵn sàng.
/// commands.rs đọc cờ này để quyết định delay navigate hay không.
pub static FILTER_READY: AtomicBool = AtomicBool::new(false);

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
    use std::sync::Mutex;

    /// Filter cache global — sống suốt process lifetime.
    static GLOBAL_FILTER: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

    /// True khi có pipeline save+load đang chạy.
    static SAVE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

    /// Manager pointers (dạng usize) đang chờ filter ready.
    /// Khi filter build xong → apply hết rồi clear.
    static PENDING_MANAGERS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

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
            log::warn!("{}: (null error)", prefix);
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

    struct Job {
        store: *mut c_void,
        filter_id: CString,
    }

    unsafe extern "C" fn on_save_done(
        _source: *mut c_void,
        res: *mut c_void,
        user_data: *mut c_void,
    ) {
        if user_data.is_null() {
            SAVE_IN_FLIGHT.store(false, Ordering::Release);
            return;
        }
        let job = Box::from_raw(user_data as *mut Job);

        let mut error: *mut GError = ptr::null_mut();
        let filter =
            webkit_user_content_filter_store_save_finish(job.store, res, &mut error);

        if filter.is_null() {
            log_gerror("Content filter save failed", error);
            g_object_unref(job.store);
            SAVE_IN_FLIGHT.store(false, Ordering::Release);
            // Pending managers: unref (không apply được).
            let pending: Vec<usize> =
                std::mem::take(&mut *PENDING_MANAGERS.lock().unwrap());
            for addr in pending {
                let m = addr as *mut c_void;
                if !m.is_null() {
                    g_object_unref(m);
                }
            }
            return;
        }

        g_object_unref(filter);
        log::info!("Content filter: save done, loading...");

        let store = job.store;
        let filter_id = job.filter_id.clone();
        let load_job = Box::new(Job { store, filter_id });
        let filter_id_ptr = load_job.filter_id.as_ptr();
        let load_ptr = Box::into_raw(load_job) as *mut c_void;

        webkit_user_content_filter_store_load(
            store,
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
            SAVE_IN_FLIGHT.store(false, Ordering::Release);
            return;
        }
        let job = Box::from_raw(user_data as *mut Job);

        let mut error: *mut GError = ptr::null_mut();
        let filter =
            webkit_user_content_filter_store_load_finish(job.store, res, &mut error);

        if filter.is_null() {
            log_gerror("Content filter load failed", error);
            // Unref pending.
            let pending: Vec<usize> =
                std::mem::take(&mut *PENDING_MANAGERS.lock().unwrap());
            for addr in pending {
                let m = addr as *mut c_void;
                if !m.is_null() {
                    g_object_unref(m);
                }
            }
        } else {
            // Cache filter.
            let prev = GLOBAL_FILTER.swap(filter, Ordering::AcqRel);
            if !prev.is_null() {
                g_object_unref(prev);
            }
            crate::content_filter::FILTER_READY.store(true, Ordering::Release);
            log::info!("Content filter: loaded & cached globally");

            // Apply cho tất cả tab đang chờ.
            let pending: Vec<usize> =
                std::mem::take(&mut *PENDING_MANAGERS.lock().unwrap());
            let count = pending.len();
            for addr in pending {
                let m = addr as *mut c_void;
                if !m.is_null() {
                    webkit_user_content_manager_add_filter(m, filter);
                    g_object_unref(m);
                }
            }
            if count > 0 {
                log::info!(
                    "Content filter: applied to {} pending tab(s)",
                    count
                );
            }
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

        // ---------- Fast path: filter đã cache ----------
        let cached = GLOBAL_FILTER.load(Ordering::Acquire);
        if !cached.is_null() {
            unsafe {
                webkit_user_content_manager_add_filter(manager_ptr, cached);
                g_object_unref(manager_ptr);
            }
            log::info!("Content filter: applied cached filter to new tab");
            return Ok(());
        }

        // ---------- Build đang chạy: queue để apply sau ----------
        if SAVE_IN_FLIGHT.load(Ordering::Acquire) {
            let queue_len = {
                let mut guard = PENDING_MANAGERS.lock().unwrap();
                guard.push(manager_ptr as usize);
                guard.len()
            };
            log::info!(
                "Content filter: build in flight, tab queued ({} pending)",
                queue_len
            );
            return Ok(());
        }

        // ---------- Bắt đầu build ----------
        if !json_path.exists() {
            unsafe { g_object_unref(manager_ptr); }
            return Err(format!("JSON not found: {}", json_path.display()));
        }

        let json_bytes = match std::fs::read(json_path) {
            Ok(b) => b,
            Err(e) => {
                unsafe { g_object_unref(manager_ptr); }
                return Err(format!("Cannot read JSON: {}", e));
            }
        };
        if json_bytes.is_empty() {
            unsafe { g_object_unref(manager_ptr); }
            return Err("JSON file is empty".to_string());
        }

        // Filter ID versioned theo mtime → invalidate cache khi update JSON.
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
                unsafe { g_object_unref(manager_ptr); }
                return Err("JSON has no parent dir".to_string());
            }
        };

        let store_path_cstr = match CString::new(store_dir.to_string_lossy().as_bytes()) {
            Ok(c) => c,
            Err(e) => {
                unsafe { g_object_unref(manager_ptr); }
                return Err(format!("Invalid store path: {}", e));
            }
        };

        let store_ptr =
            unsafe { webkit_user_content_filter_store_new(store_path_cstr.as_ptr()) };
        if store_ptr.is_null() {
            unsafe { g_object_unref(manager_ptr); }
            return Err("Failed to create UserContentFilterStore".to_string());
        }

        // Set cờ TRƯỚC khi save để tab đến sau sẽ queue.
        SAVE_IN_FLIGHT.store(true, Ordering::Release);

        // Queue manager của tab này cho lần build hiện tại.
        PENDING_MANAGERS.lock().unwrap().push(manager_ptr as usize);

        let filter_id_c = match CString::new(filter_id_str.as_str()) {
            Ok(c) => c,
            Err(e) => {
                unsafe { g_object_unref(store_ptr); }
                SAVE_IN_FLIGHT.store(false, Ordering::Release);
                return Err(format!("Invalid filter id: {}", e));
            }
        };

        let bytes_ptr = unsafe {
            g_bytes_new(json_bytes.as_ptr() as *const c_void, json_bytes.len())
        };
        if bytes_ptr.is_null() {
            unsafe { g_object_unref(store_ptr); }
            SAVE_IN_FLIGHT.store(false, Ordering::Release);
            return Err("g_bytes_new returned null".to_string());
        }

        log::info!(
            "Content filter: initiating save (id={}, {} bytes)",
            filter_id_str,
            json_bytes.len()
        );

        let job = Box::new(Job {
            store: store_ptr,
            filter_id: filter_id_c,
        });
        let filter_id_ptr = job.filter_id.as_ptr();
        let job_ptr = Box::into_raw(job) as *mut c_void;

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
