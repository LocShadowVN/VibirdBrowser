use crate::tauri_ipc::call_tauri;
use leptos::*;
use serde::Serialize;

#[derive(Serialize)]
struct EmptyArgs {}

#[derive(Serialize)]
struct AddExceptionArgs {
    domain: String,
    enabled: bool,
}

#[derive(Serialize)]
struct RemoveExceptionArgs {
    domain: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug, PartialEq)]
pub struct ShieldExceptionFE {
    pub domain: String,
    pub enabled: bool,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug, Default, PartialEq)]
pub struct DetailedStatsFE {
    pub total_blocked: u64,
    pub trackers_blocked: u64,
    pub bandwidth_saved_mb: f64,
    pub time_saved_secs: f64,
    pub domain_rules: u64,
    pub substring_rules: u64,
    pub whitelist_rules: u64,
    pub site_exceptions: u64,
}

#[component]
pub fn ShieldsView() -> impl IntoView {
    let (exceptions, set_exceptions) = create_signal(Vec::<ShieldExceptionFE>::new());
    let (stats, set_stats) = create_signal(DetailedStatsFE::default());
    let (filter, set_filter) = create_signal(String::new());
    let (input_domain, set_input_domain) = create_signal(String::new());
    let (input_enabled, set_input_enabled) = create_signal(false);
    let (error_msg, set_error_msg) = create_signal(Option::<String>::None);
    let (loading, set_loading) = create_signal(false);

    let reload = move || {
        set_loading.set(true);
        spawn_local(async move {
            if let Ok(list) = call_tauri::<_, Vec<ShieldExceptionFE>>(
                "fetch_site_exceptions",
                &EmptyArgs {},
            )
            .await
            {
                set_exceptions.set(list);
            }
            if let Ok(s) = call_tauri::<_, DetailedStatsFE>(
                "get_shield_stats_detailed",
                &EmptyArgs {},
            )
            .await
            {
                set_stats.set(s);
            }
            set_loading.set(false);
        });
    };
    reload();

    let do_add = move || {
        let d = input_domain.get().trim().to_string();
        if d.is_empty() {
            set_error_msg.set(Some("Domain is required".into()));
            return;
        }
        if d.contains(' ') || d.contains('/') {
            set_error_msg.set(Some("Invalid domain format".into()));
            return;
        }
        let enabled = input_enabled.get();
        set_error_msg.set(None);
        spawn_local(async move {
            match call_tauri::<_, ()>(
                "add_shield_exception",
                &AddExceptionArgs {
                    domain: d.clone(),
                    enabled,
                },
            )
            .await
            {
                Ok(_) => {
                    set_input_domain.set(String::new());
                    if let Ok(list) = call_tauri::<_, Vec<ShieldExceptionFE>>(
                        "fetch_site_exceptions",
                        &EmptyArgs {},
                    )
                    .await
                    {
                        set_exceptions.set(list);
                    }
                    if let Ok(s) = call_tauri::<_, DetailedStatsFE>(
                        "get_shield_stats_detailed",
                        &EmptyArgs {},
                    )
                    .await
                    {
                        set_stats.set(s);
                    }
                }
                Err(e) => set_error_msg.set(Some(e)),
            }
        });
    };

    let do_remove = move |domain: String| {
        spawn_local(async move {
            let _ = call_tauri::<_, ()>(
                "remove_shield_exception",
                &RemoveExceptionArgs { domain: domain.clone() },
            )
            .await;
            if let Ok(list) = call_tauri::<_, Vec<ShieldExceptionFE>>(
                "fetch_site_exceptions",
                &EmptyArgs {},
            )
            .await
            {
                set_exceptions.set(list);
            }
            if let Ok(s) = call_tauri::<_, DetailedStatsFE>(
                "get_shield_stats_detailed",
                &EmptyArgs {},
            )
            .await
            {
                set_stats.set(s);
            }
        });
    };

    let do_toggle = move |ex: ShieldExceptionFE| {
        let new_enabled = !ex.enabled;
        spawn_local(async move {
            let _ = call_tauri::<_, ()>(
                "add_shield_exception",
                &AddExceptionArgs {
                    domain: ex.domain.clone(),
                    enabled: new_enabled,
                },
            )
            .await;
            if let Ok(list) = call_tauri::<_, Vec<ShieldExceptionFE>>(
                "fetch_site_exceptions",
                &EmptyArgs {},
            )
            .await
            {
                set_exceptions.set(list);
            }
        });
    };

    view! {
        <div class="internal-view">
            <div class="panel-card" style="max-width: 960px;">
                <h2>"Vibird Shield — Per-Site Exceptions"</h2>
                <p style="font-size:13px; color:var(--text-secondary); margin-bottom:20px;">
                    "Domain listed here will override the global shield. \"Disabled\" = bypass adblock. \"Enabled\" = force shield ON even if global setting is Off."
                </p>

                {move || error_msg.get().map(|e| view! {
                    <div style="background:rgba(239,68,68,0.15); border:1px solid var(--danger); padding:8px 12px; border-radius:6px; color:var(--danger); font-size:12.5px; margin-bottom:16px;">
                        {e}
                    </div>
                })}

                <div class="shield-stats-grid">
                    <div class="dash-box">
                        <div class="val">{move || stats.get().total_blocked}</div>
                        <div class="lbl">"Total Blocked"</div>
                    </div>
                    <div class="dash-box">
                        <div class="val">{move || stats.get().domain_rules}</div>
                        <div class="lbl">"Domain Rules"</div>
                    </div>
                    <div class="dash-box">
                        <div class="val">{move || stats.get().substring_rules}</div>
                        <div class="lbl">"Substring Rules"</div>
                    </div>
                    <div class="dash-box">
                        <div class="val">{move || stats.get().whitelist_rules}</div>
                        <div class="lbl">"Whitelist Rules"</div>
                    </div>
                </div>

                <div class="shield-add-row">
                    <input
                        type="text"
                        class="shield-input"
                        placeholder="example.com"
                        prop:value=input_domain
                        on:input=move |ev| set_input_domain.set(event_target_value(&ev))
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            if ev.key() == "Enter" {
                                do_add();
                            }
                        }
                    />
                    <label class="shield-toggle-wrap" title="Disable shield on this domain">
                        <input
                            type="checkbox"
                            prop:checked=input_enabled
                            on:change=move |ev| set_input_enabled.set(event_target_checked(&ev))
                        />
                        <span>"Force ON"</span>
                    </label>
                    <button class="btn-action" on:click=move |_| do_add()>
                        "Add Exception"
                    </button>
                </div>

                <div style="margin-top:16px; display:flex; gap:8px; align-items:center;">
                    <input
                        type="text"
                        class="shield-input"
                        placeholder="Filter domains..."
                        prop:value=filter
                        on:input=move |ev| set_filter.set(event_target_value(&ev))
                    />
                    <button
                        class="btn-action"
                        style="background:var(--bg-tertiary); white-space:nowrap;"
                        on:click=move |_| reload()
                        disabled=move || loading.get()
                    >
                        {move || if loading.get() { "Loading..." } else { "Reload" }}
                    </button>
                </div>

                <table class="data-table" style="margin-top:16px;">
                    <thead>
                        <tr>
                            <th>"Domain"</th>
                            <th>"Status"</th>
                            <th style="width:180px;">"Action"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let f = filter.get().to_lowercase();
                            let list = exceptions.get();
                            let filtered: Vec<ShieldExceptionFE> = list
                                .into_iter()
                                .filter(|e| f.is_empty() || e.domain.contains(&f))
                                .collect();
                            if filtered.is_empty() {
                                return view! {
                                    <tr>
                                        <td colspan="3" style="text-align:center; color:var(--text-secondary); font-style:italic; padding:20px;">
                                            "No per-site exceptions. All domains use global shield setting."
                                        </td>
                                    </tr>
                                }.into_view();
                            }
                            filtered.into_iter().map(|ex| {
                                let ex_toggle = ex.clone();
                                let ex_remove = ex.clone();
                                let enabled = ex.enabled;
                                let status_style = if enabled {
                                    "color:var(--accent-shield); font-weight:600;"
                                } else {
                                    "color:var(--danger); font-weight:600;"
                                };
                                let status_label = if enabled { "Enabled" } else { "Disabled (bypass)" };
                                view! {
                                    <tr>
                                        <td><code style="font-size:12px;">{ex.domain}</code></td>
                                        <td>
                                            <span style=status_style>{status_label}</span>
                                        </td>
                                        <td style="display:flex; gap:8px;">
                                            <button
                                                class="btn-action"
                                                style="padding:4px 10px; font-size:11px; background:var(--bg-tertiary);"
                                                on:click=move |_| do_toggle(ex_toggle.clone())
                                            >
                                                "Toggle"
                                            </button>
                                            <button
                                                class="icon-btn"
                                                style="color:var(--danger);"
                                                on:click=move |_| do_remove(ex_remove.domain.clone())
                                            >
                                                "Remove"
                                            </button>
                                        </td>
                                    </tr>
                                }
                            }).collect_view()
                        }}
                    </tbody>
                </table>

                <div style="margin-top:20px; font-size:11.5px; color:var(--text-secondary); line-height:1.6;">
                    <strong>"Note:"</strong>
                    " "
                    "Adding an exception here does not reload the current tab. Reload manually (Ctrl+R) to apply changes."
                </div>
            </div>
        </div>
    }
}
