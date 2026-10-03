use crate::tauri_ipc::call_tauri;
use leptos::*;
use serde::Serialize;
use shared::HistoryRecord;

#[derive(Serialize)]
struct EmptyArgs {}

#[component]
pub fn HistoryView<F>(on_navigate: F) -> impl IntoView
where
    F: Fn(String) + 'static + Copy,
{
    let (history, set_history) = create_signal(Vec::<HistoryRecord>::new());
    let (filter, set_filter) = create_signal(String::new());

    let load_hist = move || {
        spawn_local(async move {
            if let Ok(res) =
                call_tauri::<_, Vec<HistoryRecord>>("fetch_history", &EmptyArgs {}).await
            {
                set_history.set(res);
            }
        });
    };
    load_hist();

    let clear_hist = move |_| {
        spawn_local(async move {
            let _ = call_tauri::<_, ()>("clear_history", &EmptyArgs {}).await;
            set_history.set(Vec::new());
        });
    };

    view! {
        <div class="internal-view">
            <div class="panel-card" style="max-width: 1000px;">
                <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:16px; gap:16px;">
                    <h2 style="margin:0;">"Browsing History"</h2>
                    <button
                        class="btn-action"
                        style="background:var(--danger); flex-shrink:0;"
                        on:click=clear_hist
                    >
                        "Clear History"
                    </button>
                </div>

                <input
                    type="text"
                    placeholder="Search history..."
                    prop:value=filter
                    on:input=move |ev| set_filter.set(event_target_value(&ev))
                    style="width: 100%; height: 38px; background: var(--bg-primary); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-primary); padding: 0 12px; outline: none; font-size: 13px; margin-bottom: 20px;"
                />

                <div class="history-table-wrap">
                    <table class="data-table">
                        <colgroup>
                            <col style="width: 42%;" />
                            <col style="width: 40%;" />
                            <col style="width: 18%;" />
                        </colgroup>
                        <thead>
                            <tr>
                                <th>"Title"</th>
                                <th>"URL"</th>
                                <th>"Time"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {move || {
                                let f = filter.get().to_lowercase();
                                let items: Vec<HistoryRecord> = history
                                    .get()
                                    .into_iter()
                                    .filter(|item| {
                                        f.is_empty()
                                            || item.title.to_lowercase().contains(&f)
                                            || item.url.to_lowercase().contains(&f)
                                    })
                                    .collect();

                                if items.is_empty() {
                                    return view! {
                                        <tr>
                                            <td colspan="3" style="text-align:center; color:var(--text-secondary); font-style:italic; padding:32px;">
                                                "No history records."
                                            </td>
                                        </tr>
                                    }
                                    .into_view();
                                }

                                items.into_iter().map(|item| {
                                    let u = item.url.clone();
                                    let title = if item.title.is_empty() {
                                        item.url.clone()
                                    } else {
                                        item.title.clone()
                                    };
                                    let time = item.timestamp.clone().unwrap_or_default();
                                    view! {
                                        <tr
                                            style="cursor: pointer;"
                                            on:click=move |_| on_navigate(u.clone())
                                        >
                                            <td>
                                                <div class="history-title">{title}</div>
                                            </td>
                                            <td>
                                                <div class="history-url">{item.url}</div>
                                            </td>
                                            <td>
                                                <div class="history-time">{time}</div>
                                            </td>
                                        </tr>
                                    }
                                })
                                .collect_view()
                            }}
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
    }
}
