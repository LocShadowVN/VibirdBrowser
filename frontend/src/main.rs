mod icons;
mod tauri_ipc;
mod views;

use icons::*;
use leptos::*;
use serde::{Deserialize, Serialize};
use shared::{AppConfig, BookmarkRecord, DownloadProgressPayload, SiteCredential};
use tauri_ipc::call_tauri;
use views::{
    bookmarks::BookmarksView, downloads::DownloadsView, extensions::ExtensionsView,
    history::HistoryView, newtab::NewTabView, settings::SettingsView,
    shields::ShieldsView, vault::VaultView,
};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

#[derive(Serialize)]
struct EmptyArgs {}

#[derive(Serialize)]
struct ResolveArgs {
    raw: String,
    engine: String,
}

#[derive(Serialize)]
struct OpenNativeTabArgs {
    tab_id: String,
    url: String,
    is_incognito: bool,
}

#[derive(Serialize)]
struct SwitchTabArgs {
    active_tab_id: String,
    is_internal: bool,
    all_tab_ids: Vec<String>,
}

#[derive(Serialize)]
struct CloseNativeTabArgs {
    tab_id: String,
}

#[derive(Serialize)]
struct SnoozeTabArgs {
    tab_id: String,
}

#[derive(Serialize)]
struct MenuExpandArgs {
    expanded: bool,
}

#[derive(Serialize)]
struct SaveBookmarkArgs {
    url: String,
    title: String,
}

#[derive(Serialize)]
struct GetSiteShieldArgs {
    domain: String,
}

#[derive(Serialize)]
struct ToggleSiteShieldArgs {
    domain: String,
    enabled: bool,
}

#[derive(Serialize)]
struct CheckVaultDomainArgs {
    domain: String,
}

#[derive(Serialize)]
struct ExecuteAutofillArgs {
    username: String,
    secret: String,
}

#[derive(Serialize)]
struct FindArgs {
    query: String,
    forward: bool,
    reset: bool,
}

#[derive(Serialize)]
struct ReloadArgs {
    hard: bool,
}

#[derive(Serialize)]
struct ZoomArgs {
    delta: f64,
    reset: bool,
}

#[derive(Serialize)]
struct DownloadArgs {
    url: String,
    connections: Option<usize>,
}

#[derive(Serialize)]
struct SaveSessionArgs {
    snapshot: SessionSnapshotFE,
}

#[derive(Serialize)]
struct OmniboxQueryArgs {
    query: String,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct SetChromeHeightArgs {
    height: f64,
}

#[derive(Serialize)]
struct DownloadTaskArgs {
    task_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct SessionSnapshotFE {
    tabs: Vec<SessionTabFE>,
    active_index: usize,
}

#[derive(Clone, Serialize, Deserialize)]
struct SessionTabFE {
    url: String,
    title: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PageNavigationState {
    pub tab_id: String,
    pub url: String,
    pub title: Option<String>,
    pub is_loading: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ShieldBlockedPayload {
    pub tab_id: String,
    pub count: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NewTabPayload {
    pub url: String,
    pub background: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct FindResultPayload {
    pub count: i32,
    pub current: i32,
    pub supported: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct ContextMenuPayload {
    pub action: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OmniboxSuggestionFE {
    pub url: String,
    pub title: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PageMode {
    Web,
    NewTab,
    Settings,
    History,
    Bookmarks,
    Downloads,
    Extensions,
    Vault,
    Shields,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrowserTab {
    pub id: String,
    pub url: String,
    pub title: String,
    pub blocked_count: u32,
    pub history: Vec<String>,
    pub history_index: usize,
    pub page_mode: PageMode,
    pub is_snoozed: bool,
    pub is_incognito: bool,
    pub is_loading: bool,
    pub last_active: f64,
}

fn extract_domain(url_str: &str) -> String {
    if url_str.starts_with("vibird://") || url_str.starts_with("caram://") {
        return "Vibird System".into();
    }
    if let Ok(u) = web_sys::Url::new(url_str) {
        let h = u.hostname();
        if !h.is_empty() {
            return h;
        }
    }
    url_str.to_string()
}

fn is_internal_url(u: &str) -> bool {
    u.starts_with("vibird://") || u.starts_with("caram://") || u.starts_with("about:")
}

fn classify_internal(url: &str) -> Option<PageMode> {
    match url {
        "vibird://newtab" | "caram://newtab" => Some(PageMode::NewTab),
        "vibird://settings" | "caram://settings" => Some(PageMode::Settings),
        "vibird://history" | "caram://history" => Some(PageMode::History),
        "vibird://bookmarks" | "caram://bookmarks" => Some(PageMode::Bookmarks),
        "vibird://downloads" | "caram://downloads" => Some(PageMode::Downloads),
        "vibird://extensions" | "caram://extensions" => Some(PageMode::Extensions),
        "vibird://passwords" | "caram://passwords" => Some(PageMode::Vault),
        "vibird://shields" | "caram://shields" => Some(PageMode::Shields),
        _ => None,
    }
}

fn load_lang_from_storage() -> String {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("vibird_lang").ok().flatten())
        .unwrap_or_else(|| "en".to_string())
}

#[component]
fn App() -> impl IntoView {
    let (tab_counter, set_tab_counter) = create_signal(1u64);
    let (tabs, set_tabs) = create_signal(vec![BrowserTab {
        id: "tab_1".into(),
        url: "vibird://newtab".into(),
        title: "New Tab".into(),
        blocked_count: 0,
        history: vec!["vibird://newtab".into()],
        history_index: 0,
        page_mode: PageMode::NewTab,
        is_snoozed: false,
        is_incognito: false,
        is_loading: false,
        last_active: js_sys::Date::now(),
    }]);

    let (active_tab_id, set_active_tab_id) = create_signal("tab_1".to_string());
    let (omnibox_text, set_omnibox_text) = create_signal(String::new());
    let (omnibox_focused, set_omnibox_focused) = create_signal(false);
    let (omnibox_suggestions, set_omnibox_suggestions) =
        create_signal(Vec::<OmniboxSuggestionFE>::new());
    let (omnibox_sel_index, set_omnibox_sel_index) = create_signal(-1i32);

    let (shield_open, set_shield_open) = create_signal(false);
    let (menu_open, set_menu_open) = create_signal(false);
    let (find_open, set_find_open) = create_signal(false);
    let (find_query, set_find_query) = create_signal(String::new());
    let (find_count, set_find_count) = create_signal(0i32);
    let (find_current, set_find_current) = create_signal(0i32);
    let (find_supported, set_find_supported) = create_signal(true);

    let (shield_flyout_pos, set_shield_flyout_pos) = create_signal((0.0_f64, 0.0_f64));

    let (bookmarks, set_bookmarks) = create_signal(Vec::<BookmarkRecord>::new());
    let (current_site_shield, set_current_site_shield) = create_signal(true);
    let (available_credentials, set_available_credentials) =
        create_signal(Vec::<SiteCredential>::new());
    let (active_download, set_active_download) =
        create_signal(Option::<DownloadProgressPayload>::None);

    let (config, set_config) = create_signal(AppConfig::default());

    let (pending_new_tab, set_pending_new_tab) = create_signal(None::<(String, bool)>);
    let (last_new_tab_at, set_last_new_tab_at) = create_signal(0.0f64);

    let (lang, set_lang) = create_signal(load_lang_from_storage());
    provide_context((lang, set_lang));

    let tr = move |vi: &'static str, en: &'static str| -> &'static str {
        if lang.get() == "vi" {
            vi
        } else {
            en
        }
    };

    spawn_local(async move {
        if let Ok(cfg) = call_tauri::<_, AppConfig>("get_settings", &EmptyArgs {}).await {
            set_config.set(cfg);
        }
        if let Ok(bm) = call_tauri::<_, Vec<BookmarkRecord>>("fetch_bookmarks", &EmptyArgs {}).await
        {
            set_bookmarks.set(bm);
        }

        if let Ok(Some(snap)) =
            call_tauri::<_, Option<SessionSnapshotFE>>("load_session", &EmptyArgs {}).await
        {
            if !snap.tabs.is_empty() {
                let active_idx = snap.active_index.min(snap.tabs.len().saturating_sub(1));

                let mut list = Vec::<BrowserTab>::new();
                let mut counter = 0u64;

                for (i, st) in snap.tabs.iter().enumerate() {
                    counter += 1;
                    let is_active = i == active_idx;
                    let url = st.url.clone();
                    let mode = classify_internal(&url).unwrap_or(PageMode::Web);

                    list.push(BrowserTab {
                        id: format!("tab_{}", counter),
                        url: url.clone(),
                        title: st.title.clone(),
                        blocked_count: 0,
                        history: vec![url.clone()],
                        history_index: 0,
                        page_mode: mode.clone(),
                        is_snoozed: mode == PageMode::Web && !is_active,
                        is_incognito: false,
                        is_loading: false,
                        last_active: js_sys::Date::now(),
                    });
                }

                set_tab_counter.set(counter);

                let active_id = list[active_idx].id.clone();
                let active_url = list[active_idx].url.clone();
                let active_is_web = !is_internal_url(&active_url);
                let all_ids: Vec<String> = list.iter().map(|t| t.id.clone()).collect();

                set_tabs.set(list);
                set_active_tab_id.set(active_id.clone());
                set_omnibox_text.set(if active_is_web {
                    active_url.clone()
                } else {
                    String::new()
                });

                if active_is_web {
                    let _ = call_tauri::<_, ()>(
                        "open_native_tab",
                        &OpenNativeTabArgs {
                            tab_id: active_id,
                            url: active_url,
                            is_incognito: false,
                        },
                    )
                    .await;
                } else {
                    let _ = call_tauri::<_, ()>(
                        "switch_tab_view",
                        &SwitchTabArgs {
                            active_tab_id: active_id,
                            is_internal: true,
                            all_tab_ids: all_ids,
                        },
                    )
                    .await;
                }
            }
        }
    });

    create_effect(move |_| {
        let is_dark = config.get().dark_theme;
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            if let Some(body) = doc.body() {
                let _ = body.set_attribute("data-theme", if is_dark { "dark" } else { "light" });
            }
        }
    });

    // ========================================================================
    // Đo chiều cao UI chrome (tabs + nav + bookmark strip) và báo backend.
    //
    // Đợi `document.fonts.ready` trước khi đo để tránh layout shift khi
    // font load xong (Be Vietnam Pro load async). Đo sai → backend đặt
    // content webview sai → khoảng xám trên cùng.
    //
    // Cộng 1px buffer + ceil để tránh gap sub-pixel.
    // ========================================================================
    create_effect(move |_| {
        let _ = bookmarks.get();
        let _ = tabs.get();
        let _ = config.get();

        spawn_local(async move {
            // Đợi font load xong.
            if let Some(w) = web_sys::window() {
                if let Some(doc) = w.document() {
                    let fonts = doc.fonts();
                    let _ = wasm_bindgen_futures::JsFuture::from(fonts.ready()).await;
                }
            }

            // Đợi thêm 1 frame để browser flush layout sau font swap.
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                if let Some(w) = web_sys::window() {
                    let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                        &resolve,
                        80,
                    );
                }
            });
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;

            let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
                return;
            };
            let Ok(Some(shell)) = doc.query_selector(".browser-shell") else {
                return;
            };
            let Ok(Some(vp)) = doc.query_selector(".viewport-body") else {
                return;
            };

            let shell_top = shell.get_bounding_client_rect().top();
            let vp_top = vp.get_bounding_client_rect().top();
            let h = vp_top - shell_top;

            // +1px buffer để chắc chắn không hở sub-pixel gap.
            let h_final = (h + 1.0).ceil();

            if h_final > 40.0 && h_final < 300.0 {
                let _ = call_tauri::<_, ()>(
                    "set_chrome_height",
                    &SetChromeHeightArgs { height: h_final },
                )
                .await;
            }
        });
    });

    create_effect(move |_| {
        let has_sugg = omnibox_focused.get() && !omnibox_suggestions.get().is_empty();
        let open = shield_open.get() || menu_open.get() || has_sugg;
        spawn_local(async move {
            let _ = call_tauri::<_, ()>(
                "expand_ui_for_menu",
                &MenuExpandArgs { expanded: open },
            )
            .await;
        });
    });

    {
        let session_sig = create_memo(move |_| {
            let list = tabs.get();
            let active = active_tab_id.get();
            let ids: Vec<String> = list.iter().map(|t| t.id.clone()).collect();
            (ids, active)
        });

        create_effect(move |_| {
            let _ = session_sig.get();

            let list = tabs.get_untracked();
            let active = active_tab_id.get_untracked();

            let filtered: Vec<SessionTabFE> = list
                .iter()
                .filter(|t| !t.is_incognito)
                .map(|t| SessionTabFE {
                    url: t.url.clone(),
                    title: t.title.clone(),
                })
                .collect();

            let active_index = list.iter().position(|t| t.id == active).unwrap_or(0);

            let snapshot = SessionSnapshotFE {
                tabs: filtered,
                active_index,
            };

            spawn_local(async move {
                let _ = call_tauri::<_, ()>("save_session", &SaveSessionArgs { snapshot }).await;
            });
        });
    }

    // === Listener: download progress ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload_val) =
                js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
            {
                if let Ok(prog) =
                    serde_wasm_bindgen::from_value::<DownloadProgressPayload>(payload_val)
                {
                    set_active_download.set(Some(prog));
                }
            }
        }) as Box<dyn FnMut(JsValue)>);
        let _ = tauri_ipc::listen("download-progress", cb.as_ref().unchecked_ref()).await;
        cb.forget();
    });

    // === Listener: shield blocked count ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload_val) =
                js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
            {
                if let Ok(p) =
                    serde_wasm_bindgen::from_value::<ShieldBlockedPayload>(payload_val)
                {
                    let mut list = tabs.get();
                    if let Some(tab) = list.iter_mut().find(|t| t.id == p.tab_id) {
                        tab.blocked_count = tab.blocked_count.saturating_add(p.count);
                    }
                    set_tabs.set(list);
                }
            }
        }) as Box<dyn FnMut(JsValue)>);
        let _ = tauri_ipc::listen("shield-blocked", cb.as_ref().unchecked_ref()).await;
        cb.forget();
    });

    // === Listener: update-installing ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |_event_obj: JsValue| {
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                if let Some(el) = doc.query_selector(".update-status-badge").ok().flatten() {
                    if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                        html_el.set_inner_text(
                            "Installing. Enter your password when prompted...",
                        );
                    }
                }
            }
        }) as Box<dyn FnMut(JsValue)>);
        let _ = tauri_ipc::listen("update-installing", cb.as_ref().unchecked_ref()).await;
        cb.forget();
    });

    // === Listener: update-installed ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |_event_obj: JsValue| {
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                if let Some(el) = doc.query_selector(".update-status-badge").ok().flatten() {
                    if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                        html_el.set_inner_text("Update installed. Restarting...");
                    }
                }
            }
        }) as Box<dyn FnMut(JsValue)>);
        let _ = tauri_ipc::listen("update-installed", cb.as_ref().unchecked_ref()).await;
        cb.forget();
    });

    // === Listener: update-failed ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |event_obj: JsValue| {
            let payload = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "Unknown error".to_string());
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                if let Some(el) = doc.query_selector(".update-status-badge").ok().flatten() {
                    if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                        html_el.set_inner_text(&format!("Update failed: {}", payload));
                    }
                }
            }
        }) as Box<dyn FnMut(JsValue)>);
        let _ = tauri_ipc::listen("update-failed", cb.as_ref().unchecked_ref()).await;
        cb.forget();
    });

    // === Listener: find-result ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload_val) =
                js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
            {
                if let Ok(p) = serde_wasm_bindgen::from_value::<FindResultPayload>(payload_val) {
                    set_find_count.set(p.count);
                    set_find_current.set(p.current);
                    set_find_supported.set(p.supported);
                }
            }
        }) as Box<dyn FnMut(JsValue)>);
        let _ = tauri_ipc::listen("find-result", cb.as_ref().unchecked_ref()).await;
        cb.forget();
    });

    // === Listener: open-new-tab ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload_val) =
                js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
            {
                if let Ok(p) = serde_wasm_bindgen::from_value::<NewTabPayload>(payload_val) {
                    if p.url.starts_with("http://") || p.url.starts_with("https://") {
                        let now = js_sys::Date::now();
                        if now - last_new_tab_at.get_untracked() < 200.0 {
                            return;
                        }
                        set_last_new_tab_at.set(now);
                        set_pending_new_tab.set(Some((p.url, p.background)));
                    }
                }
            }
        }) as Box<dyn FnMut(JsValue)>);
        let _ = tauri_ipc::listen("open-new-tab", cb.as_ref().unchecked_ref()).await;
        cb.forget();
    });

    // === Effect: process pending new tab ===
    create_effect(move |_| {
        let Some((url, background)) = pending_new_tab.get() else {
            return;
        };
        set_pending_new_tab.set(None);

        let mut list = tabs.get_untracked();
        let next_counter = tab_counter.get_untracked() + 1;
        set_tab_counter.set(next_counter);
        let new_id = format!("tab_{}", next_counter);

        list.push(BrowserTab {
            id: new_id.clone(),
            url: url.clone(),
            title: url.clone(),
            blocked_count: 0,
            history: vec![url.clone()],
            history_index: 0,
            page_mode: PageMode::Web,
            is_snoozed: false,
            is_incognito: false,
            is_loading: true,
            last_active: js_sys::Date::now(),
        });

        if !background {
            set_active_tab_id.set(new_id.clone());
            set_omnibox_text.set(url.clone());
        }
        set_tabs.set(list);

        let all_ids: Vec<String> = tabs
            .get_untracked()
            .iter()
            .map(|t| t.id.clone())
            .collect();
        spawn_local(async move {
            if !background {
                let _ = call_tauri::<_, ()>(
                    "switch_tab_view",
                    &SwitchTabArgs {
                        active_tab_id: new_id.clone(),
                        is_internal: false,
                        all_tab_ids: all_ids,
                    },
                )
                .await;
            }
            let _ = call_tauri::<_, ()>(
                "open_native_tab",
                &OpenNativeTabArgs {
                    tab_id: new_id,
                    url,
                    is_incognito: false,
                },
            )
            .await;
        });
    });

    let sync_site_state = move |target_url: &str| {
        let domain = extract_domain(target_url);
        if domain == "Vibird System" || domain.is_empty() {
            set_available_credentials.set(Vec::new());
            return;
        }
        let d1 = domain.clone();
        let d2 = domain;
        spawn_local(async move {
            if let Ok(enabled) =
                call_tauri::<_, bool>("get_site_shield", &GetSiteShieldArgs { domain: d1 }).await
            {
                set_current_site_shield.set(enabled);
            }
            if let Ok(creds) = call_tauri::<_, Vec<SiteCredential>>(
                "check_vault_credentials_for_domain",
                &CheckVaultDomainArgs { domain: d2 },
            )
            .await
            {
                set_available_credentials.set(creds);
            }
        });
    };

    // === Listener: tab navigation state ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload_val) =
                js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
            {
                if let Ok(state) =
                    serde_wasm_bindgen::from_value::<PageNavigationState>(payload_val)
                {
                    let mut list = tabs.get();
                    if let Some(tab) = list.iter_mut().find(|t| t.id == state.tab_id) {
                        tab.is_loading = state.is_loading;
                        if !state.url.is_empty() {
                            tab.url = state.url.clone();
                        }
                        if let Some(t) = state.title {
                            if !t.is_empty() {
                                tab.title = t;
                            }
                        }
                    }
                    set_tabs.set(list);

                    if active_tab_id.get() == state.tab_id && !is_internal_url(&state.url) {
                        if !omnibox_focused.get() {
                            set_omnibox_text.set(state.url.clone());
                        }
                        sync_site_state(&state.url);
                    }
                }
            }
        }) as Box<dyn FnMut(JsValue)>);
        let _ = tauri_ipc::listen("tab-navigation-state", cb.as_ref().unchecked_ref()).await;
        cb.forget();
    });

    // === Smart Tab Snoozer ===
    spawn_local(async move {
        loop {
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                if let Some(w) = web_sys::window() {
                    let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                        &resolve,
                        30_000,
                    );
                }
            });
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;

            let now = js_sys::Date::now();
            let cur_active = active_tab_id.get();
            let mut list = tabs.get();
            let mut changed = false;

            for t in list.iter_mut() {
                if t.id != cur_active
                    && !t.is_snoozed
                    && !is_internal_url(&t.url)
                    && (now - t.last_active > 600_000.0)
                {
                    t.is_snoozed = true;
                    changed = true;
                    let id_c = t.id.clone();
                    spawn_local(async move {
                        let _ = call_tauri::<_, ()>(
                            "snooze_tab",
                            &SnoozeTabArgs { tab_id: id_c },
                        )
                        .await;
                    });
                }
            }

            if changed {
                set_tabs.set(list);
            }
        }
    });
        let navigate = move |target_url: String, record_history: bool| {
        let engine = config.get().search_engine;
        spawn_local(async move {
            set_menu_open.set(false);
            set_shield_open.set(false);
            set_find_open.set(false);
            set_omnibox_suggestions.set(Vec::new());
            set_omnibox_sel_index.set(-1);

            let cur_id = active_tab_id.get();
            let mut list = tabs.get();
            let tab_opt = list.iter_mut().find(|x| x.id == cur_id);
            if tab_opt.is_none() {
                return;
            }
            let tab = tab_opt.unwrap();
            tab.last_active = js_sys::Date::now();
            tab.is_snoozed = false;
            let incognito = tab.is_incognito;

            let target = target_url.trim().to_string();

            let is_internal_route = match target.as_str() {
                "vibird://newtab" | "caram://newtab" => {
                    tab.url = target.clone();
                    tab.title = if incognito {
                        "Incognito Tab".into()
                    } else {
                        "New Tab".into()
                    };
                    tab.page_mode = PageMode::NewTab;
                    true
                }
                "vibird://settings" | "caram://settings" => {
                    tab.url = target.clone();
                    tab.title = "Settings".into();
                    tab.page_mode = PageMode::Settings;
                    true
                }
                "vibird://history" | "caram://history" => {
                    tab.url = target.clone();
                    tab.title = "History".into();
                    tab.page_mode = PageMode::History;
                    true
                }
                "vibird://bookmarks" | "caram://bookmarks" => {
                    tab.url = target.clone();
                    tab.title = "Bookmarks".into();
                    tab.page_mode = PageMode::Bookmarks;
                    true
                }
                "vibird://downloads" | "caram://downloads" => {
                    tab.url = target.clone();
                    tab.title = "Downloads".into();
                    tab.page_mode = PageMode::Downloads;
                    true
                }
                "vibird://extensions" | "caram://extensions" => {
                    tab.url = target.clone();
                    tab.title = "Extensions".into();
                    tab.page_mode = PageMode::Extensions;
                    true
                }
                "vibird://passwords" | "caram://passwords" => {
                    tab.url = target.clone();
                    tab.title = "Password Vault".into();
                    tab.page_mode = PageMode::Vault;
                    true
                }
                "vibird://shields" | "caram://shields" => {
                    tab.url = target.clone();
                    tab.title = "Shields".into();
                    tab.page_mode = PageMode::Shields;
                    true
                }
                _ => false,
            };

            if is_internal_route {
                if record_history && !incognito {
                    tab.history.truncate(tab.history_index + 1);
                    tab.history.push(target.clone());
                    tab.history_index = tab.history.len() - 1;
                }
                set_tabs.set(list);
                set_omnibox_text.set(if is_internal_url(&target) && target.ends_with("newtab") {
                    String::new()
                } else {
                    target
                });

                let all_ids: Vec<String> = tabs.get().iter().map(|t| t.id.clone()).collect();
                let _ = call_tauri::<_, ()>(
                    "switch_tab_view",
                    &SwitchTabArgs {
                        active_tab_id: cur_id,
                        is_internal: true,
                        all_tab_ids: all_ids,
                    },
                )
                .await;
                return;
            }

            let resolved: String = call_tauri(
                "resolve_url",
                &ResolveArgs {
                    raw: target.clone(),
                    engine,
                },
            )
            .await
            .unwrap_or(target);

            tab.url = resolved.clone();
            tab.title = resolved.clone();
            tab.page_mode = PageMode::Web;
            tab.is_loading = true;

            if record_history && !incognito {
                tab.history.truncate(tab.history_index + 1);
                tab.history.push(resolved.clone());
                tab.history_index = tab.history.len() - 1;
            }
            set_tabs.set(list);
            set_omnibox_text.set(resolved.clone());
            sync_site_state(&resolved);

            let _ = call_tauri::<_, ()>(
                "open_native_tab",
                &OpenNativeTabArgs {
                    tab_id: cur_id,
                    url: resolved,
                    is_incognito: incognito,
                },
            )
            .await;
        });
    };

    // === Listener: context menu actions ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload_val) =
                js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
            {
                if let Ok(p) =
                    serde_wasm_bindgen::from_value::<ContextMenuPayload>(payload_val)
                {
                    match p.action.as_str() {
                        "open_link_new_tab" => {
                            if let Some(url) = p.url {
                                set_pending_new_tab.set(Some((url, false)));
                            }
                        }
                        "open_link_bg" => {
                            if let Some(url) = p.url {
                                set_pending_new_tab.set(Some((url, true)));
                            }
                        }
                        "search_selection" => {
                            if let Some(text) = p.text {
                                let engine = config.get_untracked().search_engine;
                                let encoded = js_sys::encode_uri_component(&text)
                                    .as_string()
                                    .unwrap_or_default();
                                let url = if engine.contains("%s") {
                                    engine.replace("%s", &encoded)
                                } else {
                                    format!("{}{}", engine, encoded)
                                };
                                navigate(url, true);
                            }
                        }
                        "save_image" => {
                            if let Some(url) = p.url {
                                spawn_local(async move {
                                    let _ = call_tauri::<_, String>(
                                        "start_multithread_download",
                                        &DownloadArgs {
                                            url,
                                            connections: None,
                                        },
                                    )
                                    .await;
                                });
                            }
                        }
                        "back" => {
                            let cur = active_tab_id.get_untracked();
                            let list = tabs.get_untracked();
                            if let Some(tab) = list.iter().find(|t| t.id == cur) {
                                if tab.page_mode == PageMode::Web {
                                    spawn_local(async move {
                                        let _ = call_tauri::<_, ()>(
                                            "webview_go_back",
                                            &EmptyArgs {},
                                        )
                                        .await;
                                    });
                                }
                            }
                        }
                        "forward" => {
                            let cur = active_tab_id.get_untracked();
                            let list = tabs.get_untracked();
                            if let Some(tab) = list.iter().find(|t| t.id == cur) {
                                if tab.page_mode == PageMode::Web {
                                    spawn_local(async move {
                                        let _ = call_tauri::<_, ()>(
                                            "webview_go_forward",
                                            &EmptyArgs {},
                                        )
                                        .await;
                                    });
                                }
                            }
                        }
                        "reload" => {
                            spawn_local(async move {
                                let _ = call_tauri::<_, ()>(
                                    "webview_reload",
                                    &ReloadArgs { hard: false },
                                )
                                .await;
                            });
                        }
                        "inspect_element" => {
                            spawn_local(async move {
                                let _ = call_tauri::<_, ()>("toggle_devtools", &EmptyArgs {})
                                    .await;
                            });
                        }
                        _ => {}
                    }
                }
            }
        }) as Box<dyn FnMut(JsValue)>);
        let _ = tauri_ipc::listen("context-menu-action", cb.as_ref().unchecked_ref()).await;
        cb.forget();
    });

    let create_new_tab = move |incognito: bool| {
        let mut list = tabs.get();
        let next_counter = tab_counter.get() + 1;
        set_tab_counter.set(next_counter);
        let new_id = format!("tab_{}", next_counter);
        list.push(BrowserTab {
            id: new_id.clone(),
            url: "vibird://newtab".into(),
            title: if incognito {
                "Incognito Tab".into()
            } else {
                "New Tab".into()
            },
            blocked_count: 0,
            history: vec!["vibird://newtab".into()],
            history_index: 0,
            page_mode: PageMode::NewTab,
            is_snoozed: false,
            is_incognito: incognito,
            is_loading: false,
            last_active: js_sys::Date::now(),
        });
        set_tabs.set(list);
        set_active_tab_id.set(new_id.clone());
        set_omnibox_text.set(String::new());
        set_omnibox_suggestions.set(Vec::new());
        set_omnibox_sel_index.set(-1);

        let all_ids: Vec<String> = tabs.get().iter().map(|t| t.id.clone()).collect();
        spawn_local(async move {
            let _ = call_tauri::<_, ()>(
                "switch_tab_view",
                &SwitchTabArgs {
                    active_tab_id: new_id,
                    is_internal: true,
                    all_tab_ids: all_ids,
                },
            )
            .await;
        });
    };

    // === Keyboard shortcuts ===
    {
        let window = web_sys::window().unwrap();
        let key_closure = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
            let ctrl = e.ctrl_key() || e.meta_key();
            let key = e.key();

            if ctrl {
                match key.to_lowercase().as_str() {
                    "t" => {
                        e.prevent_default();
                        create_new_tab(e.shift_key());
                    }
                    "w" => {
                        e.prevent_default();
                        let cur_id = active_tab_id.get();
                        let mut t_list = tabs.get();
                        if t_list.len() > 1 {
                            let del_index = t_list.iter().position(|x| x.id == cur_id);
                            t_list.retain(|x| x.id != cur_id);
                            let next_idx = del_index.unwrap_or(1).saturating_sub(1);
                            let next_tab = &t_list[next_idx];
                            set_active_tab_id.set(next_tab.id.clone());
                            set_omnibox_text.set(if is_internal_url(&next_tab.url) {
                                String::new()
                            } else {
                                next_tab.url.clone()
                            });
                            set_tabs.set(t_list);
                            spawn_local(async move {
                                let _ = call_tauri::<_, ()>(
                                    "close_native_tab",
                                    &CloseNativeTabArgs { tab_id: cur_id },
                                )
                                .await;
                            });
                        }
                    }
                    "l" => {
                        e.prevent_default();
                        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                            if let Some(input) =
                                doc.query_selector(".omnibox-input").ok().flatten()
                            {
                                if let Ok(el) = input.dyn_into::<web_sys::HtmlInputElement>() {
                                    let _ = el.focus();
                                    el.select();
                                }
                            }
                        }
                    }
                    "r" => {
                        e.prevent_default();
                        let hard = e.shift_key();
                        spawn_local(async move {
                            let _ =
                                call_tauri::<_, ()>("webview_reload", &ReloadArgs { hard }).await;
                        });
                    }
                    "+" | "=" => {
                        e.prevent_default();
                        spawn_local(async move {
                            let _ = call_tauri::<_, ()>(
                                "webview_zoom_by",
                                &ZoomArgs { delta: 0.1, reset: false },
                            )
                            .await;
                        });
                    }
                    "-" => {
                        e.prevent_default();
                        spawn_local(async move {
                            let _ = call_tauri::<_, ()>(
                                "webview_zoom_by",
                                &ZoomArgs { delta: -0.1, reset: false },
                            )
                            .await;
                        });
                    }
                    "0" => {
                        e.prevent_default();
                        spawn_local(async move {
                            let _ = call_tauri::<_, ()>(
                                "webview_zoom_by",
                                &ZoomArgs { delta: 0.0, reset: true },
                            )
                            .await;
                        });
                    }
                    "f" => {
                        e.prevent_default();
                        set_find_open.set(!find_open.get());
                    }
                    "h" => {
                        e.prevent_default();
                        navigate("vibird://history".into(), true);
                    }
                    "j" => {
                        e.prevent_default();
                        navigate("vibird://downloads".into(), true);
                    }
                    "tab" => {
                        e.prevent_default();
                        let cur_id = active_tab_id.get();
                        let list = tabs.get();
                        if let Some(idx) = list.iter().position(|t| t.id == cur_id) {
                            let next_idx = if e.shift_key() {
                                if idx == 0 { list.len() - 1 } else { idx - 1 }
                            } else {
                                (idx + 1) % list.len()
                            };
                            let target_tab = &list[next_idx];
                            let next_id = target_tab.id.clone();
                            set_active_tab_id.set(next_id.clone());
                            set_omnibox_text.set(if is_internal_url(&target_tab.url) {
                                String::new()
                            } else {
                                target_tab.url.clone()
                            });
                        }
                    }
                    _ => {}
                }
            } else if key == "F5" {
                e.prevent_default();
                spawn_local(async move {
                    let _ = call_tauri::<_, ()>("webview_reload", &ReloadArgs { hard: false }).await;
                });
            } else if key == "Escape" {
                set_find_open.set(false);
                set_shield_open.set(false);
                set_menu_open.set(false);
                set_omnibox_suggestions.set(Vec::new());
                set_omnibox_sel_index.set(-1);
            }
        }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);

        let _ = window
            .add_event_listener_with_callback("keydown", key_closure.as_ref().unchecked_ref());
        key_closure.forget();
    }

    // === Omnibox debounced query — generation counter ===
    let omnibox_query_generation = store_value(0u64);

    let trigger_omnibox_query = move |q: String| {
        let q_trim = q.trim().to_string();
        if q_trim.is_empty() {
            set_omnibox_suggestions.set(Vec::new());
            set_omnibox_sel_index.set(-1);
            return;
        }
        if q_trim.starts_with("http://") || q_trim.starts_with("https://") {
            set_omnibox_suggestions.set(Vec::new());
            set_omnibox_sel_index.set(-1);
            return;
        }

        let my_gen = omnibox_query_generation.get_value() + 1;
        omnibox_query_generation.set_value(my_gen);

        wasm_bindgen_futures::spawn_local(async move {
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                if let Some(w) = web_sys::window() {
                    let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                        &resolve, 120,
                    );
                }
            });
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;

            let res: Result<Vec<OmniboxSuggestionFE>, String> = call_tauri(
                "query_omnibox_suggestions",
                &OmniboxQueryArgs {
                    query: q_trim,
                    limit: Some(8),
                },
            )
            .await;

            if omnibox_query_generation.get_value() != my_gen {
                return;
            }

            match res {
                Ok(list) => {
                    set_omnibox_suggestions.set(list);
                    set_omnibox_sel_index.set(-1);
                }
                Err(_) => {
                    set_omnibox_suggestions.set(Vec::new());
                    set_omnibox_sel_index.set(-1);
                }
            }
        });
    };

    // Clone helper closures cho view! macro.
    let tr_nav = tr;
    let tr_menu = tr;
    let tr_shield = tr;
    let tr_find = tr;

    let create_new_tab_for_menu = create_new_tab;
    let navigate_for_menu = navigate;
    let navigate_for_find = navigate;
    let navigate_for_bookmarks = navigate;
    let navigate_for_newtab = navigate;
    let navigate_for_history = navigate;

    let _ = trigger_omnibox_query;

    view! {
        <div class="browser-shell">
            <header class="tabs-strip">
                <div class="tabs-list">
                    {move || tabs.get().into_iter().map(|tab| {
                        let id = tab.id.clone();
                        let id_del = tab.id.clone();
                        let id_snooze = tab.id.clone();
                        let active = tab.id == active_tab_id.get();
                        let snoozed = tab.is_snoozed;
                        let incognito = tab.is_incognito;
                        view! {
                            <div
                                class=format!(
                                    "tab-chip {} {} {}",
                                    if active { "active" } else { "" },
                                    if snoozed { "snoozed" } else { "" },
                                    if incognito { "incognito" } else { "" }
                                )
                                on:click=move |_| {
                                    let id_c = id.clone();
                                    set_active_tab_id.set(id_c.clone());
                                    let mut list = tabs.get();
                                    let is_int = list
                                        .iter()
                                        .find(|t| t.id == id_c)
                                        .map(|t| is_internal_url(&t.url))
                                        .unwrap_or(true);
                                    let cur_url = list
                                        .iter()
                                        .find(|t| t.id == id_c)
                                        .map(|t| t.url.clone())
                                        .unwrap_or_default();
                                    let was_snoozed = list
                                        .iter()
                                        .find(|t| t.id == id_c)
                                        .map(|t| t.is_snoozed)
                                        .unwrap_or(false);
                                    let is_inc = list
                                        .iter()
                                        .find(|t| t.id == id_c)
                                        .map(|t| t.is_incognito)
                                        .unwrap_or(false);

                                    if let Some(t) = list.iter_mut().find(|t| t.id == id_c) {
                                        t.last_active = js_sys::Date::now();
                                        t.is_snoozed = false;
                                    }
                                    set_tabs.set(list);

                                    set_omnibox_text.set(if is_internal_url(&cur_url) {
                                        String::new()
                                    } else {
                                        cur_url.clone()
                                    });
                                    set_omnibox_suggestions.set(Vec::new());
                                    set_omnibox_sel_index.set(-1);
                                    sync_site_state(&cur_url);

                                    let all_ids: Vec<String> =
                                        tabs.get().iter().map(|t| t.id.clone()).collect();
                                    spawn_local(async move {
                                        let _ = call_tauri::<_, ()>(
                                            "switch_tab_view",
                                            &SwitchTabArgs {
                                                active_tab_id: id_c.clone(),
                                                is_internal: is_int,
                                                all_tab_ids: all_ids,
                                            },
                                        )
                                        .await;
                                        if was_snoozed && !is_int {
                                            let _ = call_tauri::<_, ()>(
                                                "open_native_tab",
                                                &OpenNativeTabArgs {
                                                    tab_id: id_c.clone(),
                                                    url: cur_url,
                                                    is_incognito: is_inc,
                                                },
                                            )
                                            .await;
                                        }
                                    });
                                }
                            >
                                {if incognito {
                                    view! {
                                        <span style="margin-right:4px; font-size:10px; font-weight:700; color:#c4b5fd;">
                                            "[INC]"
                                        </span>
                                    }.into_view()
                                } else {
                                    view! { <span style="display:none;"></span> }.into_view()
                                }}
                                <span>{tab.title}</span>

                                {if !active && !snoozed && !is_internal_url(&tab.url) {
                                    view! {
                                        <div
                                            class="btn-tab-snooze"
                                            title="Snooze tab"
                                            on:click=move |ev| {
                                                ev.stop_propagation();
                                                let id_s = id_snooze.clone();
                                                let mut t_list = tabs.get();
                                                if let Some(t) =
                                                    t_list.iter_mut().find(|x| x.id == id_s)
                                                {
                                                    t.is_snoozed = true;
                                                }
                                                set_tabs.set(t_list);
                                                spawn_local(async move {
                                                    let _ = call_tauri::<_, ()>(
                                                        "snooze_tab",
                                                        &SnoozeTabArgs { tab_id: id_s },
                                                    )
                                                    .await;
                                                });
                                            }
                                        >
                                            "Z"
                                        </div>
                                    }.into_view()
                                } else {
                                    view! { <div style="display:none;"></div> }.into_view()
                                }}

                                <div
                                    class="btn-tab-close"
                                    on:click=move |ev| {
                                        ev.stop_propagation();
                                        let mut t_list = tabs.get();
                                        if t_list.len() > 1 {
                                            let del_id = id_del.clone();
                                            let del_index =
                                                t_list.iter().position(|x| x.id == del_id);
                                            t_list.retain(|x| x.id != del_id);
                                            if active_tab_id.get() == del_id {
                                                let next_idx =
                                                    del_index.unwrap_or(1).saturating_sub(1);
                                                let next_tab = &t_list[next_idx];
                                                set_active_tab_id.set(next_tab.id.clone());
                                                set_omnibox_text.set(
                                                    if is_internal_url(&next_tab.url) {
                                                        String::new()
                                                    } else {
                                                        next_tab.url.clone()
                                                    },
                                                );
                                            }
                                            set_tabs.set(t_list);
                                            spawn_local(async move {
                                                let _ = call_tauri::<_, ()>(
                                                    "close_native_tab",
                                                    &CloseNativeTabArgs { tab_id: del_id },
                                                )
                                                .await;
                                            });
                                        }
                                    }
                                >
                                    <IconClose />
                                </div>
                            </div>
                        }
                    }).collect_view()}

                    // Nút + tab mới inline cạnh tab cuối (đã ở trái).
                    <button
                        class="tab-new-btn"
                        title=move || tr_nav("Tab mới (Ctrl+T)", "New Tab (Ctrl+T)")
                        on:click=move |_| create_new_tab(false)
                    >
                        <IconPlus />
                    </button>
                </div>
            </header>

            <div class="nav-bar">
                <button
                    class="icon-btn"
                    title=move || tr_nav("Quay lại", "Go Back")
                    on:click=move |_| {
                        let cur = active_tab_id.get();
                        let mut list = tabs.get();
                        if let Some(tab) = list.iter_mut().find(|t| t.id == cur) {
                            if tab.page_mode == PageMode::Web {
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>(
                                        "webview_go_back",
                                        &EmptyArgs {},
                                    )
                                    .await;
                                });
                            } else if tab.history_index > 0 {
                                tab.history_index -= 1;
                                let prev = tab.history[tab.history_index].clone();
                                set_tabs.set(list);
                                navigate(prev, false);
                            }
                        }
                    }
                >
                    <IconBack />
                </button>

                <button
                    class="icon-btn"
                    title=move || tr_nav("Tiến tới", "Go Forward")
                    on:click=move |_| {
                        let cur = active_tab_id.get();
                        let mut list = tabs.get();
                        if let Some(tab) = list.iter_mut().find(|t| t.id == cur) {
                            if tab.page_mode == PageMode::Web {
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>(
                                        "webview_go_forward",
                                        &EmptyArgs {},
                                    )
                                    .await;
                                });
                            } else if tab.history_index + 1 < tab.history.len() {
                                tab.history_index += 1;
                                let next = tab.history[tab.history_index].clone();
                                set_tabs.set(list);
                                navigate(next, false);
                            }
                        }
                    }
                >
                    <IconForward />
                </button>

                <button
                    class="icon-btn"
                    title=move || tr_nav("Tải lại (Ctrl+R)", "Reload (Ctrl+R)")
                    on:click=move |_| {
                        let cur = active_tab_id.get();
                        let list = tabs.get();
                        if let Some(tab) = list.into_iter().find(|t| t.id == cur) {
                            if tab.page_mode == PageMode::Web {
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>(
                                        "webview_reload",
                                        &ReloadArgs { hard: false },
                                    )
                                    .await;
                                });
                            } else {
                                navigate(tab.url, false);
                            }
                        }
                    }
                >
                    <IconReload />
                </button>

                <div class="omnibox-box">
                    <div class="lock-icon"><IconLock /></div>
                    <input
                        type="text"
                        class="omnibox-input"
                        placeholder=move || tr_nav(
                            "Tìm kiếm hoặc nhập địa chỉ (Ctrl+L)",
                            "Search or enter address (Ctrl+L)",
                        )
                        prop:value=omnibox_text
                        on:focus=move |_| set_omnibox_focused.set(true)
                        on:blur=move |_| {
                            set_omnibox_focused.set(false);
                            set_omnibox_suggestions.set(Vec::new());
                            set_omnibox_sel_index.set(-1);
                        }
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            set_omnibox_text.set(v.clone());
                            trigger_omnibox_query(v);
                        }
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            let suggs = omnibox_suggestions.get_untracked();
                            let has_sugg = !suggs.is_empty();

                            match ev.key().as_str() {
                                "Enter" => {
                                    ev.prevent_default();
                                    let cur_sel = omnibox_sel_index.get_untracked();
                                    if has_sugg && cur_sel >= 0 {
                                        let idx = cur_sel as usize;
                                        if let Some(s) = suggs.get(idx) {
                                            let url = s.url.clone();
                                            set_omnibox_suggestions.set(Vec::new());
                                            set_omnibox_sel_index.set(-1);
                                            navigate(url, true);
                                            return;
                                        }
                                    }
                                    set_omnibox_suggestions.set(Vec::new());
                                    set_omnibox_sel_index.set(-1);
                                    navigate(omnibox_text.get_untracked(), true);
                                }
                                "ArrowDown" => {
                                    if has_sugg {
                                        ev.prevent_default();
                                        let cur_sel = omnibox_sel_index.get_untracked();
                                        let next = if cur_sel + 1 >= suggs.len() as i32 {
                                            0
                                        } else {
                                            cur_sel + 1
                                        };
                                        set_omnibox_sel_index.set(next);
                                    }
                                }
                                "ArrowUp" => {
                                    if has_sugg {
                                        ev.prevent_default();
                                        let cur_sel = omnibox_sel_index.get_untracked();
                                        let next = if cur_sel <= 0 {
                                            suggs.len() as i32 - 1
                                        } else {
                                            cur_sel - 1
                                        };
                                        set_omnibox_sel_index.set(next);
                                    }
                                }
                                "Escape" => {
                                    if has_sugg {
                                        ev.prevent_default();
                                        set_omnibox_suggestions.set(Vec::new());
                                        set_omnibox_sel_index.set(-1);
                                    }
                                }
                                _ => {}
                            }
                        }
                    />

                    {move || {
                        let creds = available_credentials.get();
                        if !creds.is_empty() {
                            let first = creds[0].clone();
                            view! {
                                <button
                                    class="autofill-btn"
                                    title=format!("Autofill as {}", first.username)
                                    on:click=move |_| {
                                        let u = first.username.clone();
                                        let s = first.secret.clone();
                                        spawn_local(async move {
                                            let _ = call_tauri::<_, ()>(
                                                "execute_autofill",
                                                &ExecuteAutofillArgs {
                                                    username: u,
                                                    secret: s,
                                                },
                                            )
                                            .await;
                                        });
                                    }
                                >
                                    <IconKey />
                                    <span>"Fill"</span>
                                </button>
                            }.into_view()
                        } else {
                            view! { <div style="display:none;"></div> }.into_view()
                        }
                    }}

                    <button
                        class="shield-btn"
                        on:click=move |_| {
                            let was_open = shield_open.get_untracked();
                            if !was_open {
                                if let Some(doc) = web_sys::window().and_then(|w| w.document())
                                {
                                    if let Ok(Some(btn)) = doc.query_selector(".shield-btn") {
                                        let rect = btn.get_bounding_client_rect();
                                        let center_x = rect.left() + rect.width() / 2.0;
                                        let bottom_y = rect.bottom();
                                        let mut x = center_x - 160.0;
                                        let y = bottom_y + 6.0;
                                        if let Some(w) = web_sys::window() {
                                            let vw = w
                                                .inner_width()
                                                .ok()
                                                .and_then(|v| v.as_f64())
                                                .unwrap_or(1400.0);
                                            if x + 320.0 > vw - 8.0 {
                                                x = vw - 328.0;
                                            }
                                            if x < 8.0 {
                                                x = 8.0;
                                            }
                                        }
                                        set_shield_flyout_pos.set((x, y));
                                    }
                                }
                            }
                            set_shield_open.set(!was_open);
                        }
                    >
                        <IconShield />
                        <span>
                            {move || {
                                let cur_id = active_tab_id.get();
                                let n = tabs.get()
                                    .into_iter()
                                    .find(|t| t.id == cur_id)
                                    .map(|x| x.blocked_count)
                                    .unwrap_or(0);
                                if n > 99 {
                                    "99+".to_string()
                                } else {
                                    n.to_string()
                                }
                            }}
                        </span>
                    </button>

                    <button
                        class="icon-btn"
                        title=move || tr_nav("Đánh dấu trang này", "Bookmark this page")
                        on:click=move |_| {
                            let cur_url = omnibox_text.get();
                            if !cur_url.is_empty() && !is_internal_url(&cur_url) {
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>(
                                        "save_bookmark",
                                        &SaveBookmarkArgs {
                                            url: cur_url.clone(),
                                            title: cur_url,
                                        },
                                    )
                                    .await;
                                    if let Ok(bm) = call_tauri::<_, Vec<BookmarkRecord>>(
                                        "fetch_bookmarks",
                                        &EmptyArgs {},
                                    )
                                    .await
                                    {
                                        set_bookmarks.set(bm);
                                    }
                                });
                            }
                        }
                    >
                        <IconBookmark />
                    </button>

                    {move || {
                        let focused = omnibox_focused.get();
                        let suggs = omnibox_suggestions.get();
                        if !focused || suggs.is_empty() {
                            return view! { <div style="display:none;"></div> }.into_view();
                        }
                        let sel = omnibox_sel_index.get();
                        let items = suggs
                            .into_iter()
                            .enumerate()
                            .map(|(i, s)| {
                                let url = s.url.clone();
                                let url_disp = url.clone();
                                let title = if s.title.is_empty() {
                                    url.clone()
                                } else {
                                    s.title.clone()
                                };
                                let is_bookmark = s.kind == "bookmark";
                                let kind_label =
                                    if is_bookmark { "BOOKMARK" } else { "HISTORY" };
                                let is_selected = sel >= 0 && (sel as usize) == i;

                                let url_for_click = url.clone();
                                view! {
                                    <div
                                        class=format!(
                                            "omnibox-suggest-item {} {}",
                                            if is_bookmark { "bookmark" } else { "history" },
                                            if is_selected { "selected" } else { "" },
                                        )
                                        on:mousedown=move |ev| {
                                            ev.prevent_default();
                                            ev.stop_propagation();
                                        }
                                        on:click=move |ev| {
                                            ev.prevent_default();
                                            ev.stop_propagation();
                                            let u = url_for_click.clone();
                                            set_omnibox_suggestions.set(Vec::new());
                                            set_omnibox_sel_index.set(-1);
                                            set_omnibox_focused.set(false);
                                            navigate_for_newtab(u, true);
                                        }
                                    >
                                        <span class="suggest-icon">
                                            {if is_bookmark { "★" } else { "◷" }}
                                        </span>
                                        <div class="suggest-text">
                                            <div class="suggest-title">{title}</div>
                                            <div class="suggest-url">{url_disp}</div>
                                        </div>
                                        <span class="suggest-kind-badge">{kind_label}</span>
                                    </div>
                                }
                            })
                            .collect_view();
                        view! {
                            <div class="omnibox-suggest">{items}</div>
                        }.into_view()
                    }}
                </div>

                <button
                    class="icon-btn"
                    on:click=move |_| set_find_open.set(!find_open.get())
                    title=move || tr_nav("Tìm trong trang (Ctrl+F)", "Find in page (Ctrl+F)")
                >
                    <IconFind />
                </button>
                <button
                    class="icon-btn"
                    on:click=move |_| navigate_for_menu("vibird://extensions".into(), true)
                    title=move || tr_nav("Tiện ích", "Extensions")
                >
                    <IconExtension />
                </button>
                <button
                    class="icon-btn"
                    on:click=move |_| navigate_for_menu("vibird://downloads".into(), true)
                    title=move || tr_nav("Tải về (Ctrl+J)", "Downloads (Ctrl+J)")
                >
                    <IconDownload />
                </button>
                <button
                    class="icon-btn"
                    on:click=move |_| navigate_for_menu("vibird://passwords".into(), true)
                    title=move || tr_nav("Két mật khẩu", "Password Vault")
                >
                    <IconKey />
                </button>
                <button
                    class="icon-btn"
                    on:click=move |_| set_menu_open.set(!menu_open.get())
                    title=move || tr_nav("Cài đặt & Menu", "Settings & Menu")
                >
                    <IconMenu />
                </button>
            </div>

            <div class="bookmarks-strip">
                {move || bookmarks.get().into_iter().map(|b| {
                    let u = b.url.clone();
                    view! {
                        <span
                            class="bookmark-item"
                            on:click=move |_| navigate_for_bookmarks(u.clone(), true)
                        >
                            {b.title}
                        </span>
                    }
                }).collect_view()}
            </div>

            {move || if find_open.get() {
                view! {
                    <div class="find-bar">
                        <input
                            type="text"
                            placeholder=move || tr_find("Tìm trong trang...", "Find in page...")
                            prop:value=find_query
                            on:input=move |ev| {
                                let q = event_target_value(&ev);
                                set_find_query.set(q.clone());
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>(
                                        "find_in_page",
                                        &FindArgs {
                                            query: q,
                                            forward: true,
                                            reset: true,
                                        },
                                    )
                                    .await;
                                });
                            }
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    ev.prevent_default();
                                    let q = find_query.get_untracked();
                                    if !q.is_empty() {
                                        let forward = !ev.shift_key();
                                        spawn_local(async move {
                                            let _ = call_tauri::<_, ()>(
                                                "find_in_page",
                                                &FindArgs {
                                                    query: q,
                                                    forward,
                                                    reset: false,
                                                },
                                            )
                                            .await;
                                        });
                                    }
                                } else if ev.key() == "Escape" {
                                    ev.prevent_default();
                                    set_find_open.set(false);
                                    set_find_query.set(String::new());
                                    spawn_local(async move {
                                        let _ = call_tauri::<_, ()>(
                                            "find_in_page",
                                            &FindArgs {
                                                query: String::new(),
                                                forward: true,
                                                reset: true,
                                            },
                                        )
                                        .await;
                                    });
                                }
                            }
                        />
                        <span class="find-counter">
                            {move || {
                                if !find_supported.get() {
                                    return String::new();
                                }
                                let c = find_count.get();
                                if c == 0 {
                                    "0/0".to_string()
                                } else {
                                    format!("{}/{}", find_current.get(), c)
                                }
                            }}
                        </span>
                        <button
                            class="icon-btn"
                            title=move || tr_find("Trước", "Previous")
                            on:click=move |_| {
                                let q = find_query.get_untracked();
                                if !q.is_empty() {
                                    spawn_local(async move {
                                        let _ = call_tauri::<_, ()>(
                                            "find_in_page",
                                            &FindArgs {
                                                query: q,
                                                forward: false,
                                                reset: false,
                                            },
                                        )
                                        .await;
                                    });
                                }
                            }
                        >
                            <IconBack />
                        </button>
                        <button
                            class="icon-btn"
                            title=move || tr_find("Sau", "Next")
                            on:click=move |_| {
                                let q = find_query.get_untracked();
                                if !q.is_empty() {
                                    spawn_local(async move {
                                        let _ = call_tauri::<_, ()>(
                                            "find_in_page",
                                            &FindArgs {
                                                query: q,
                                                forward: true,
                                                reset: false,
                                            },
                                        )
                                        .await;
                                    });
                                }
                            }
                        >
                            <IconForward />
                        </button>
                        <button
                            class="icon-btn"
                            title=move || tr_find("Đóng", "Close")
                            on:click=move |_| {
                                set_find_open.set(false);
                                set_find_query.set(String::new());
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>(
                                        "find_in_page",
                                        &FindArgs {
                                            query: String::new(),
                                            forward: true,
                                            reset: true,
                                        },
                                    )
                                    .await;
                                });
                            }
                        >
                            <IconClose />
                        </button>
                    </div>
                }
            } else {
                view! { <div style="display:none;"></div> }
            }}

            {move || if shield_open.get() {
                let cur_url = omnibox_text.get();
                let domain = extract_domain(&cur_url);
                let dom_for_toggle = domain.clone();
                let is_site_enabled = current_site_shield.get();
                let badge_style = if is_site_enabled {
                    "color:var(--accent-shield)"
                } else {
                    "color:var(--danger)"
                };
                let (pos_x, pos_y) = shield_flyout_pos.get();
                let flyout_style = format!("position:fixed;left:{}px;top:{}px;", pos_x, pos_y);
                let badge_text = if is_site_enabled {
                    tr_shield("ĐANG BẬT", "ENABLED")
                } else {
                    tr_shield("ĐANG TẮT", "DISABLED")
                };
                view! {
                    <div class="shield-flyout" style=flyout_style>
                        <div class="flyout-head">
                            <strong>"Vibird Shield"</strong>
                            <span class="shield-status-badge" style=badge_style>
                                {badge_text}
                            </span>
                        </div>

                        <div class="shield-site-box">
                            <div class="site-name">{domain}</div>
                            <label class="switch">
                                <input
                                    type="checkbox"
                                    prop:checked=is_site_enabled
                                    on:change=move |ev| {
                                        let checked = event_target_checked(&ev);
                                        set_current_site_shield.set(checked);
                                        let d = dom_for_toggle.clone();
                                        spawn_local(async move {
                                            let _ = call_tauri::<_, ()>(
                                                "toggle_site_shield",
                                                &ToggleSiteShieldArgs {
                                                    domain: d,
                                                    enabled: checked,
                                                },
                                            )
                                            .await;
                                        });
                                    }
                                />
                                <span class="slider round"></span>
                            </label>
                        </div>

                        <div class="flyout-stat">
                            <div class="num">
                                {move || {
                                    let cur = active_tab_id.get();
                                    tabs.get()
                                        .into_iter()
                                        .find(|t| t.id == cur)
                                        .map(|x| x.blocked_count)
                                        .unwrap_or(0)
                                }}
                            </div>
                            <span style="font-size:11px; color:var(--text-secondary)">
                                {move || tr_shield(
                                    "Quảng cáo & theo dõi đã chặn",
                                    "Ads & trackers blocked",
                                )}
                            </span>
                        </div>

                        <button
                            class="btn-action"
                            style="margin-top:12px; width:100%; background:var(--bg-tertiary); font-size:11px;"
                            on:click=move |_| {
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>(
                                        "clear_site_data",
                                        &EmptyArgs {},
                                    )
                                    .await;
                                });
                            }
                        >
                            {move || tr_shield("Xoá cookie & cache", "Clear cookies & cache")}
                        </button>
                        <button
                            class="btn-action"
                            style="margin-top:6px; width:100%; background:var(--bg-tertiary); font-size:11px;"
                            on:click=move |_| {
                                set_shield_open.set(false);
                                navigate_for_find("vibird://shields".into(), true);
                            }
                        >
                            {move || tr_shield("Quản lý ngoại lệ...", "Manage exceptions...")}
                        </button>
                    </div>
                }
            } else {
                view! { <div style="display:none;"></div> }
            }}

            {move || if menu_open.get() {
                view! {
                    <div class="hamburger-menu">
                        <div class="menu-item" on:click=move |_| create_new_tab_for_menu(false)>
                            {move || tr_menu("Tab mới (Ctrl+T)", "New Tab (Ctrl+T)")}
                        </div>
                        <div class="menu-item" on:click=move |_| create_new_tab_for_menu(true)>
                            {move || tr_menu(
                                "Tab ẩn danh (Ctrl+Shift+T)",
                                "Incognito Tab (Ctrl+Shift+T)",
                            )}
                        </div>
                        <div class="menu-divider"></div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate_for_history("vibird://history".into(), true)
                        >
                            {move || tr_menu("Lịch sử (Ctrl+H)", "History (Ctrl+H)")}
                        </div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate_for_history("vibird://downloads".into(), true)
                        >
                            {move || tr_menu("Tải về (Ctrl+J)", "Downloads (Ctrl+J)")}
                        </div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate_for_history("vibird://bookmarks".into(), true)
                        >
                            {move || tr_menu("Dấu trang", "Bookmarks")}
                        </div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate_for_history("vibird://extensions".into(), true)
                        >
                            {move || tr_menu("Tiện ích", "Extensions")}
                        </div>
                        <div class="menu-divider"></div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate_for_history("vibird://passwords".into(), true)
                        >
                            {move || tr_menu("Két mật khẩu", "Password Vault")}
                        </div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate_for_history("vibird://shields".into(), true)
                        >
                            {move || tr_menu("Quản lý Shield", "Shield Manager")}
                        </div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate_for_history("vibird://settings".into(), true)
                        >
                            {move || tr_menu("Cài đặt", "Settings")}
                        </div>
                        <div class="menu-divider"></div>
                        <div
                            class="menu-item"
                            on:click=move |_| {
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>("toggle_devtools", &EmptyArgs {})
                                        .await;
                                });
                            }
                        >
                            {move || tr_menu("DevTools (F12)", "DevTools (F12)")}
                        </div>
                    </div>
                }
            } else {
                view! { <div style="display:none;"></div> }
            }}

            {move || active_download.get().map(|prog| {
                let task_id = prog.id.clone();
                let status_lower = prog.status.to_lowercase();
                let is_paused = status_lower.contains("pause");
                let is_finished = status_lower.contains("completed")
                    || status_lower.contains("failed")
                    || status_lower.contains("cancel");
                let task_id_pause = task_id.clone();
                let task_id_resume = task_id.clone();
                let task_id_cancel = task_id.clone();
                view! {
                    <div class="download-shelf">
                        <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:6px;">
                            <span style="font-weight:600; font-size:12px; max-width:150px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">
                                {prog.filename.clone()}
                            </span>
                            <div style="display:flex; align-items:center; gap:6px;">
                                <span style="font-size:11px; color:var(--accent); font-family:var(--font-mono);">
                                    {if is_paused {
                                        "Paused".to_string()
                                    } else {
                                        format!("{} Mbps", prog.speed_mbps)
                                    }}
                                </span>

                                {if !is_finished {
                                    view! {
                                        <button
                                            class="icon-btn"
                                            style="padding:2px 6px; font-size:11px;"
                                            title=if is_paused { "Resume" } else { "Pause" }
                                            on:click={
                                                let p_id = task_id_pause.clone();
                                                let r_id = task_id_resume.clone();
                                                move |_| {
                                                    let tid = if is_paused {
                                                        r_id.clone()
                                                    } else {
                                                        p_id.clone()
                                                    };
                                                    let cmd = if is_paused {
                                                        "resume_download"
                                                    } else {
                                                        "pause_download"
                                                    };
                                                    spawn_local(async move {
                                                        let _ = call_tauri::<_, ()>(
                                                            cmd,
                                                            &DownloadTaskArgs { task_id: tid },
                                                        )
                                                        .await;
                                                    });
                                                }
                                            }
                                        >
                                            {if is_paused { "▶" } else { "⏸" }}
                                        </button>
                                    }.into_view()
                                } else {
                                    view! { <div style="display:none;"></div> }.into_view()
                                }}

                                {if !is_finished {
                                    view! {
                                        <button
                                            class="icon-btn"
                                            style="padding:2px 6px; font-size:11px; color:var(--danger);"
                                            title="Cancel"
                                            on:click={
                                                let c_id = task_id_cancel.clone();
                                                move |_| {
                                                    let tid = c_id.clone();
                                                    spawn_local(async move {
                                                        let _ = call_tauri::<_, ()>(
                                                            "cancel_download",
                                                            &DownloadTaskArgs { task_id: tid },
                                                        )
                                                        .await;
                                                    });
                                                }
                                            }
                                        >
                                            "✕"
                                        </button>
                                    }.into_view()
                                } else {
                                    view! { <div style="display:none;"></div> }.into_view()
                                }}

                                <button
                                    class="icon-btn"
                                    style="padding:2px; font-size:10px;"
                                    on:click=move |_| set_active_download.set(None)
                                >
                                    <IconClose />
                                </button>
                            </div>
                        </div>
                        <div class="shelf-progress-bar">
                            <div
                                class="shelf-progress-fill"
                                style=format!("width: {}%", prog.progress_percent)
                            ></div>
                        </div>
                        <div style="display:flex; justify-content:space-between; margin-top:4px; font-size:10px; color:var(--text-secondary);">
                            <span>{format!("{:.1}%", prog.progress_percent)}</span>
                            <span>{prog.status.clone()}</span>
                        </div>
                    </div>
                }
            })}

            <main class="viewport-body">
                {move || {
                    let cur_id = active_tab_id.get();
                    let is_loading = tabs.get()
                        .into_iter()
                        .find(|t| t.id == cur_id)
                        .map(|t| t.is_loading)
                        .unwrap_or(false);
                    if is_loading {
                        view! { <div class="page-loading-bar"></div> }.into_view()
                    } else {
                        view! { <div style="display:none;"></div> }.into_view()
                    }
                }}

                {move || {
                    let cur_id = active_tab_id.get();
                    let current_tab = tabs.get().into_iter().find(|t| t.id == cur_id);
                    let mode = current_tab
                        .as_ref()
                        .map(|t| t.page_mode.clone())
                        .unwrap_or(PageMode::NewTab);

                    match mode {
                        PageMode::NewTab => {
                            view! { <NewTabView on_navigate=move |u| navigate_for_newtab(u, true) /> }
                                .into_view()
                        }
                        PageMode::Settings => {
                            view! { <SettingsView config=config set_config=set_config /> }
                                .into_view()
                        }
                        PageMode::History => {
                            view! { <HistoryView on_navigate=move |u| navigate_for_history(u, true) /> }
                                .into_view()
                        }
                        PageMode::Bookmarks => {
                            view! { <BookmarksView on_navigate=move |u| navigate_for_bookmarks(u, true) /> }
                                .into_view()
                        }
                        PageMode::Downloads => view! { <DownloadsView /> }.into_view(),
                        PageMode::Extensions => view! { <ExtensionsView /> }.into_view(),
                        PageMode::Vault => view! { <VaultView /> }.into_view(),
                        PageMode::Shields => view! { <ShieldsView /> }.into_view(),
                        PageMode::Web => view! {
                            <div style="width:100%; height:100%; background:transparent;"></div>
                        }
                        .into_view(),
                    }
                }}
            </main>
        </div>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App/> })
}
