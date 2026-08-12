//! Way 2 — same features as the main page, but on Android the save path
//! goes through `tauri-plugin-android-fs` instead of `tauri-plugin-dialog`.
//!
//! Why: `tauri-plugin-dialog`'s `saveFileDialog` opens a NEW native "Save As"
//! screen (one `startActivityForResult` round trip) for EVERY file — and
//! that round trip intermittently never calls back (see SETUP.md), leaving
//! a file stuck/failed. Here the user instead picks ONE destination folder
//! (one round trip, total, for the whole batch), then every file is created
//! inside it via plain async calls — nothing left to race on for file 2..N.
//! The actual byte-writing still goes through save.rs's proven chunked
//! `plugin:fs|open`/`plugin:fs|write` writer — only *how the target path is
//! obtained* changes.

use crate::model::{format_size, Picked};
use crate::pick::next_frame;
use crate::save::{download_all_zip, save_picked, write_file_bytes};
use crate::tauri_api::{is_android, is_tauri, tauri_invoke_timeout};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlInputElement, Url};

/// Save every picked file. Web: same zip fallback as Way 1. iOS/desktop:
/// falls back to the existing per-file "Save As" (android-fs is Android-only).
/// Android: pick one folder, then create+write each file into it.
async fn save_all_way2(
    files: &[Picked],
    set_status: WriteSignal<String>,
    set_progress: WriteSignal<Option<u8>>,
) -> Result<String, String> {
    if !is_tauri() {
        return download_all_zip(files, set_status, set_progress).await;
    }
    if !is_android() {
        let total = files.len();
        let mut ok = 0usize;
        let mut failed = Vec::new();
        for (i, f) in files.iter().enumerate() {
            let label = format!("[{}/{total}] ", i + 1);
            match save_picked(f, &label, set_status, set_progress).await {
                Ok(_) => ok += 1,
                Err(e) => {
                    failed.push(f.name.clone());
                    leptos::logging::log!("[way2 save] \"{}\" failed: {e}", f.name);
                    set_status.set(format!("{label}⚠️ \"{}\" failed — moving on", f.name));
                }
            }
            set_progress.set(None);
        }
        return Ok(if failed.is_empty() {
            format!("✅ All {total} file(s) saved")
        } else {
            format!("⚠️ {ok}/{total} saved — failed: {}", failed.join(", "))
        });
    }

    // Android: one folder pick for the whole batch.
    set_status.set(format!("👉 Choose a folder to save {} file(s) into…", files.len()));
    let dir = tauri_invoke_timeout("android_pick_save_dir", JsValue::UNDEFINED, 60_000)
        .await
        .map_err(|e| format!("Folder picker failed: {e}"))?;
    if dir.is_null() || dir.is_undefined() {
        return Ok("✖️ Save cancelled (no folder chosen)".into());
    }

    let total = files.len();
    let mut ok = 0usize;
    let mut failed = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let label = format!("[{}/{total}] ", i + 1);
        set_status.set(format!("{label}💾 Creating \"{}\"…", f.name));

        let create_args = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&create_args, &"dir".into(), &dir);
        let _ = js_sys::Reflect::set(&create_args, &"name".into(), &f.name.clone().into());
        let outcome: Result<(), String> = async {
            let path_val =
                tauri_invoke_timeout("android_create_file_path", create_args.into(), 15_000)
                    .await
                    .map_err(|e| format!("Could not create file: {e}"))?;
            let path = path_val
                .as_string()
                .ok_or_else(|| "No path returned".to_string())?;
            set_status.set(format!("{label}💾 Writing \"{}\"…", f.name));
            write_file_bytes(&path, &f.file, set_progress).await?;
            Ok(())
        }
        .await;

        match outcome {
            Ok(()) => ok += 1,
            Err(e) => {
                failed.push(f.name.clone());
                leptos::logging::log!("[way2 android save] \"{}\" failed: {e}", f.name);
                set_status.set(format!("{label}⚠️ \"{}\" failed — moving on", f.name));
            }
        }
        set_progress.set(None);
    }

    Ok(if failed.is_empty() {
        format!("✅ All {total} file(s) saved (1 folder pick, Android plugin)")
    } else {
        format!("⚠️ {ok}/{total} saved — failed: {}", failed.join(", "))
    })
}

#[component]
pub fn Way2() -> impl IntoView {
    let (files, set_files) = signal(Vec::<Picked>::new());
    let (preview, set_preview) = signal(Option::<Picked>::None);
    let (status, set_status) = signal(String::new());
    let (progress, set_progress) = signal(Option::<u8>::None);
    let on_pick_start = move |_| {
        set_status.set("⏳ Waiting for your selection… (cloud files can take a moment to download)".into());
    };

    let dir_input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move || {
        if let Some(input) = dir_input.get() {
            let _ = input.set_attribute("webkitdirectory", "");
        }
    });

    let on_change = move |ev: leptos::ev::Event| {
        let input: HtmlInputElement = ev.target().unwrap().dyn_into().unwrap();
        let Some(list) = input.files() else { return };
        let files_vec: Vec<web_sys::File> =
            (0..list.length()).filter_map(|i| list.get(i)).collect();
        input.set_value("");

        if files_vec.is_empty() {
            set_status.set(
                "⚠️ No files received — the pick was cancelled or the file isn't ready yet. Please try again.".into(),
            );
            return;
        }

        spawn_local(async move {
            let total = files_vec.len();
            let mut picked = Vec::with_capacity(total);
            for (i, f) in files_vec.into_iter().enumerate() {
                let name = f.name();
                let size = f.size();
                set_status.set(format!(
                    "📥 Loading {}/{}: \"{}\" ({})…",
                    i + 1, total, name, format_size(size)
                ));
                let mime = f.type_();
                let url = Url::create_object_url_with_blob(&f).unwrap_or_default();
                picked.push(Picked {
                    name,
                    size,
                    is_image: mime.starts_with("image/"),
                    mime,
                    url,
                    rel_path: js_sys::Reflect::get(&f, &"webkitRelativePath".into())
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default(),
                    file: f,
                });
                next_frame().await;
            }
            set_status.set(format!("✅ {} file(s) loaded", picked.len()));
            set_files.set(picked);
        });
    };

    let do_save = move |f: Picked| {
        spawn_local(async move {
            match save_all_way2(&[f], set_status, set_progress).await {
                Ok(msg) => set_status.set(msg),
                Err(e) => set_status.set(format!("⚠️ {e}")),
            }
            set_progress.set(None);
        });
    };

    view! {
        <h1>"Image & Files Picker — Way 2 (native plugins, Android via android-fs)"</h1>

        <div class="pickers">
            <label class="picker-btn" on:click=on_pick_start>
                "🖼️ Pick Images"
                <input type="file" multiple=true accept="image/*" on:change=on_change />
            </label>

            <label class="picker-btn" on:click=on_pick_start>
                "📄 Pick Documents"
                <input
                    type="file"
                    multiple=true
                    accept=".pdf,.doc,.docx,.txt,.xls,.xlsx,.ppt,.pptx"
                    on:change=on_change
                />
            </label>

            <label class="picker-btn" on:click=on_pick_start>
                "📁 Pick Directory"
                <input type="file" node_ref=dir_input on:change=on_change />
            </label>
        </div>

        <p class="status">
            {move || status.get()}
            {move || progress.get().map(|p| format!(" {p}%"))}
        </p>
        {move || progress.get().map(|p| view! {
            <div class="progress-wrap">
                <div class="progress-bar" style:width=format!("{p}%")></div>
            </div>
        })}

        <Show
            when=move || !files.get().is_empty()
            fallback=|| view! { <p class="empty">"No files selected yet."</p> }
        >
            <div class="toolbar">
                <p class="count">{move || format!("{} file(s) selected", files.get().len())}</p>
                <button
                    class="dl-all"
                    on:click=move |_| {
                        let list = files.get_untracked();
                        spawn_local(async move {
                            match save_all_way2(&list, set_status, set_progress).await {
                                Ok(msg) => set_status.set(msg),
                                Err(e) => set_status.set(format!("⚠️ {e}")),
                            }
                            set_progress.set(None);
                        });
                    }
                >"⬇️ Download All"</button>
            </div>
            <div class="grid">
                {move || {
                    files
                        .get()
                        .into_iter()
                        .map(|f| {
                            let icon = if f.mime.contains("pdf") { "📕" } else { "📄" };
                            let this = f.clone();
                            let for_save = f.clone();
                            view! {
                                <div class="card" on:click=move |_| set_preview.set(Some(this.clone()))>
                                    <button
                                        class="save-btn"
                                        title="Download this file"
                                        on:click=move |ev| {
                                            ev.stop_propagation();
                                            do_save(for_save.clone());
                                        }
                                    >"💾"</button>
                                    {if f.is_image {
                                        view! { <img src=f.url.clone() /> }.into_any()
                                    } else {
                                        view! { <div class="doc-icon">{icon}</div> }.into_any()
                                    }}
                                    <div class="info">
                                        <div class="name" title=f.name.clone()>{f.name.clone()}</div>
                                        <div class="meta">
                                            {format_size(f.size)}
                                            {if f.mime.is_empty() { String::new() } else { format!(" · {}", f.mime) }}
                                        </div>
                                        <div class="meta path">
                                            "path: "
                                            {if f.rel_path.is_empty() {
                                                "❌ not available".to_string()
                                            } else {
                                                f.rel_path.clone()
                                            }}
                                        </div>
                                    </div>
                                </div>
                            }
                        })
                        .collect_view()
                }}
            </div>
        </Show>

        {move || {
            preview.get().map(|f| {
                view! {
                    <div class="overlay" on:click=move |_| set_preview.set(None)>
                        <div class="preview-box">
                            {
                                if f.is_image {
                                    view! { <img class="preview-media" src=f.url.clone() /> }.into_any()
                                } else if f.mime.contains("pdf") {
                                    view! { <iframe class="preview-media preview-doc" src=f.url.clone()></iframe> }.into_any()
                                } else {
                                    view! { <div class="preview-fallback">"📄 No preview available for this file type"</div> }.into_any()
                                }
                            }
                            <div class="preview-caption">
                                {f.name.clone()} " · " {format_size(f.size)}
                                <span class="preview-hint">" (click anywhere to close)"</span>
                            </div>
                        </div>
                    </div>
                }
            })
        }}
    }
}
