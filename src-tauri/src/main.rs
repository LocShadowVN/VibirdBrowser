#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adblock;
mod bridge;
mod commands;
mod crypto;
mod database;
mod dns;
mod downloader;
mod extensions;

use adblock::ShieldEngine;
use commands::{VaultSession, ViewportManager};
use database::DbManager;
use tauri::webview::WebviewWindowBuilder;
use tauri::WebviewUrl;

fn main() {
    #[cfg(target_os = "linux")]
    {
        let is_vm = std::fs::read_to_string("/proc/cpuinfo")
            .map(|s| s.contains("hypervisor"))
            .unwrap_or(false);
        if is_vm {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
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
            commands::load_session
        ])
        .run(tauri::generate_context!())
        .expect("Vibird Browser launch failure");
}
