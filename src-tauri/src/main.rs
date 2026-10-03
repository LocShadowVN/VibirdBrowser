#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adblock;
mod bridge;
mod commands;
mod content_filter;
mod crypto;
mod database;
mod dns;
mod downloader;
mod extensions;

use adblock::ShieldEngine;
use commands::{VaultSession, ViewportManager};
use content_filter::ContentFilterState;
use database::DbManager;
use tauri::webview::WebviewWindowBuilder;
use tauri::{Manager, WebviewUrl};

fn main() {
    // ========================================================================
    // WebKitGTK env setup — PHẢI chạy TRƯỚC khi init bất cứ thứ gì WebKit.
    //
    // Vì sao luôn set (không chỉ VM):
    //   - UI chrome (main webview) chỉ cao NAV_BAR_HEIGHT = 118px.
    //   - Content webview (tab_*) phủ bên dưới.
    //   - Nếu compositing bật, content webview có GL layer riêng → trên
    //     Wayland và một số GPU config, layer này vẽ đè lên UI chrome.
    //   - Triệu chứng: khoảng đen YouTube, iframe Maps chen lên omnibox.
    //
    // Trade-off: tắt compositing giảm FPS nhẹ khi scroll trang nặng.
    // Chấp nhận được — đổi lấy UI không vỡ.
    // ========================================================================
    #[cfg(target_os = "linux")]
    {
        let is_wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|s| s.to_lowercase().contains("wayland"))
            .unwrap_or(false)
            || std::env::var("WAYLAND_DISPLAY").is_ok();

        let is_vm = std::fs::read_to_string("/proc/cpuinfo")
            .map(|s| s.contains("hypervisor"))
            .unwrap_or(false);

        // 1. Tắt compositing — fix UI overlap.
        if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }

        // 2. Tắt DMABUF renderer — fix flicker + overlap trên Intel/AMD.
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }

        // 3. Trên Wayland, ép X11 backend. WebKitGTK native Wayland
        //    không expose API ổn định để set z-order giữa 2 webview.
        if is_wayland && std::env::var_os("GDK_BACKEND").is_none() {
            std::env::set_var("GDK_BACKEND", "x11");
        }

        if is_vm {
            log::info!("VM detected — all WebKit compositing workarounds applied");
        }
        if is_wayland {
            log::info!("Wayland session — GDK_BACKEND=x11 applied");
        }
    }

    env_logger::init();

    let db = DbManager::init();
    let shield = ShieldEngine::new();
    let vp_manager = ViewportManager::new();
    let vault_session = VaultSession::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(db)
        .manage(shield)
        .manage(vp_manager)
        .manage(vault_session)
        .setup(|app| {
            // ================================================================
            // Content filter: resolve resource path
            // ================================================================
            let resource_path = app
                .path()
                .resolve(
                    "resources/easylist_content_blocker.json",
                    tauri::path::BaseDirectory::Resource,
                )
                .ok()
                .filter(|p| p.exists());

            if let Some(ref p) = resource_path {
                log::info!("Content filter resource found at {:?}", p);
            } else {
                log::warn!(
                    "Content filter resource not found — network-level adblock disabled"
                );
            }

            app.manage(ContentFilterState::new(resource_path));

            // ================================================================
            // FFI probe — chỉ chạy trong debug build, tránh tạo temp dir rác
            // mỗi lần khởi động ở bản release.
            // ================================================================
            #[cfg(all(target_os = "linux", debug_assertions))]
            {
                match content_filter::probe_content_filter_store() {
                    Ok(_) => log::info!("=== Content filter FFI probe PASSED ==="),
                    Err(e) => log::warn!("=== Content filter FFI probe FAILED: {} ===", e),
                }
            }

            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Vibird Browser")
                .inner_size(1400.0, 900.0)
                .min_inner_size(950.0, 650.0)
                .resizable(true)
                .build()?;

            let app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::Resized(phys) = event {
                    let handle = app_handle.clone();
                    let p_size = *phys;
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::commands::handle_window_resize(&handle, p_size).await;
                    });
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_version,
            commands::check_for_updates,
            commands::apply_update,
            commands::restart_browser,
            commands::open_native_tab,
            commands::switch_tab_view,
            commands::close_native_tab,
            commands::snooze_tab,
            commands::expand_ui_for_menu,
            commands::get_site_shield,
            commands::toggle_site_shield,
            commands::start_multithread_download,
            commands::check_vault_credentials_for_domain,
            commands::execute_autofill,
            commands::webview_go_back,
            commands::webview_go_forward,
            commands::webview_reload,
            commands::webview_zoom_by,
            commands::find_in_page,
            commands::clear_site_data,
            commands::report_tab_title,
            commands::check_shield,
            commands::set_shield_level,
            commands::resolve_url,
            commands::fetch_web_page,
            commands::record_history,
            commands::fetch_history,
            commands::clear_history,
            commands::save_bookmark,
            commands::fetch_bookmarks,
            commands::remove_bookmark,
            commands::fetch_downloads,
            commands::clear_downloads,
            commands::remove_download,
            commands::open_file_manager,
            commands::fetch_extensions,
            commands::load_unpacked_extension,
            commands::toggle_extension,
            commands::remove_extension,
            commands::test_doh,
            commands::vault_is_configured,
            commands::vault_setup,
            commands::vault_save_credential,
            commands::vault_read_all,
            commands::vault_delete,
            commands::vault_lock,
            commands::generate_password,
            commands::get_settings,
            commands::update_setting,
            commands::get_shield_stats,
            commands::get_shield_stats_detailed,
            commands::fetch_site_exceptions,
            commands::add_shield_exception,
            commands::remove_shield_exception,
            commands::report_shield_block,
            commands::increment_blocked_stat,
            commands::toggle_devtools,
            commands::save_session,
            commands::load_session,
            commands::query_omnibox_suggestions
        ])
        .run(tauri::generate_context!())
        .expect("Vibird Browser launch failure");
}
