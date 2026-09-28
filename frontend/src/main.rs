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
    history::HistoryView, newtab::NewTabView, settings::SettingsView, vault::VaultView,
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
        _ => None,
    }
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
    let (shield_open, set_shield_open) = create_signal(false);
    let (menu_open, set_menu_open) = create_signal(false);
    let (find_open, set_find_open) = create_signal(false);
    let (find_query, set_find_query) = create_signal(String::new());
    let (find_count, set_find_count) = create_signal(0i32);
    let (find_current, set_find_current) = create_signal(0i32);
    let (find_supported, set_find_supported) = create_signal(true);

    let (bookmarks, set_bookmarks) = create_signal(Vec::<BookmarkRecord>::new());
    let (current_site_shield, set_current_site_shield) = create_signal(true);
    let (available_credentials, set_available_credentials) =
        create_signal(Vec::<SiteCredential>::new());
    let (active_download, set_active_download) =
        create_signal(Option::<DownloadProgressPayload>::None);

    let (config, set_config) = create_signal(AppConfig::default());

    let (pending_new_tab, set_pending_new_tab) = create_signal(None::<(String, bool)>);
    let (last_new_tab_at, set_last_new_tab_at) = create_signal(0.0f64);

    // === Bootstrap: config + bookmarks + session restore ===
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

    create_effect(move |_| {
        let open = shield_open.get() || menu_open.get();
        spawn_local(async move {
            let _ = call_tauri::<_, ()>("expand_ui_for_menu", &MenuExpandArgs { expanded: open })
                .await;
        });
    });

    // === Session auto-save ===
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
            if let Ok(payload_val) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
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
            if let Ok(payload_val) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
            {
                if let Ok(p) = serde_wasm_bindgen::from_value::<ShieldBlockedPayload>(payload_val) {
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

    // === Listener: find-result ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload_val) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
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

    // === Listener: open-new-tab từ content webview ===
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload_val) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
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

        let all_ids: Vec<String> = tabs.get_untracked().iter().map(|t| t.id.clone()).collect();
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
            if let Ok(payload_val) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
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
                    let _ =
                        w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 30_000);
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
            if let Ok(payload_val) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload"))
            {
                if let Ok(p) = serde_wasm_bindgen::from_value::<ContextMenuPayload>(payload_val) {
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
                                        let _ =
                                            call_tauri::<_, ()>("webview_go_back", &EmptyArgs {})
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
                                &ZoomArgs {
                                    delta: 0.1,
                                    reset: false,
                                },
                            )
                            .await;
                        });
                    }
                    "-" => {
                        e.prevent_default();
                        spawn_local(async move {
                            let _ = call_tauri::<_, ()>(
                                "webview_zoom_by",
                                &ZoomArgs {
                                    delta: -0.1,
                                    reset: false,
                                },
                            )
                            .await;
                        });
                    }
                    "0" => {
                        e.prevent_default();
                        spawn_local(async move {
                            let _ = call_tauri::<_, ()>(
                                "webview_zoom_by",
                                &ZoomArgs {
                                    delta: 0.0,
                                    reset: true,
                                },
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
                                if idx == 0 {
                                    list.len() - 1
                                } else {
                                    idx - 1
                                }
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
            }
        }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);

        let _ = window
            .add_event_listener_with_callback("keydown", key_closure.as_ref().unchecked_ref());
        key_closure.forget();
    }

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
                                    }
                                        .into_view()
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
                                                if let Some(t) = t_list.iter_mut().find(|x| x.id == id_s) {
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
                                    }
                                        .into_view()
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
                                            let del_index = t_list.iter().position(|x| x.id == del_id);
                                            t_list.retain(|x| x.id != del_id);
                                            if active_tab_id.get() == del_id {
                                                let next_idx = del_index.unwrap_or(1).saturating_sub(1);
                                                let next_tab = &t_list[next_idx];
                                                set_active_tab_id.set(next_tab.id.clone());
                                                set_omnibox_text.set(if is_internal_url(&next_tab.url) {
                                                    String::new()
                                                } else {
                                                    next_tab.url.clone()
                                                });
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
                </div>

                <button
                    class="icon-btn"
                    title="New Tab (Ctrl+T)"
                    on:click=move |_| create_new_tab(false)
                >
                    <IconPlus />
                </button>
            </header>

            <div class="nav-bar">
                <button
                    class="icon-btn"
                    title="Go Back"
                    on:click=move |_| {
                        let cur = active_tab_id.get();
                        let mut list = tabs.get();
                        if let Some(tab) = list.iter_mut().find(|t| t.id == cur) {
                            if tab.page_mode == PageMode::Web {
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>("webview_go_back", &EmptyArgs {})
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
                    title="Go Forward"
                    on:click=move |_| {
                        let cur = active_tab_id.get();
                        let mut list = tabs.get();
                        if let Some(tab) = list.iter_mut().find(|t| t.id == cur) {
                            if tab.page_mode == PageMode::Web {
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>("webview_go_forward", &EmptyArgs {})
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
                    title="Reload (Ctrl+R)"
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
                        placeholder="Search web or enter address (Ctrl+L to focus)"
                        prop:value=omnibox_text
                        on:focus=move |_| set_omnibox_focused.set(true)
                        on:blur=move |_| set_omnibox_focused.set(false)
                        on:input=move |ev| set_omnibox_text.set(event_target_value(&ev))
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            if ev.key() == "Enter" {
                                navigate(omnibox_text.get(), true);
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
                                    <span>"Autofill"</span>
                                </button>
                            }
                                .into_view()
                        } else {
                            view! { <div style="display:none;"></div> }.into_view()
                        }
                    }}

                    <button
                        class="shield-btn"
                        on:click=move |_| set_shield_open.set(!shield_open.get())
                    >
                        <IconShield />
                        <span>
                            {move || {
                                let cur_id = active_tab_id.get();
                                tabs.get()
                                    .into_iter()
                                    .find(|t| t.id == cur_id)
                                    .map(|x| x.blocked_count)
                                    .unwrap_or(0)
                            }}
                        </span>
                    </button>

                    <button
                        class="icon-btn"
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
                </div>

                <button
                    class="icon-btn"
                    on:click=move |_| set_find_open.set(!find_open.get())
                    title="Find in Page (Ctrl+F)"
                >
                    <span style="font-weight:700; font-size:12px;">"F"</span>
                </button>
                <button
                    class="icon-btn"
                    on:click=move |_| navigate("vibird://extensions".into(), true)
                    title="Extensions"
                >
                    <IconExtension />
                </button>
                <button
                    class="icon-btn"
                    on:click=move |_| navigate("vibird://downloads".into(), true)
                    title="Downloads (Ctrl+J)"
                >
                    <IconDownload />
                </button>
                <button
                    class="icon-btn"
                    on:click=move |_| navigate("vibird://passwords".into(), true)
                    title="Password Vault"
                >
                    <IconKey />
                </button>
                <button
                    class="icon-btn"
                    on:click=move |_| set_menu_open.set(!menu_open.get())
                    title="Settings & Menu"
                >
                    <IconMenu />
                </button>
            </div>

            <div class="bookmarks-strip">
                {move || bookmarks.get().into_iter().map(|b| {
                    let u = b.url.clone();
                    view! {
                        <span class="bookmark-item" on:click=move |_| navigate(u.clone(), true)>
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
                            placeholder="Find in page..."
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
                            title="Previous"
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
                            "P"
                        </button>
                        <button
                            class="icon-btn"
                            title="Next"
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
                            "N"
                        </button>
                        <button
                            class="icon-btn"
                            title="Close"
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
                view! {
                    <div class="shield-flyout">
                        <div class="flyout-head">
                            <strong>"Vibird Shield Core"</strong>
                            <span class="shield-status-badge" style=badge_style>
                                {if is_site_enabled { "Shields UP" } else { "Shields DOWN" }}
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
                                "Trackers, Ads & Cookies Neutralized"
                            </span>
                        </div>

                        <button
                            class="btn-action"
                            style="margin-top:12px; width:100%; background:var(--bg-tertiary); font-size:11px;"
                            on:click=move |_| {
                                spawn_local(async move {
                                    let _ = call_tauri::<_, ()>("clear_site_data", &EmptyArgs {})
                                        .await;
                                });
                            }
                        >
                            "Clear Cookies & Cache"
                        </button>
                    </div>
                }
            } else {
                view! { <div style="display:none;"></div> }
            }}

            {move || if menu_open.get() {
                view! {
                    <div class="hamburger-menu">
                        <div class="menu-item" on:click=move |_| create_new_tab(false)>
                            "New Tab (Ctrl+T)"
                        </div>
                        <div class="menu-item" on:click=move |_| create_new_tab(true)>
                            "New Incognito Tab (Ctrl+Shift+T)"
                        </div>
                        <div class="menu-divider"></div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate("vibird://history".into(), true)
                        >
                            "History (Ctrl+H)"
                        </div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate("vibird://downloads".into(), true)
                        >
                            "Downloads (Ctrl+J)"
                        </div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate("vibird://bookmarks".into(), true)
                        >
                            "Bookmarks"
                        </div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate("vibird://extensions".into(), true)
                        >
                            "Extensions"
                        </div>
                        <div class="menu-divider"></div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate("vibird://passwords".into(), true)
                        >
                            "Passwords (Vault)"
                        </div>
                        <div
                            class="menu-item"
                            on:click=move |_| navigate("vibird://settings".into(), true)
                        >
                            "Settings"
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
                            "Developer Tools (F12)"
                        </div>
                    </div>
                }
            } else {
                view! { <div style="display:none;"></div> }
            }}

            {move || active_download.get().map(|prog| {
                view! {
                    <div class="download-shelf">
                        <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:6px;">
                            <span style="font-weight:600; font-size:12px; max-width:170px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;">
                                {prog.filename}
                            </span>
                            <div style="display:flex; align-items:center; gap:8px;">
                                <span style="font-size:11px; color:var(--accent); font-family:var(--font-mono);">
                                    {format!("{} Mbps ({} threads)", prog.speed_mbps, prog.threads)}
                                </span>
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
                            <span>{prog.status}</span>
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
                            view! { <NewTabView on_navigate=move |u| navigate(u, true) /> }
                                .into_view()
                        }
                        PageMode::Settings => {
                            view! { <SettingsView config=config set_config=set_config /> }
                                .into_view()
                        }
                        PageMode::History => {
                            view! { <HistoryView on_navigate=move |u| navigate(u, true) /> }
                                .into_view()
                        }
                        PageMode::Bookmarks => {
                            view! { <BookmarksView on_navigate=move |u| navigate(u, true) /> }
                                .into_view()
                        }
                        PageMode::Downloads => view! { <DownloadsView /> }.into_view(),
                        PageMode::Extensions => view! { <ExtensionsView /> }.into_view(),
                        PageMode::Vault => view! { <VaultView /> }.into_view(),
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
