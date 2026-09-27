use crate::tauri_ipc::call_tauri;
use leptos::*;
use serde::{Deserialize, Serialize};
use shared::{AppConfig, DnsTestResult};

#[derive(Serialize)]
struct SettingArgs {
    key: String,
    value: String,
}

#[derive(Serialize)]
struct ShieldLevelArgs {
    level: String,
}

#[derive(Serialize)]
struct DohArgs {
    url: String,
}

#[derive(Serialize)]
struct EmptyArgs {}

#[derive(Serialize)]
struct ApplyUpdateArgs {
    download_url: String,
    asset_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_notes: String,
    pub download_url: String,
    pub asset_name: String,
    pub is_appimage: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Lang {
    Vi,
    En,
}

#[derive(Clone, PartialEq)]
enum SettingsTab {
    General,
    Shield,
    Dns,
    Appearance,
    About,
}

#[component]
pub fn SettingsView(
    config: ReadSignal<AppConfig>,
    set_config: WriteSignal<AppConfig>,
) -> impl IntoView {
    let (tab, set_tab) = create_signal(SettingsTab::General);
    let (doh_result, set_doh_result) = create_signal(String::new());
    let (doh_input, set_doh_input) = create_signal(String::new());

    create_effect(move |_| {
        let v = config.get().custom_doh_url;
        if doh_input.get_untracked().is_empty() && !v.is_empty() {
            set_doh_input.set(v);
        }
    });

    let (app_version, set_app_version) = create_signal("1.0.0".to_string());

    spawn_local(async move {
        if let Ok(v) = call_tauri::<_, String>("get_app_version", &EmptyArgs {}).await {
            set_app_version.set(v);
        }
    });

    let initial_lang = if let Some(w) = web_sys::window() {
        w.local_storage()
            .ok()
            .flatten()
            .and_then(|s| s.get_item("vibird_lang").ok().flatten())
            .map(|l| if l == "en" { Lang::En } else { Lang::Vi })
            .unwrap_or(Lang::Vi)
    } else {
        Lang::Vi
    };
    let (lang, set_lang) = create_signal(initial_lang);

    let t = move |vi: &'static str, en: &'static str| -> &'static str {
        if lang.get() == Lang::Vi { vi } else { en }
    };

    let (update_info, set_update_info) = create_signal(Option::<UpdateInfo>::None);
    let (checking_update, set_checking_update) = create_signal(false);
    let (updating, set_updating) = create_signal(false);
    let (update_status_msg, set_update_status_msg) = create_signal(String::new());
    let (update_ready_restart, set_update_ready_restart) = create_signal(false);

    let test_dns = move |_| {
        let endpoint = doh_input.get();
        set_doh_result.set(if lang.get() == Lang::Vi {
            "Đang kiểm tra...".into()
        } else {
            "Testing...".into()
        });
        spawn_local(async move {
            let res: Result<DnsTestResult, _> =
                call_tauri("test_doh", &DohArgs { url: endpoint.clone() }).await;
            match res {
                Ok(r) if r.success => {
                    set_doh_result.set(format!(
                        "{}: {} ms (IP: {})",
                        if lang.get() == Lang::Vi { "Độ trễ" } else { "Latency" },
                        r.latency_ms,
                        r.resolved_ip.unwrap_or_default()
                    ));
                    let _ = call_tauri::<_, ()>(
                        "update_setting",
                        &SettingArgs {
                            key: "custom_doh_url".into(),
                            value: endpoint,
                        },
                    )
                    .await;
                }
                Ok(r) => set_doh_result.set(format!(
                    "{}: {}",
                    if lang.get() == Lang::Vi { "Thất bại" } else { "Failed" },
                    r.error.unwrap_or_default()
                )),
                Err(e) => set_doh_result.set(format!(
                    "{}: {}",
                    if lang.get() == Lang::Vi { "Lỗi" } else { "Error" },
                    e
                )),
            }
        });
    };

    let do_check_update = move |_| {
        set_checking_update.set(true);
        set_update_status_msg.set(if lang.get() == Lang::Vi {
            "Đang kiểm tra từ máy chủ...".into()
        } else {
            "Checking server...".into()
        });
        spawn_local(async move {
            let res: Result<UpdateInfo, _> = call_tauri("check_for_updates", &EmptyArgs {}).await;
            set_checking_update.set(false);
            match res {
                Ok(info) => {
                    set_app_version.set(info.current_version.clone());
                    if info.has_update {
                        set_update_status_msg.set(format!(
                            "{}: v{}",
                            if lang.get() == Lang::Vi { "Phiên bản mới" } else { "New version" },
                            info.latest_version
                        ));
                    } else {
                        set_update_status_msg.set(if lang.get() == Lang::Vi {
                            "Bạn đang dùng bản mới nhất.".into()
                        } else {
                            "You are on the latest version.".into()
                        });
                    }
                    set_update_info.set(Some(info));
                }
                Err(e) => {
                    set_update_status_msg.set(format!(
                        "{}: {}",
                        if lang.get() == Lang::Vi { "Lỗi kiểm tra" } else { "Check failed" },
                        e
                    ));
                }
            }
        });
    };

    let do_apply_update = move |_| {
        if let Some(info) = update_info.get() {
            set_updating.set(true);
            set_update_status_msg.set(if lang.get() == Lang::Vi {
                "Đang tải dữ liệu...".into()
            } else {
                "Downloading update payload...".into()
            });
            let dl_url = info.download_url.clone();
            let asset = info.asset_name.clone();
            let is_appimage = info.is_appimage;

            spawn_local(async move {
                let res: Result<String, _> = call_tauri(
                    "apply_update",
                    &ApplyUpdateArgs {
                        download_url: dl_url,
                        asset_name: asset,
                    },
                )
                .await;

                set_updating.set(false);
                match res {
                    Ok(code) if code == "SUCCESS_APPIMAGE" => {
                        set_update_status_msg.set(if lang.get() == Lang::Vi {
                            "Cập nhật AppImage thành công. Hãy khởi động lại.".into()
                        } else {
                            "AppImage updated. Restart to apply.".into()
                        });
                        if is_appimage {
                            set_update_ready_restart.set(true);
                        }
                    }
                    Ok(code) if code.starts_with("SUCCESS_DEB:") => {
                        let path = code.trim_start_matches("SUCCESS_DEB:");
                        set_update_status_msg.set(format!(
                            "{}: {}\n{}",
                            if lang.get() == Lang::Vi { "Đã lưu tại" } else { "Saved to" },
                            path,
                            if lang.get() == Lang::Vi {
                                "Chạy lệnh: sudo dpkg -i <file>"
                            } else {
                                "Run: sudo dpkg -i <file>"
                            }
                        ));
                    }
                    Ok(other) => {
                        set_update_status_msg.set(other);
                    }
                    Err(e) => {
                        set_update_status_msg.set(format!(
                            "{}: {}",
                            if lang.get() == Lang::Vi { "Thất bại" } else { "Failed" },
                            e
                        ));
                    }
                }
            });
        }
    };

    let do_restart = move |_| {
        spawn_local(async move {
            let _ = call_tauri::<_, ()>("restart_browser", &EmptyArgs {}).await;
        });
    };

    view! {
        <div class="internal-view">
            <div class="settings-container">
                <aside class="settings-sidebar">
                    <button
                        class=move || format!(
                            "sidebar-btn {}",
                            if tab.get() == SettingsTab::General { "active" } else { "" },
                        )
                        on:click=move |_| set_tab.set(SettingsTab::General)
                    >
                        {move || t("Cài đặt chung", "General")}
                    </button>
                    <button
                        class=move || format!(
                            "sidebar-btn {}",
                            if tab.get() == SettingsTab::Shield { "active" } else { "" },
                        )
                        on:click=move |_| set_tab.set(SettingsTab::Shield)
                    >
                        "Vibird Shield"
                    </button>
                    <button
                        class=move || format!(
                            "sidebar-btn {}",
                            if tab.get() == SettingsTab::Dns { "active" } else { "" },
                        )
                        on:click=move |_| set_tab.set(SettingsTab::Dns)
                    >
                        {move || t("Bảo mật DNS", "Secure DNS")}
                    </button>
                    <button
                        class=move || format!(
                            "sidebar-btn {}",
                            if tab.get() == SettingsTab::Appearance { "active" } else { "" },
                        )
                        on:click=move |_| set_tab.set(SettingsTab::Appearance)
                    >
                        {move || t("Giao diện & Ngôn ngữ", "Appearance & Language")}
                    </button>
                    <button
                        class=move || format!(
                            "sidebar-btn {}",
                            if tab.get() == SettingsTab::About { "active" } else { "" },
                        )
                        on:click=move |_| set_tab.set(SettingsTab::About)
                    >
                        {move || t("Giới thiệu & Cập nhật", "About & Updates")}
                    </button>
                </aside>

                <main class="settings-content">
                    {move || match tab.get() {
                        SettingsTab::General => view! {
                            <div class="panel-card">
                                <h2>{move || t("Cài đặt chung", "General Preferences")}</h2>
                                <div class="grid-form">
                                    <label>{move || t("Công cụ tìm kiếm", "Default Search Engine")}</label>
                                    <select on:change=move |ev| {
                                        let val = event_target_value(&ev);
                                        let mut c = config.get();
                                        c.search_engine = val.clone();
                                        set_config.set(c);
                                        spawn_local(async move {
                                            let _ = call_tauri::<_, ()>(
                                                "update_setting",
                                                &SettingArgs {
                                                    key: "search_engine".into(),
                                                    value: val,
                                                },
                                            )
                                            .await;
                                        });
                                    }>
                                        <option value="https://search.brave.com/search?q=" selected=move || config.get().search_engine.contains("brave")>
                                            "Brave Search"
                                        </option>
                                        <option value="https://duckduckgo.com/?q=" selected=move || config.get().search_engine.contains("duckduckgo")>
                                            "DuckDuckGo"
                                        </option>
                                        <option value="https://www.google.com/search?q=" selected=move || config.get().search_engine.contains("google")>
                                            "Google"
                                        </option>
                                    </select>

                                    <label>{move || t("Thư mục tải về", "Downloads Save Directory")}</label>
                                    <input
                                        type="text"
                                        prop:value=move || config.get().download_path
                                        on:input=move |ev| {
                                            let val = event_target_value(&ev);
                                            let mut c = config.get();
                                            c.download_path = val.clone();
                                            set_config.set(c);
                                            spawn_local(async move {
                                                let _ = call_tauri::<_, ()>(
                                                    "update_setting",
                                                    &SettingArgs {
                                                        key: "download_path".into(),
                                                        value: val,
                                                    },
                                                )
                                                .await;
                                            });
                                        }
                                    />
                                </div>
                            </div>
                        }
                        .into_view(),

                        SettingsTab::Shield => view! {
                            <div class="panel-card">
                                <h2>"Vibird Shield Core"</h2>
                                <div class="grid-form">
                                    <label>{move || t("Cấp độ bảo vệ", "Protection Level")}</label>
                                    <select on:change=move |ev| {
                                        let val = event_target_value(&ev);
                                        let mut c = config.get();
                                        c.shield_level = val.clone();
                                        set_config.set(c);
                                        let val_clone = val.clone();
                                        spawn_local(async move {
                                            let _ = call_tauri::<_, ()>(
                                                "update_setting",
                                                &SettingArgs {
                                                    key: "shield_level".into(),
                                                    value: val,
                                                },
                                            )
                                            .await;
                                            let _ = call_tauri::<_, ()>(
                                                "set_shield_level",
                                                &ShieldLevelArgs { level: val_clone },
                                            )
                                            .await;
                                        });
                                    }>
                                        <option value="Standard" selected=move || config.get().shield_level == "Standard">
                                            {move || t(
                                                "Tiêu chuẩn (Chặn quảng cáo, theo dõi, banner cookie)",
                                                "Standard (Blocks ads, trackers, cookie dialogs)",
                                            )}
                                        </option>
                                        <option value="Aggressive" selected=move || config.get().shield_level == "Aggressive">
                                            {move || t(
                                                "Nghiêm ngặt (Chặn sâu, chống nhận diện vân tay máy)",
                                                "Aggressive (Strict blocking & Farbling)",
                                            )}
                                        </option>
                                        <option value="Off" selected=move || config.get().shield_level == "Off">
                                            {move || t("Tắt", "Disabled")}
                                        </option>
                                    </select>
                                </div>
                            </div>
                        }
                        .into_view(),

                        SettingsTab::Dns => view! {
                            <div class="panel-card">
                                <h2>"DNS-over-HTTPS (RFC 8484)"</h2>
                                <div class="grid-form">
                                    <label>{move || t("Địa chỉ máy chủ DoH", "DoH Endpoint")}</label>
                                    <input
                                        type="text"
                                        prop:value=doh_input
                                        on:input=move |ev| set_doh_input.set(event_target_value(&ev))
                                    />
                                    <div style="display:flex; gap:10px; align-items:center; margin-top:8px;">
                                        <button class="btn-action" on:click=test_dns>
                                            {move || t("Kiểm tra kết nối", "Test Latency")}
                                        </button>
                                        <span style="font-size:12px; color:var(--text-secondary); font-family:var(--font-mono);">
                                            {doh_result}
                                        </span>
                                    </div>
                                </div>
                            </div>
                        }
                        .into_view(),

                        SettingsTab::Appearance => view! {
                            <div class="panel-card">
                                <h2>{move || t("Giao diện & Ngôn ngữ", "Appearance & Language")}</h2>
                                <div class="grid-form">
                                    <label>{move || t("Ngôn ngữ hiển thị", "Interface Language")}</label>
                                    <select on:change=move |ev| {
                                        let val = event_target_value(&ev);
                                        let selected = if val == "en" { Lang::En } else { Lang::Vi };
                                        set_lang.set(selected);
                                        if let Some(w) = web_sys::window() {
                                            if let Ok(Some(s)) = w.local_storage() {
                                                let _ = s.set_item(
                                                    "vibird_lang",
                                                    if selected == Lang::En { "en" } else { "vi" },
                                                );
                                            }
                                        }
                                    }>
                                        <option value="vi" selected=move || lang.get() == Lang::Vi>
                                            "Tiếng Việt"
                                        </option>
                                        <option value="en" selected=move || lang.get() == Lang::En>
                                            "English"
                                        </option>
                                    </select>

                                    <label>{move || t("Chủ đề giao diện", "Color Theme")}</label>
                                    <select on:change=move |ev| {
                                        let val = event_target_value(&ev);
                                        let is_dark = val == "dark";
                                        let mut c = config.get();
                                        c.dark_theme = is_dark;
                                        set_config.set(c);
                                        spawn_local(async move {
                                            let _ = call_tauri::<_, ()>(
                                                "update_setting",
                                                &SettingArgs {
                                                    key: "dark_theme".into(),
                                                    value: if is_dark { "true".into() } else { "false".into() },
                                                },
                                            )
                                            .await;
                                        });
                                    }>
                                        <option value="dark" selected=move || config.get().dark_theme>
                                            {move || t("Tối (Titanium Dark)", "Dark (Titanium Dark)")}
                                        </option>
                                        <option value="light" selected=move || !config.get().dark_theme>
                                            {move || t("Sáng (Light)", "Light")}
                                        </option>
                                    </select>
                                </div>
                            </div>
                        }
                        .into_view(),

                        SettingsTab::About => view! {
                            <div class="panel-card">
                                <h2>{move || t("Thông tin & Cập nhật", "About & Updates")}</h2>

                                <div class="update-box">
                                    <div style="display:flex; justify-content:space-between; align-items:center;">
                                        <div>
                                            <div style="font-weight:700; font-size:14px;">"Vibird Browser"</div>
                                            <div style="font-size:12px; color:var(--text-secondary); margin-top:2px; font-family:var(--font-mono);">
                                                {move || format!(
                                                    "{}: v{}",
                                                    t("Phiên bản", "Version"),
                                                    app_version.get(),
                                                )}
                                            </div>
                                        </div>

                                        <button
                                            class="btn-action"
                                            disabled=move || checking_update.get() || updating.get()
                                            on:click=do_check_update
                                        >
                                            {move || if checking_update.get() {
                                                t("Đang kiểm tra...", "Checking...")
                                            } else {
                                                t("Kiểm tra cập nhật", "Check for updates")
                                            }}
                                        </button>
                                    </div>

                                    {move || {
                                        let msg = update_status_msg.get();
                                        if !msg.is_empty() {
                                            view! {
                                                <div class="update-status-badge">{msg}</div>
                                            }
                                                .into_view()
                                        } else {
                                            view! { <div style="display:none;"></div> }.into_view()
                                        }
                                    }}

                                    {move || update_info.get().and_then(|info| {
                                        if info.has_update {
                                            Some(view! {
                                                <div style="margin-top:14px; border-top:1px solid var(--border); padding-top:12px;">
                                                    <div style="font-size:11px; font-weight:700; text-transform:uppercase; color:var(--text-secondary); margin-bottom:6px;">
                                                        {t("Ghi chú bản phát hành", "Release Notes")}
                                                    </div>
                                                    <div class="changelog-box">{info.release_notes}</div>

                                                    <div style="margin-top:12px;">
                                                        {if update_ready_restart.get() {
                                                            view! {
                                                                <button class="btn-action" on:click=do_restart>
                                                                    {t("Khởi động lại ứng dụng", "Restart Application")}
                                                                </button>
                                                            }
                                                                .into_view()
                                                        } else {
                                                            view! {
                                                                <button
                                                                    class="btn-action"
                                                                    disabled=move || updating.get()
                                                                    on:click=do_apply_update
                                                                >
                                                                    {if updating.get() {
                                                                        t("Đang tải xuống...", "Downloading...")
                                                                    } else {
                                                                        t("Cập nhật ngay", "Update now")
                                                                    }}
                                                                </button>
                                                            }
                                                                .into_view()
                                                        }}
                                                    </div>
                                                </div>
                                            })
                                        } else {
                                            None
                                        }
                                    })}
                                </div>

                                <div style="margin-top:20px; font-size:11px; color:var(--text-secondary); line-height:1.6; font-family:var(--font-mono);">
                                    "WebKitGTK 4.1 • Tauri v2 • Leptos CSR WASM"<br />
                                    "GNU General Public License v3.0"
                                </div>
                            </div>
                        }
                        .into_view(),
                    }}
                </main>
            </div>
        </div>
    }
}
