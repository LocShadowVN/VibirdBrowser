use crate::tauri_ipc::call_tauri;
use leptos::*;
use serde::Serialize;
use shared::ShieldStats;

#[derive(Serialize)]
struct EmptyArgs {}

#[component]
pub fn NewTabView<F>(on_navigate: F) -> impl IntoView
where
    F: Fn(String) + 'static + Copy,
{
    let (search_query, set_search_query) = create_signal(String::new());
    let (stats, set_stats) = create_signal(ShieldStats {
        total_blocked: 0,
        trackers_blocked: 0,
        bandwidth_saved_mb: 0.0,
        time_saved_secs: 0.0,
    });

    let (time_display, set_time_display) = create_signal(String::from("--:--"));

    let update_clock = move || {
        let d = js_sys::Date::new_0();
        set_time_display.set(format!("{:02}:{:02}", d.get_hours(), d.get_minutes()));
    };
    update_clock();
    set_interval(update_clock, std::time::Duration::from_secs(30));

    spawn_local(async move {
        if let Ok(st) = call_tauri::<_, ShieldStats>("get_shield_stats", &EmptyArgs {}).await {
            set_stats.set(st);
        }
    });

    view! {
        <div class="internal-view">
            <div class="newtab-container">
                <div class="newtab-clock">{time_display}</div>
                <p style="color:var(--text-secondary); margin-bottom:24px;">
                    "Protected by Vibird Shield Core & Brave Filtering Engine"
                </p>

                <div class="newtab-search-box">
                    <input
                        type="text"
                        placeholder="Search web with Brave Search or enter URL..."
                        prop:value=search_query
                        on:input=move |ev| set_search_query.set(event_target_value(&ev))
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            if ev.key() == "Enter" {
                                let q = search_query.get();
                                if !q.trim().is_empty() {
                                    on_navigate(q);
                                }
                            }
                        }
                    />
                    <button
                        class="btn-action"
                        on:click=move |_| {
                            let q = search_query.get();
                            if !q.trim().is_empty() {
                                on_navigate(q);
                            }
                        }
                    >
                        "Search"
                    </button>
                </div>

                <div class="privacy-dash">
                    <div class="dash-box">
                        <div class="val">{move || stats.get().total_blocked}</div>
                        <div class="lbl">"Trackers & Ads Blocked"</div>
                    </div>
                    <div class="dash-box">
                        <div class="val">
                            {move || format!("{:.1} MB", stats.get().bandwidth_saved_mb)}
                        </div>
                        <div class="lbl">"Bandwidth Saved"</div>
                    </div>
                    <div class="dash-box">
                        <div class="val">
                            {move || format!("{:.1} s", stats.get().time_saved_secs)}
                        </div>
                        <div class="lbl">"Estimated Time Saved"</div>
                    </div>
                </div>

                <div class="speed-dial-grid">
                    <div class="dial-item" on:click=move |_| on_navigate("https://search.brave.com".into())>
                        <strong>"Brave Search"</strong>
                        <span style="font-size:11px; color:var(--text-secondary);">
                            "Private Search"
                        </span>
                    </div>
                    <div class="dial-item" on:click=move |_| on_navigate("https://duckduckgo.com".into())>
                        <strong>"DuckDuckGo"</strong>
                        <span style="font-size:11px; color:var(--text-secondary);">
                            "Search Engine"
                        </span>
                    </div>
                    <div class="dial-item" on:click=move |_| on_navigate("https://github.com".into())>
                        <strong>"GitHub"</strong>
                        <span style="font-size:11px; color:var(--text-secondary);">
                            "Code Hosting"
                        </span>
                    </div>
                    <div class="dial-item" on:click=move |_| on_navigate("https://rust-lang.org".into())>
                        <strong>"Rust Lang"</strong>
                        <span style="font-size:11px; color:var(--text-secondary);">
                            "Systems Language"
                        </span>
                    </div>
                    <div class="dial-item" on:click=move |_| on_navigate("https://wikipedia.org".into())>
                        <strong>"Wikipedia"</strong>
                        <span style="font-size:11px; color:var(--text-secondary);">
                            "Open Encyclopedia"
                        </span>
                    </div>
                    <div class="dial-item" on:click=move |_| on_navigate("https://news.ycombinator.com".into())>
                        <strong>"Hacker News"</strong>
                        <span style="font-size:11px; color:var(--text-secondary);">
                            "Tech Aggregator"
                        </span>
                    </div>
                </div>
            </div>
        </div>
    }
}
