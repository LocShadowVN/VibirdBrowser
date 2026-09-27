use crate::tauri_ipc::call_tauri;
use leptos::*;
use serde::Serialize;
use shared::DecryptedVaultRecord;

#[derive(Serialize)]
struct EmptyArgs {}

#[derive(Serialize)]
struct MasterPassArgs {
    master_pass: String,
}

#[derive(Serialize)]
struct AddVaultArgs {
    master_pass: String,
    website: String,
    username: String,
    secret: String,
}

#[derive(Serialize)]
struct DeleteVaultArgs {
    id: i64,
}

#[derive(Serialize)]
struct GenPassArgs {
    length: usize,
}

#[component]
pub fn VaultView() -> impl IntoView {
    let (configured, set_configured) = create_signal(false);
    let (unlocked_pass, set_unlocked_pass) = create_signal(Option::<String>::None);
    let (creds, set_creds) = create_signal(Vec::<DecryptedVaultRecord>::new());
    let (error_msg, set_error_msg) = create_signal(Option::<String>::None);

    let (master_input, set_master_input) = create_signal(String::new());
    let (site_input, set_site_input) = create_signal(String::new());
    let (user_input, set_user_input) = create_signal(String::new());
    let (sec_input, set_sec_input) = create_signal(String::new());

    spawn_local(async move {
        if let Ok(c) = call_tauri::<_, bool>("vault_is_configured", &EmptyArgs {}).await {
            set_configured.set(c);
        }
    });

    let do_generate_random = move || {
        spawn_local(async move {
            if let Ok(pw) =
                call_tauri::<_, String>("generate_password", &GenPassArgs { length: 22 }).await
            {
                set_sec_input.set(pw);
            }
        });
    };

    let do_unlock = move || {
        let p = master_input.get();
        let p_clone = p.clone();
        set_error_msg.set(None);
        spawn_local(async move {
            let res: Result<Vec<DecryptedVaultRecord>, _> =
                call_tauri("vault_read_all", &MasterPassArgs { master_pass: p }).await;
            match res {
                Ok(list) => {
                    set_creds.set(list);
                    set_unlocked_pass.set(Some(p_clone));
                    set_master_input.set(String::new());
                }
                Err(err) => {
                    set_error_msg.set(Some(format!("Unlock error: {}", err)));
                }
            }
        });
    };

    let do_setup = move || {
        let p = master_input.get();
        if p.len() < 8 {
            set_error_msg.set(Some("Password must be at least 8 characters".into()));
            return;
        }
        spawn_local(async move {
            if call_tauri::<_, ()>("vault_setup", &MasterPassArgs { master_pass: p })
                .await
                .is_ok()
            {
                set_configured.set(true);
                set_error_msg.set(None);
                set_master_input.set(String::new());
            }
        });
    };

    let do_save_cred = move || {
        if let Some(pass) = unlocked_pass.get() {
            let site = site_input.get();
            let user = user_input.get();
            let sec = sec_input.get();

            if site.is_empty() || user.is_empty() || sec.is_empty() {
                set_error_msg.set(Some("All credential fields are required.".into()));
                return;
            }

            let pass_clone = pass.clone();
            spawn_local(async move {
                let _ = call_tauri::<_, ()>(
                    "vault_save_credential",
                    &AddVaultArgs {
                        master_pass: pass_clone.clone(),
                        website: site,
                        username: user,
                        secret: sec,
                    },
                )
                .await;

                set_site_input.set(String::new());
                set_user_input.set(String::new());
                set_sec_input.set(String::new());
                set_error_msg.set(None);

                if let Ok(list) = call_tauri::<_, Vec<DecryptedVaultRecord>>(
                    "vault_read_all",
                    &MasterPassArgs { master_pass: pass_clone },
                )
                .await
                {
                    set_creds.set(list);
                }
            });
        }
    };

    view! {
        <div class="internal-view">
            {move || if let Some(err) = error_msg.get() {
                view! {
                    <div style="max-width:600px; margin:0 auto 16px auto; background:rgba(239,68,68,0.15); border:1px solid var(--danger); padding:10px 16px; border-radius:6px; color:var(--danger); font-size:13px;">
                        {err}
                    </div>
                }
            } else {
                view! { <div style="display:none;"></div> }
            }}

            {move || if !configured.get() {
                view! {
                    <div class="panel-card" style="max-width:480px;">
                        <h2>"Initialize Vibird Master Vault"</h2>
                        <div class="grid-form">
                            <label>"Master Password (minimum 8 characters)"</label>
                            <input
                                type="password"
                                prop:value=master_input
                                on:input=move |ev| set_master_input.set(event_target_value(&ev))
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    if ev.key() == "Enter" {
                                        do_setup();
                                    }
                                }
                            />
                            <button class="btn-action" on:click=move |_| do_setup()>
                                "Set Master Password"
                            </button>
                        </div>
                    </div>
                }.into_view()
            } else if unlocked_pass.get().is_none() {
                view! {
                    <div class="panel-card" style="max-width:440px;">
                        <h2>"Unlock Vault"</h2>
                        <div class="grid-form">
                            <label>"Master Password"</label>
                            <input
                                type="password"
                                prop:value=master_input
                                on:input=move |ev| set_master_input.set(event_target_value(&ev))
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    if ev.key() == "Enter" {
                                        do_unlock();
                                    }
                                }
                            />
                            <button class="btn-action" on:click=move |_| do_unlock()>
                                "Unlock"
                            </button>
                        </div>
                    </div>
                }.into_view()
            } else {
                view! {
                    <div class="panel-card">
                        <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:16px;">
                            <h2>"Password Vault (Argon2id + AES-256-GCM)"</h2>
                            <button
                                class="btn-action"
                                style="background:var(--bg-tertiary)"
                                on:click=move |_| {
                                set_unlocked_pass.set(None);
                                set_creds.set(Vec::new());
                                }
                            >
                                "Lock Vault"
                            </button>
                        </div>

                        <div class="grid-form" style="margin-bottom:20px; background:var(--bg-primary); padding:16px; border-radius:6px;">
                            <input
                                type="text"
                                placeholder="Website"
                                prop:value=site_input
                                on:input=move |ev| set_site_input.set(event_target_value(&ev))
                            />
                            <input
                                type="text"
                                placeholder="Username / Email"
                                prop:value=user_input
                                on:input=move |ev| set_user_input.set(event_target_value(&ev))
                            />
                            <div style="display:flex; gap:10px;">
                                <input
                                    type="text"
                                    placeholder="Password"
                                    prop:value=sec_input
                                    on:input=move |ev| set_sec_input.set(event_target_value(&ev))
                                    style="flex:1;"
                                />
                                <button
                                    class="btn-action"
                                    style="background:var(--bg-tertiary)"
                                    on:click=move |_| do_generate_random()
                                >
                                    "Generate Strong"
                                </button>
                            </div>
                            <button class="btn-action" on:click=move |_| do_save_cred()>
                                "Save to Vault"
                            </button>
                        </div>

                        <table class="data-table">
                            <thead>
                                <tr>
                                    <th>"Website"</th>
                                    <th>"Username"</th>
                                    <th>"Secret"</th>
                                    <th>"Date"</th>
                                    <th>"Action"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {move || creds.get().into_iter().map(|c| {
                                    let id = c.id;
                                    view! {
                                        <tr>
                                            <td>{c.website}</td>
                                            <td>{c.username}</td>
                                            <td><code>{c.secret}</code></td>
                                            <td>{c.created_at}</td>
                                            <td>
                                                <button
                                                    class="icon-btn"
                                                    style="color:var(--danger)"
                                                    on:click=move |_| {
                                                        let p = unlocked_pass.get().unwrap_or_default();
                                                        spawn_local(async move {
                                                            let _ = call_tauri::<_, ()>(
                                                                "vault_delete",
                                                                &DeleteVaultArgs { id },
                                                            )
                                                            .await;
                                                            if let Ok(list) = call_tauri::<_, Vec<DecryptedVaultRecord>>(
                                                                "vault_read_all",
                                                                &MasterPassArgs { master_pass: p },
                                                            )
                                                            .await
                                                            {
                                                                set_creds.set(list);
                                                            }
                                                        });
                                                    }
                                                >
                                                    "Delete"
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_view()
            }}
        </div>
    }
}
