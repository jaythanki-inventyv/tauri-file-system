//! The UI — and nothing else. Picking lives in pick.rs, saving in save.rs,
//! data types in model.rs, the native bridge in tauri_api.rs.

use crate::model::{format_size, Picked};
use crate::pick::{next_frame, pick_images_native};
use crate::save::{download_all_zip, save_picked};
use crate::tauri_api::is_tauri;
use crate::way2::Way2;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, Url};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Way1,
    Way2,
}

#[component]
pub fn App() -> impl IntoView {
    let (page, set_page) = signal(Page::Way1);

    view! {
        <div class="nav">
            <button
                class="nav-btn"
                class:active=move || page.get() == Page::Way1
                on:click=move |_| set_page.set(Page::Way1)
            >"Way 1 — <input type=file>"</button>
            <button
                class="nav-btn"
                class:active=move || page.get() == Page::Way2
                on:click=move |_| set_page.set(Page::Way2)
            >"Way 2 — native plugins"</button>
        </div>
        {move || match page.get() {
            Page::Way1 => view! { <Way1 /> }.into_any(),
            Page::Way2 => view! { <Way2 /> }.into_any(),
        }}
    }
}

#[component]
fn Way1() -> impl IntoView {
    let (files, set_files) = signal(Vec::<Picked>::new());
    // The file currently open in the fullscreen preview overlay, if any.
    let (preview, set_preview) = signal(Option::<Picked>::None);
    // What's happening right now, shown on screen so slow/failed picks aren't silent.
    let (status, set_status) = signal(String::new());
    // Save progress: Some(0–100) while a save is writing, None otherwise.
    let (progress, set_progress) = signal(Option::<u8>::None);
    let on_pick_start = move |_| {
        set_status.set("⏳ Waiting for your selection… (cloud files can take a moment to download)".into());
    };

    // `webkitdirectory` is a non-standard attribute Leptos' typed builder
    // doesn't know, so it is set on the element after mount via a NodeRef.
    let dir_input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move || {
        if let Some(input) = dir_input.get() {
            let _ = input.set_attribute("webkitdirectory", "");
        }
    });

    // Way 1: every picker is a plain <input type="file">; the browser/webview
    // opens its own native chooser. One handler serves all three inputs.
    let on_change = move |ev: leptos::ev::Event| {
        let input: HtmlInputElement = ev.target().unwrap().dyn_into().unwrap();
        let Some(list) = input.files() else { return };
        let files_vec: Vec<web_sys::File> =
            (0..list.length()).filter_map(|i| list.get(i)).collect();
        // Allow re-picking the same files.
        input.set_value("");

        // Android sometimes fires `change` with an empty list (cancelled pick,
        // or a cloud file that wasn't downloaded in time) — keep the previous
        // selection instead of wiping it.
        if files_vec.is_empty() {
            set_status.set(
                "⚠️ No files received — the pick was cancelled or the file isn't ready yet. Please try again.".into(),
            );
            return;
        }

        // Process the list one file per frame so the status line stays live
        // when many files are picked at once. (No progress bar here — local
        // picks are near-instant; the bar is only used while saving.)
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

    // Save one file (💾 button). All the real work is in save.rs.
    let do_save = move |f: Picked| {
        spawn_local(async move {
            // save_picked itself sets the phase-appropriate status
            // ("choose a location…" then "writing…") as it progresses.
            match save_picked(&f, "", set_status, set_progress).await {
                Ok(msg) => set_status.set(msg),
                Err(e) => set_status.set(format!("⚠️ {e}")),
            }
            set_progress.set(None);
        });
    };

    view! {
        <h1>"Image & Files Picker — Way 1 (<input type=file>)"</h1>

        <div class="pickers">
            <label
                class="picker-btn"
                on:click=move |ev: leptos::ev::MouseEvent| {
                    if is_tauri() {
                        // Native path: PHPicker on iOS (Photo Library only —
                        // no Take Photo / Choose Files sheet), system image
                        // picker on Android. Stop the <input> from opening.
                        ev.prevent_default();
                        spawn_local(async move {
                            set_status.set("⏳ Waiting for your selection…".into());
                            match pick_images_native(set_status).await {
                                Ok(picked) if picked.is_empty() =>
                                    set_status.set("✖️ Pick cancelled.".into()),
                                Ok(picked) => {
                                    set_status.set(format!("✅ {} file(s) loaded", picked.len()));
                                    set_files.set(picked);
                                }
                                Err(e) => set_status.set(format!("⚠️ {e}")),
                            }
                        });
                    } else {
                        on_pick_start(ev);
                    }
                }
            >
                // "🖼️ Pick Images"
                // <input
                //     type="file"
                //     multiple=true
                //     accept="image/*"
                //     on:change=on_change
                // />

                    "🖼️ Pick Images"
                <input type="file" 
                multiple={true} 
                accept="image/*,video/*,application/pdf" 
                capture="environment" 
                on:change={on_change} />
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
                <input
                    type="file"
                    node_ref=dir_input
                    on:change=on_change
                />
            </label>
        </div>

        <p class="status">
            {move || status.get()}
            {move || progress.get().map(|p| format!(" {p}%"))}
        </p>
        // Thin progress bar, visible only while a pick/save is running.
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
                            if !is_tauri() {
                                // Safari allows only one download per click —
                                // bundle everything into a single zip instead.
                                match download_all_zip(&list, set_status, set_progress).await {
                                    Ok(msg) => set_status.set(msg),
                                    Err(e) => set_status.set(format!("⚠️ {e}")),
                                }
                                set_progress.set(None);
                                return;
                            }
                            // Tauri: one at a time — each file gets its own
                            // "Save As", so the dialogs must not overlap.
                            let total = list.len();
                            let mut ok_count = 0usize;
                            let mut failed_names = Vec::new();
                            for (i, f) in list.iter().enumerate() {
                                let label = format!("[{}/{total}] ", i + 1);
                                match save_picked(f, &label, set_status, set_progress).await {
                                    Ok(_) => ok_count += 1,
                                    Err(e) => {
                                        failed_names.push(f.name.clone());
                                        leptos::logging::log!("[download all] \"{}\" failed: {e}", f.name);
                                        // Show the failure for THIS file immediately —
                                        // no pause, straight on to the next one right after.
                                        set_status.set(format!("{label}⚠️ \"{}\" failed — moving on", f.name));
                                    }
                                }
                                set_progress.set(None);
                            }
                            // One combined summary at the end — otherwise the
                            // last file's own message is all that's left on
                            // screen, which reads as if the rest never ran.
                            set_status.set(if failed_names.is_empty() {
                                format!("✅ All {total} file(s) saved")
                            } else {
                                format!(
                                    "⚠️ {ok_count}/{total} saved — failed: {}",
                                    failed_names.join(", ")
                                )
                            });
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
                                            // Don't let the card's click open the preview.
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

        // Fullscreen preview overlay for the clicked file.
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
