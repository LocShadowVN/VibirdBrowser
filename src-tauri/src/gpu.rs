//! GPU detection + WebKitGTK workaround tuning.
//!
//! Chiến lược:
//!   - Hybrid (iGPU + Nvidia): force iGPU → tránh bug Nvidia + Wayland.
//!   - Chỉ Nvidia: tắt DMABUF, giữ compositing → giảm artifact.
//!   - Chỉ iGPU (Intel/AMD): không cần workaround đặc biệt.
//!
//! Override bằng VIBIRD_FORCE_GPU:
//!   =igpu   → force iGPU dù có Nvidia
//!   =nvidia → force Nvidia (không khuyến nghị)
//!   unset   → auto (hybrid → iGPU, chỉ Nvidia → DMABUF off)

#[derive(Debug, Clone, Copy, PartialEq)]
enum GpuVendor {
    Intel,
    Amd,
    Nvidia,
    Other,
}

#[cfg(target_os = "linux")]
fn detect_all_gpus() -> Vec<GpuVendor> {
    use std::fs;
    let mut vendors = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return vendors;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Chỉ card chính, bỏ connector (card0-DP-1, ...)
        if !name_str.starts_with("card") || name_str.contains('-') {
            continue;
        }
        let vendor_path = entry.path().join("device/vendor");
        let Ok(v) = fs::read_to_string(&vendor_path) else {
            continue;
        };
        let v = v.trim().to_lowercase();
        let vendor = match v.as_str() {
            "0x8086" => GpuVendor::Intel,
            "0x1002" | "0x1022" => GpuVendor::Amd,
            "0x10de" => GpuVendor::Nvidia,
            _ => GpuVendor::Other,
        };
        if !vendors.contains(&vendor) {
            vendors.push(vendor);
        }
    }
    vendors
}

/// Trả về số card (0, 1, ...) của iGPU nếu có.
#[cfg(target_os = "linux")]
fn find_igpu_card_number() -> Option<u32> {
    use std::fs;
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return None;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("card") || name_str.contains('-') {
            continue;
        }
        // Lấy số từ "card0" -> 0
        let num_str = name_str.trim_start_matches("card");
        let Ok(num) = num_str.parse::<u32>() else {
            continue;
        };
        let vendor_path = entry.path().join("device/vendor");
        let Ok(v) = fs::read_to_string(&vendor_path) else {
            continue;
        };
        let v = v.trim().to_lowercase();
        if v == "0x8086" || v == "0x1002" || v == "0x1022" {
            return Some(num);
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub fn apply_workarounds() {
    use std::fs;

    let is_wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|s| s.to_lowercase().contains("wayland"))
        .unwrap_or(false)
        || std::env::var("WAYLAND_DISPLAY").is_ok();

    let is_vm = fs::read_to_string("/proc/cpuinfo")
        .map(|s| s.contains("hypervisor"))
        .unwrap_or(false);

    let vendors = detect_all_gpus();
    let has_nvidia = vendors.contains(&GpuVendor::Nvidia);
    let has_igpu = vendors.contains(&GpuVendor::Intel) || vendors.contains(&GpuVendor::Amd);
    let is_hybrid = has_nvidia && has_igpu;

    log::info!(
        "[gpu] vendors={:?}, hybrid={}, session={}, vm={}",
        vendors,
        is_hybrid,
        if is_wayland { "wayland" } else { "x11" },
        is_vm
    );

    // ================================================================
    // Compositing baseline
    // ================================================================
    match std::env::var("VIBIRD_FORCE_COMPOSITING").ok().as_deref() {
        Some("0") => {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            log::info!("[gpu] compositing + DMABUF: forced OFF");
        }
        _ => {
            std::env::remove_var("WEBKIT_DISABLE_COMPOSITING_MODE");
            // DMABUF quyết định bên dưới theo vendor.
            log::info!("[gpu] compositing: ON");
        }
    }

    // Wayland → XWayland (ổn định hơn cho Nvidia proprietary)
    if is_wayland {
        set_if_unset("GDK_BACKEND", "x11");
    }

    // ================================================================
    // GPU selection
    // ================================================================
    let force = std::env::var("VIBIRD_FORCE_GPU").ok();
    let use_igpu = match force.as_deref() {
        Some("igpu") => {
            if has_igpu {
                true
            } else {
                log::warn!("[gpu] VIBIRD_FORCE_GPU=igpu but no iGPU detected — ignoring");
                false
            }
        }
        Some("nvidia") => false,
        _ => is_hybrid, // auto: hybrid → iGPU
    };

    if use_igpu {
        log::info!("[gpu] forcing iGPU (Intel/AMD) — bypassing Nvidia");

        // Remove Nvidia offload trigger — driver check existence, không check value.
        std::env::remove_var("__NV_PRIME_RENDER_OFFLOAD");

        // Force Mesa GLX vendor.
        std::env::set_var("__GLX_VENDOR_LIBRARY_NAME", "mesa");

        // Vulkan: chỉ cho layer non-Nvidia chạy.
        std::env::set_var("__VK_LAYER_NV_optimus", "non_nvidia_only");

        // Mesa GBM backend.
        std::env::set_var("GBM_BACKEND", "drm");

        // DRI_PRIME: chỉ set khi iGPU KHÔNG phải card0.
        // Nếu iGPU là card0 → đã là primary, set DRI_PRIME sẽ chọn
        // card1 (Nvidia) → ngược ý muốn.
        if let Some(igpu_num) = find_igpu_card_number() {
            if igpu_num != 0 {
                std::env::set_var("DRI_PRIME", igpu_num.to_string());
                log::info!("[gpu] DRI_PRIME={} (iGPU=card{})", igpu_num, igpu_num);
            } else {
                std::env::remove_var("DRI_PRIME");
                log::info!("[gpu] iGPU is card0 (primary), DRI_PRIME unset");
            }
        }

        // DMABUF hoạt động tốt trên Mesa → bật.
        std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER");

    } else if has_nvidia {
        log::info!("[gpu] Nvidia-only mode — DMABUF off, compositing on");

        // Nvidia + WebKitGTK → DMABUF sinh artifact (khoảng xám).
        // Tắt DMABUF, giữ compositing để hardware accel vẫn chạy.
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("__GL_THREADED_OPTIMIZATIONS", "1");
        std::env::set_var("__GL_SYNC_TO_VBLANK", "0");

    } else {
        // Intel/AMD only → DMABUF bật mặc định.
        std::env::remove_var("WEBKIT_DISABLE_DMABUF_RENDERER");
    }

    // ================================================================
    // Vendor-specific tuning (chỉ khi KHÔNG force iGPU)
    // ================================================================
    if !use_igpu {
        if vendors.contains(&GpuVendor::Intel) {
            set_if_unset("MESA_GLTHREAD", "true");
            if is_vm {
                set_if_unset("INTEL_DEBUG", "norb");
            }
        }
        if vendors.contains(&GpuVendor::Amd) {
            set_if_unset("RADV_DEBUG", "nosync");
        }
    }

    if is_vm {
        log::info!("[gpu] VM detected — passthrough GPU may be unavailable");
    }
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
