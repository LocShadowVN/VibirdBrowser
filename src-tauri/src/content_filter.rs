//! Network-level adblock via WebKit UserContentFilter.
//!
//! # Bước 2: save + load + apply filter (fire-and-forget state machine)
//!
//! WebKitGTK async callbacks chạy trên GLib main thread. Không block main
//! thread khi chờ callback. Thay vào đó dùng state machine qua user_data:
//!
//!   1. apply_filter_for_manager(manager_ptr, json_path)
//!        → save(json) → return ngay
//!   2. [on_save_done callback]
//!        → save_finish → load(filter_id) → return ngay
//!   3. [on_load_done callback]
//!        → load_finish → add_filter(manager, filter) → cleanup
//!
//! # Safety
//!
//! - Mọi raw pointer null-check trước khi deref.
//! - `manager_ptr` được `to_glib_full()` (tăng ref) trước khi pass vào async.
//!   Callback cuối unref → ref count trở về như ban đầu.
//! - `store_ptr` từ `store_new` có 1 ref. Ta giữ ref này tới hết callback
//!   load, rồi unref. WebKit tự ref/unref quanh mỗi async op.
//! - `filter_id` giữ trong `Box<SaveJob>` / `Box<LoadJob>` để CString không bị
//!   drop khi WebKit còn dùng pointer. WebKit copy string ngay khi gọi API,
//!   nhưng giữ trong Box vẫn an toàn hơn.

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
// Linux: FFI implementation
// ============================================================================

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    use std::path::Path;
    use std::ptr;

    // ------------------------------------------------------------------------
    // GError — layout cố định theo GLib convention
    // ------------------------------------------------------------------------
    #[repr(C)]
    struct GError {
        domain: u32,
        code: c_int,
        message: *mut c_char,
    }

    // ------------------------------------------------------------------------
    // WebKitGTK C API
    // ------------------------------------------------------------------------
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
        fn g_object_ref(object: *mut c_void) -> *mut c_void;
    }

    type GAsyncReadyCallback = Option<
        unsafe extern "C" fn(
            source_object: *mut c_void,
            res: *mut c_void,
            user_data: *mut c_void,
        ),
    >;

    // ------------------------------------------------------------------------
    // Helper: log lỗi từ GError và free
    // ------------------------------------------------------------------------
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

    // ------------------------------------------------------------------------
    // Job structs — encode state qua async boundaries
    // ------------------------------------------------------------------------
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

    // ------------------------------------------------------------------------
    // Callback: save done → fire load
    // ------------------------------------------------------------------------
    unsafe extern "C" fn on_save_done(
        _source: *mut c_void,
        res: *mut c_void,
        user_data: *mut c_void,
    ) {
        if user_data.is_null() {
            log::warn!("on_save_done: user_data null");
            return;
        }
        let job = Box::from_raw(user_data as *mut SaveJob);

        let mut error: *mut GError = ptr::null_mut();
        let filter = webkit_user_content_filter_store_save_finish(
            job.store,
            res,
            &mut error,
        );

        if filter.is_null() {
            log_gerror("Content filter save failed", error);
            // Cleanup: unref store + manager (ta giữ ref)
            g_object_unref(job.store);
            g_object_unref(job.manager);
            return;
        }

        // save_finish trả filter "transfer full" — ta không dùng, unref ngay
        g_object_unref(filter);
        log::info!("Content filter saved, now loading...");

        // Chuẩn bị load job
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
        // store_ptr và manager_ptr giờ do load_job giữ. Không unref ở đây.
    }

    // ------------------------------------------------------------------------
    // Callback: load done → apply filter → cleanup
    // ------------------------------------------------------------------------
    unsafe extern "C" fn on_load_done(
        _source: *mut c_void,
        res: *mut c_void,
        user_data: *mut c_void,
    ) {
        if user_data.is_null() {
            log::warn!("on_load_done: user_data null");
            return;
        }
        let job = Box::from_raw(user_data as *mut LoadJob);

        let mut error: *mut GError = ptr::null_mut();
        let filter = webkit_user_content_filter_store_load_finish(
            job.store,
            res,
            &mut error,
        );

        if filter.is_null() {
            log_gerror("Content filter load failed", error);
        } else {
            webkit_user_content_manager_add_filter(job.manager, filter);
            g_object_unref(filter);
            log::info!("Content filter applied to UserContentManager");
        }

        // Cleanup refs ta giữ
        g_object_unref(job.store);
        g_object_unref(job.manager);
    }

    // ------------------------------------------------------------------------
    // Public: probe (giữ từ bước 1, dùng để debug)
    // ------------------------------------------------------------------------
    pub fn probe_content_filter_store() -> Result<(), String> {
        let temp_dir = std::env::temp_dir().join("vibird-filter-probe");
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| format!("Cannot create probe dir: {}", e))?;

        let path_cstr = CString::new(temp_dir.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid path: {}", e))?;

        log::info!(
            "FFI probe: calling webkit_user_content_filter_store_new with path={:?}",
            temp_dir
        );

        let store = unsafe { webkit_user_content_filter_store_new(path_cstr.as_ptr()) };

        if store.is_null() {
            return Err("store_new returned null".to_string());
        }

        log::info!("FFI probe: SUCCESS — store pointer = {:p}", store);

        unsafe {
            g_object_unref(store);
        }

        log::info!("FFI probe: store unreffed, cleaned up");
        Ok(())
    }

    // ------------------------------------------------------------------------
    // Public: apply filter cho một UserContentManager
    //
    // manager_ptr: raw `WebKitUserContentManager*` với ref count của caller.
    //              Hàm này sẽ tự `g_object_ref` thêm 1 lần, và unref trong
    //              callback cuối. Caller có thể unref ref của mình sau đó.
    // ------------------------------------------------------------------------
    pub fn apply_filter_for_manager(
        manager_ptr: *mut c_void,
        json_path: &Path,
    ) -> Result<(), String> {
        if manager_ptr.is_null() {
            return Err("manager pointer is null".to_string());
        }
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

        // Store dùng cùng thư mục với JSON
        let store_dir = json_path
            .parent()
            .ok_or_else(|| "JSON has no parent dir".to_string())?;
        let store_path_cstr = CString::new(store_dir.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid store path: {}", e))?;

        let store_ptr = unsafe { webkit_user_content_filter_store_new(store_path_cstr.as_ptr()) };
        if store_ptr.is_null() {
            return Err("Failed to create UserContentFilterStore".to_string());
        }

        const FILTER_ID: &str = "vibird-easylist";
        let filter_id_c = CString::new(FILTER_ID)
            .map_err(|e| format!("Invalid filter id: {}", e))?;

        // GBytes từ JSON (ta unref sau khi pass vào WebKit)
        let bytes_ptr = unsafe {
            g_bytes_new(json_bytes.as_ptr() as *const c_void, json_bytes.len())
        };
        if bytes_ptr.is_null() {
            unsafe { g_object_unref(store_ptr) };
            return Err("g_bytes_new returned null".to_string());
        }

        // Ref manager thêm 1 lần — callback sẽ unref
        let manager_ref = unsafe { g_object_ref(manager_ptr) };

        // Tạo SaveJob — pass qua user_data
        let save_job = Box::new(SaveJob {
            store: store_ptr,
            manager: manager_ref,
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
        }

        // Unref bytes — WebKit đã giữ ref riêng
        unsafe { g_bytes_unref(bytes_ptr) };

        log::info!("Content filter: save initiated (async)");
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
