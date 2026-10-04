//! Network-level adblock via WebKit UserContentFilter (multi-filter).
//!
//! Vibird chạy 3 filter song song:
//!   - easylist.json        → ads (network chính)
//!   - easyprivacy.json     → trackers
//!   - fanboy_annoyance.json → cookie banner, popup annoyances
//!
//! Mỗi filter cache riêng biệt. FILTER_READY chỉ true khi TẤT CẢ load
//! xong. Tab đến khi đang build → queue, apply tất cả filter khi ready.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// True khi tất cả filter đã load xong. Alias cũ `FILTER_READY` giữ để
/// không phải sửa commands.rs.
pub static FILTER_READY: AtomicBool = AtomicBool::new(false);

pub struct ContentFilterState {
    applied: AtomicBool,
    resource_paths: Vec<PathBuf>,
}

impl ContentFilterState {
    pub fn new(resource_paths: Vec<PathBuf>) -> Self {
        Self {
            applied: AtomicBool::new(false),
            resource_paths,
        }
    }

    pub fn is_applied(&self) -> bool {
        self.applied.load(Ordering::Relaxed)
    }

    pub fn mark_applied(&self) {
        self.applied.store(true, Ordering::Relaxed);
    }

    pub fn resource_paths(&self) -> &[PathBuf] {
        &self.resource_paths
    }

    /// Back-compat: trả về path đầu tiên. Chỉ dùng cho code cũ chưa update.
    pub fn resource_path(&self) -> Option<&PathBuf> {
        self.resource_paths.first()
    }
}

// ============================================================================
// Linux FFI — multi-filter
// ============================================================================

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    use std::path::{Path, PathBuf};
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
    use std::sync::Mutex;

    const MAX_FILTERS: usize = 8;

    // Cache filter pointer theo slot. Slot rỗng = null.
    static FILTER_CACHE: [AtomicPtr<c_void>; MAX_FILTERS] = [
        AtomicPtr::new(ptr::null_mut()),
        AtomicPtr::new(ptr::null_mut()),
        AtomicPtr::new(ptr::null_mut()),
        AtomicPtr::new(ptr::null_mut()),
        AtomicPtr::new(ptr::null_mut()),
        AtomicPtr::new(ptr::null_mut()),
        AtomicPtr::new(ptr::null_mut()),
        AtomicPtr::new(ptr::null_mut()),
    ];

    // Batch state: cần Mutex vì callback fire song song, thứ tự không đảm bảo.
    struct Batch {
        target: usize,
        done: usize,
    }

    static BATCH: Mutex<Batch> = Mutex::new(Batch {
        target: 0,
        done: 0,
    });

    static SAVE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
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

    struct SaveJob {
        store: *mut c_void,
        filter_id: CString,
        cache_slot: usize,
    }

    struct LoadJob {
        store: *mut c_void,
        filter_id: CString,
        cache_slot: usize,
    }

    /// Unref tất cả pending manager. Gọi khi build fail toàn bộ hoặc done.
    unsafe fn drain_pending(apply_filters: bool) {
        let pending: Vec<usize> = std::mem::take(&mut *PENDING_MANAGERS.lock().unwrap());
        let count = pending.len();

        for addr in pending {
            let m = addr as *mut c_void;
            if m.is_null() {
                continue;
            }
            if apply_filters {
                // Apply tất cả slot có filter.
                for slot in 0..MAX_FILTERS {
                    let f = FILTER_CACHE[slot].load(Ordering::Acquire);
                    if !f.is_null() {
                        webkit_user_content_manager_add_filter(m, f);
                    }
                }
            }
            g_object_unref(m);
        }

        if count > 0 {
            log::info!(
                "Content filter: {} pending tab(s) {}",
                count,
                if apply_filters { "got filters" } else { "released (no filters)" }
            );
        }
    }

    /// Gọi khi 1 filter hoàn thành (save+load, success hay fail).
    /// An toàn khi gọi từ async callback ở GLib main thread.
    unsafe fn on_one_filter_finished() {
        let (done, target, is_last) = {
            let mut b = BATCH.lock().unwrap();
            b.done += 1;
            (b.done, b.target, b.done >= b.target)
        };

        log::info!("Content filter: {}/{} finished", done, target);

        if !is_last {
            return;
        }

        // Tất cả filter đã xử lý xong (success hoặc fail).
        // Kiểm tra có filter nào cache được không.
        let mut cached_count = 0usize;
        for slot in 0..MAX_FILTERS {
            if !FILTER_CACHE[slot].load(Ordering::Acquire).is_null() {
                cached_count += 1;
            }
        }

        if cached_count > 0 {
            crate::content_filter::FILTER_READY.store(true, Ordering::Release);
            log::info!(
                "Content filter: {} filter(s) cached, ready",
                cached_count
            );
            drain_pending(true);
        } else {
            log::warn!("Content filter: no filter loaded successfully");
            drain_pending(false);
        }

        SAVE_IN_FLIGHT.store(false, Ordering::Release);
    }

    unsafe extern "C" fn on_save_done(
        _source: *mut c_void,
        res: *mut c_void,
        user_data: *mut c_void,
    ) {
        if user_data.is_null() {
            on_one_filter_finished();
            return;
        }
        let job = Box::from_raw(user_data as *mut SaveJob);

        let mut error: *mut GError = ptr::null_mut();
        let filter =
            webkit_user_content_filter_store_save_finish(job.store, res, &mut error);

        if filter.is_null() {
            log_gerror("Content filter save failed", error);
            g_object_unref(job.store);
            on_one_filter_finished();
            return;
        }

        // save_finish trả filter full ref — ta không dùng, unref ngay.
        g_object_unref(filter);
        log::info!("Content filter: save done (slot {}), loading...", job.cache_slot);

        let store = job.store;
        let filter_id = job.filter_id.clone();
        let cache_slot = job.cache_slot;

        let load_job = Box::new(LoadJob {
            store,
            filter_id,
            cache_slot,
        });
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
            on_one_filter_finished();
            return;
        }
        let job = Box::from_raw(user_data as *mut LoadJob);

        let mut error: *mut GError = ptr::null_mut();
        let filter =
            webkit_user_content_filter_store_load_finish(job.store, res, &mut error);

        if !filter.is_null() {
            let slot = job.cache_slot;
            if slot < MAX_FILTERS {
                let prev = FILTER_CACHE[slot].swap(filter, Ordering::AcqRel);
                if !prev.is_null() {
                    // Không nên xảy ra vì mỗi slot chỉ build 1 lần.
                    g_object_unref(prev);
                }
                log::info!("Content filter: cached at slot {}", slot);
            } else {
                g_object_unref(filter);
            }
        } else {
            log_gerror("Content filter load failed", error);
        }

        g_object_unref(job.store);
        on_one_filter_finished();
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
}
    /// Apply nhiều filter cho 1 UserContentManager.
    ///
    /// Flow:
    ///   - Đã cache → apply tất cả slot có filter, unref manager, xong.
    ///   - Đang build → queue manager, return. Callback cuối sẽ drain.
    ///   - Chưa build → start batch. Với mỗi path:
    ///       + Path không tồn tại / empty → skip, tăng done counter ngay.
    ///       + Path OK → start save (async) → done counter tăng khi callback fire.
    ///   - Khi done >= target → finalize: apply cache cho pending managers,
    ///     set FILTER_READY, unref pending, reset SAVE_IN_FLIGHT.
    pub fn apply_filter_for_manager_multi(
        manager_ptr: *mut c_void,
        paths: &[PathBuf],
    ) -> Result<(), String> {
        if manager_ptr.is_null() {
            return Err("manager pointer is null".to_string());
        }
        if paths.is_empty() {
            return Err("no filter paths provided".to_string());
        }

        // ---------- Fast path: filter đã cache ----------
        if crate::content_filter::FILTER_READY.load(Ordering::Acquire) {
            let mut applied = 0usize;
            unsafe {
                for slot in 0..MAX_FILTERS {
                    let f = FILTER_CACHE[slot].load(Ordering::Acquire);
                    if !f.is_null() {
                        webkit_user_content_manager_add_filter(manager_ptr, f);
                        applied += 1;
                    }
                }
                g_object_unref(manager_ptr);
            }
            log::info!(
                "Content filter: applied {} cached filter(s) to new tab",
                applied
            );
            return Ok(());
        }

        // ---------- Đang build → queue ----------
        if SAVE_IN_FLIGHT.load(Ordering::Acquire) {
            let n = {
                let mut q = PENDING_MANAGERS.lock().unwrap();
                q.push(manager_ptr as usize);
                q.len()
            };
            log::info!(
                "Content filter: build in flight, tab queued ({} pending)",
                n
            );
            return Ok(());
        }

        // ---------- Bắt đầu build batch ----------
        let limit = paths.len().min(MAX_FILTERS);

        // Reset batch state. Set target = limit, done = 0.
        {
            let mut b = BATCH.lock().unwrap();
            b.target = limit;
            b.done = 0;
        }

        PENDING_MANAGERS.lock().unwrap().push(manager_ptr as usize);
        SAVE_IN_FLIGHT.store(true, Ordering::Release);

        for (idx, json_path) in paths.iter().take(limit).enumerate() {
            // ---- Validate path ----
            if !json_path.exists() {
                log::warn!(
                    "Content filter: slot {} skip — not found {:?}",
                    idx,
                    json_path
                );
                unsafe { on_one_filter_finished() };
                continue;
            }

            let json_bytes = match std::fs::read(json_path) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!(
                        "Content filter: slot {} skip — cannot read {:?}: {}",
                        idx,
                        json_path,
                        e
                    );
                    unsafe { on_one_filter_finished() };
                    continue;
                }
            };

            if json_bytes.is_empty() {
                log::warn!("Content filter: slot {} skip — empty JSON", idx);
                unsafe { on_one_filter_finished() };
                continue;
            }

            // ---- Filter ID versioned theo mtime ----
            // Cache compiled binary của WebKit cùng thư mục JSON. Nếu ID
            // không đổi mà JSON đổi → WebKit dùng cache cũ. Version theo
            // mtime để invalidate cache khi update list.
            let version = std::fs::metadata(json_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let stem = json_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("filter");
            let filter_id_str = format!("vibird-{}-v{}", stem, version);

            let store_dir = match json_path.parent() {
                Some(p) => p,
                None => {
                    log::warn!("Content filter: slot {} skip — no parent dir", idx);
                    unsafe { on_one_filter_finished() };
                    continue;
                }
            };

            let store_path_cstr = match CString::new(store_dir.to_string_lossy().as_bytes()) {
                Ok(c) => c,
                Err(_) => {
                    unsafe { on_one_filter_finished() };
                    continue;
                }
            };

            let store_ptr =
                unsafe { webkit_user_content_filter_store_new(store_path_cstr.as_ptr()) };
            if store_ptr.is_null() {
                log::warn!("Content filter: slot {} skip — store_new null", idx);
                unsafe { on_one_filter_finished() };
                continue;
            }

            let filter_id_c = match CString::new(filter_id_str.as_str()) {
                Ok(c) => c,
                Err(_) => {
                    unsafe { g_object_unref(store_ptr) };
                    unsafe { on_one_filter_finished() };
                    continue;
                }
            };

            let bytes_ptr = unsafe {
                g_bytes_new(json_bytes.as_ptr() as *const c_void, json_bytes.len())
            };
            if bytes_ptr.is_null() {
                unsafe { g_object_unref(store_ptr) };
                unsafe { on_one_filter_finished() };
                continue;
            }

            log::info!(
                "Content filter: start slot {} (id={}, {} bytes)",
                idx,
                filter_id_str,
                json_bytes.len()
            );

            let job = Box::new(SaveJob {
                store: store_ptr,
                filter_id: filter_id_c,
                cache_slot: idx,
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
        }

        // Không cần check `started == 0` ở đây:
        //  - Nếu mọi slot đều skip → mỗi lần skip đã gọi on_one_filter_finished,
        //    khi lần skip cuối cùng chạy xong → done == target → finalize tự
        //    động reset SAVE_IN_FLIGHT.
        //  - Nếu ít nhất 1 slot start save → callback sẽ tăng done.
        // Trường hợp `limit == 0` không xảy ra vì `paths.is_empty()` đã
        // return sớm ở đầu function.

        Ok(())
    }

    // ========================================================================
    // Internal helper — không public, dùng cho code test.
    // ========================================================================
    #[allow(dead_code)]
    pub fn get_cached_count() -> usize {
        let mut n = 0;
        for slot in 0..MAX_FILTERS {
            if !FILTER_CACHE[slot].load(Ordering::Acquire).is_null() {
                n += 1;
            }
        }
        n
    }
}

#[cfg(target_os = "linux")]
pub use linux::{apply_filter_for_manager_multi, probe_content_filter_store};

// ============================================================================
// Non-Linux stubs
// ============================================================================

#[cfg(not(target_os = "linux"))]
pub fn probe_content_filter_store() -> Result<(), String> {
    Err("Not supported on non-Linux".to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_filter_for_manager_multi(
    _manager: *mut std::os::raw::c_void,
    _paths: &[std::path::PathBuf],
) -> Result<(), String> {
    Err("Not supported on non-Linux".to_string())
}
