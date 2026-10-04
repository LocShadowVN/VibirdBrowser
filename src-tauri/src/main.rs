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
mod gpu;

use adblock::ShieldEngine;
use commands::{VaultSession, ViewportManager};
use content_filter::ContentFilterState;
use database::DbManager;
use tauri::webview::WebviewWindowBuilder;
use tauri::{Manager, WebviewUrl};

fn main() {
    // env_logger init trước để log từ gpu::apply_workarounds() hoạt động.
    env_logger::init();

    // ========================================================================
    // WebKitGTK env setup — PHẢI chạy TRƯỚC khi init bất cứ thứ gì WebKit.
    //
    // Tuning per-vendor nằm trong gpu.rs (Intel/AMD/NVIDIA + VM).
    // ========================================================================
    #[cfg(target_os = "linux")]
    {
        gpu::apply_workarounds();
    }

    // Log env var để verify.
    #[cfg(target_os = "linux")]
    {
        log::info!(
            "[env] WEBKIT_DISABLE_COMPOSITING_MODE={:?}",
            std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE")
        );
        log::info!(
            "[env] WEBKIT_DISABLE_DMABUF_RENDERER={:?}",
            std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER")
        );
        log::info!("[env] GDK_BACKEND={:?}", std::env::var("GDK_BACKEND"));
        log::info!(
            "[env] XDG_SESSION_TYPE={:?}",
            std::env::var("XDG_SESSION_TYPE")
        );
    }

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
            commands::query_omnibox_suggestions,
            commands::is_filter_ready
            commands::set_chrome_height,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download
        ])
        ])
        .run(tauri::generate_context!())
        .expect("Vibird Browser launch failure");
}
