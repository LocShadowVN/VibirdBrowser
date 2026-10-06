//! GPU detection + WebKitGTK workaround tuning.

#[derive(Debug)]
enum GpuVendor {
    Intel,
    Amd,
    Nvidia,
    Other,
}

#[cfg(target_os = "linux")]
pub fn apply_workarounds() {
    use std::fs;

    let is_wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|s| s.to_lowercase().contains("wayland"))
        .unwrap_or(false)
        || std::env::var("WAYLAND_DISPLAY").is_ok();

    let is_x11 = std::env::var("XDG_SESSION_TYPE")
        .map(|s| s.to_lowercase().contains("x11"))
        .unwrap_or(false)
        || (std::env::var("DISPLAY").is_ok() && !is_wayland);

    let is_vm = fs::read_to_string("/proc/cpuinfo")
        .map(|s| s.contains("hypervisor"))
        .unwrap_or(false);

    let vendor = detect_gpu_vendor();
    log::info!(
        "[gpu] vendor={:?}, session={}, vm={}",
        vendor,
        if is_wayland { "wayland" } else if is_x11 { "x11" } else { "unknown" },
        is_vm
    );

    // ========================================================================
    // Compositing + DMABUF mode
    //
    // WebKitGTK 4.1 trên Wayland native bị bug z-order và DMABUF. Workaround
    // cũ: tắt cả 2 → nhưng kết quả là software rendering → CPU spike 60%.
    //
    // Chiến lược mới:
    //   - Luôn chạy qua XWayland (GDK_BACKEND=x11) khi là Wayland session.
    //     XWayland đã ổn định z-order và hardware accel.
    //   - Compositing + DMABUF GIỮ NGUYÊN (bật) — vì XWayland native X11,
    //     không cần tắt.
    //   - Chỉ tắt khi chạy Wayland native thuần (không qua XWayland).
    //
    // User override bằng VIBIRD_FORCE_COMPOSITING:
    //   =0 → tắt compositing + DMABUF (cho troubleshooting)
    //   =1 → bật cả 2
    //   unset → auto (đã mô tả trên)
    // ========================================================================
    let force_comp = std::env::var("VIBIRD_FORCE_COMPOSITING").ok();

    match force_comp.as_deref() {
        Some("0") => {
            set_if_unset("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
            set_if_unset("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            log::info!("[gpu] compositing + DMABUF: forced OFF");
        }
        Some("1") => {
            std::env::remove_var("WEBKIT_DISABLE_COMPOSITING_MODE");
            std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER");
            log::info!("[gpu] compositing + DMABUF: forced ON");
        }
        _ => {
            // Wayland → dùng XWayland (GDK_BACKEND=x11) → hardware accel bật.
            // Không tắt compositing nữa.
            if is_wayland {
                log::info!("[gpu] compositing + DMABUF: ON (via XWayland)");
            } else {
                log::info!("[gpu] compositing + DMABUF: ON (X11 native)");
            }
        }
    }

    // Wayland → chạy qua XWayland để hardware accel hoạt động
    if is_wayland {
        set_if_unset("GDK_BACKEND", "x11");
    }

    match vendor {
        GpuVendor::Intel => {
            set_if_unset("MESA_GLTHREAD", "true");
            if is_vm {
                set_if_unset("INTEL_DEBUG", "norb");
            }
        }
        GpuVendor::Amd => {
            set_if_unset("RADV_DEBUG", "nosync");
        }
        GpuVendor::Nvidia => {
            // Bật threaded optimization cho XWayland + Nvidia.
            // Trước đây tắt vì sợ race, nhưng với driver ≥ 545 ổn định.
            set_if_unset("__GL_THREADED_OPTIMIZATIONS", "1");
            // Vsync off để giảm latency khi user scroll tab.
            set_if_unset("__GL_SYNC_TO_VBLANK", "0");
        }
        GpuVendor::Other => {}
    }

    if is_vm {
        log::info!("[gpu] VM detected — passthrough GPU may be unavailable");
    }
}

#[cfg(target_os = "linux")]
fn detect_gpu_vendor() -> GpuVendor {
    use std::fs;
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return GpuVendor::Other;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("card") || name_str.contains('-') {
            continue;
        }
        let vendor_path = entry.path().join("device/vendor");
        let Ok(vendor_str) = fs::read_to_string(&vendor_path) else {
            continue;
        };
        let vendor_clean = vendor_str.trim().to_lowercase();
        return match vendor_clean.as_str() {
            "0x8086" => GpuVendor::Intel,
            "0x1002" | "0x1022" => GpuVendor::Amd,
            "0x10de" => GpuVendor::Nvidia,
            _ => GpuVendor::Other,
        };
    }
    GpuVendor::Other
}

#[cfg(not(target_os = "linux"))]
pub fn apply_workarounds() {}

#[cfg(target_os = "linux")]
fn set_if_unset(key: &str, value: &str) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, value);
        log::info!("[gpu] set {}={}", key, value);
    }
}
