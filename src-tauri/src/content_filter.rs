//! Network-level adblock via WebKit UserContentFilter.
//!
//! # Trạng thái: PROTOTYPE — Bước 1 (verify FFI link)
//!
//! Module này kiểm tra xem WebKitGTK C API có accessible từ Rust không.
//! Nếu pass, chứng minh FFI approach khả thi. Chưa apply filter thật.
//!
//! # Approach
//!
//! WebKitGTK C API có `webkit_user_content_filter_store_new()`. Crate
//! `webkit2gtk` (safe binding) không expose. Nhưng C symbol nằm trong
//! `libwebkit2gtk-4.1.so` — có thể gọi qua FFI.
//!
//! # Safety
//!
//! Chỉ dùng 1 extern symbol và 1 g_object_unref. Cả 2 đều null-check.

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
// Linux: FFI probe
// ============================================================================

#[cfg(target_os = "linux")]
pub mod ffi_probe {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_void};

    // ------------------------------------------------------------------------
    // WebKitGTK C API — chỉ khai báo symbol cần thiết cho probe.
    //
    // Link name: `webkit2gtk-4.1` (khớp pkg-config của WebKitGTK 4.1).
    // Nếu distro dùng soname khác, linker sẽ báo "cannot find -lwebkit2gtk-4.1".
    // Khi đó cần đổi tên (VD `webkit2gtk-4.0` cho WebKitGTK 4.0).
    // ------------------------------------------------------------------------

    #[link(name = "webkit2gtk-4.1")]
    extern "C" {
        fn webkit_user_content_filter_store_new(
            storage_path: *const c_char,
        ) -> *mut c_void;
    }

    #[link(name = "gobject-2.0")]
    extern "C" {
        fn g_object_unref(object: *mut c_void);
    }

    /// Probe: gọi `webkit_user_content_filter_store_new()` và log kết quả.
    ///
    /// - Return `Ok(())` nếu store được tạo (non-null).
    /// - Return `Err(...)` nếu store null hoặc path invalid.
    ///
    /// Hàm này không giữ store. Unref ngay sau khi tạo.
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
}

#[cfg(target_os = "linux")]
pub use ffi_probe::probe_content_filter_store;

#[cfg(not(target_os = "linux"))]
pub fn probe_content_filter_store() -> Result<(), String> {
    Err("Not supported on non-Linux".to_string())
}
