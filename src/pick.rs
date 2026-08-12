//! Picking files:
//! - Browser picks are handled in app.rs's `on_change` (plain `<input>`).
//! - `pick_images_native` is the Tauri-only image pick: real native picker
//!   (PHPicker on iOS — Photo Library only), returns paths, bytes read here.

use crate::model::{guess_image_mime, Picked};
use crate::tauri_api::tauri_invoke_timeout;
use leptos::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::Url;

/// Yield control briefly so the browser/webview can paint between loop steps,
/// keeping status updates visible while processing many files.
///
/// Uses `setTimeout(16)` instead of `requestAnimationFrame` because on Android
/// WebView, rAF can hang indefinitely when the paint cycle isn't running
/// (e.g. while the JS file-processing loop is active after a programmatic
/// DataTransfer inject), causing the entire pick loop to freeze after the
/// first item. setTimeout always fires regardless of paint state.
pub async fn next_frame() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 16);
    });
    let _ = JsFuture::from(promise).await;
}

/// Native image pick (Tauri only): plugin-dialog with image-only filters.
/// On iOS this opens PHPicker (Photo Library directly — no
/// "Take Photo / Choose Files" sheet); on Android the system image picker.
/// Returns the picked files with bytes read via plugin-fs.
pub async fn pick_images_native(
    set_status: WriteSignal<String>,
) -> Result<Vec<Picked>, String> {
    let filter = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&filter, &"name".into(), &"Images".into());
    let exts = js_sys::Array::new();
    for e in ["png", "jpg", "jpeg", "gif", "webp", "heic"] {
        exts.push(&(*e).into());
    }
    let _ = js_sys::Reflect::set(&filter, &"extensions".into(), &exts);
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&options, &"multiple".into(), &true.into());
    let _ = js_sys::Reflect::set(&options, &"filters".into(), &js_sys::Array::of1(&filter));
    let args = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&args, &"options".into(), &options);

    // 60s: waiting on the user, not a fixed operation — but still bounded,
    // so a native callback that never fires doesn't hang the UI forever.
    let v = tauri_invoke_timeout("plugin:dialog|open", args.into(), 60_000)
        .await
        .map_err(|e| format!("Picker failed: {e}"))?;
    if v.is_null() || v.is_undefined() {
        return Ok(Vec::new()); // cancelled
    }
    // Desktop/iOS resolve this as a plain array of paths. Android's plugin
    // (confirmed against the installed 2.7.1 Kotlin source) resolves it as
    // `{ files: [...] }` instead — `Array::from()` on that plain object
    // silently yields an empty array, so every Android native pick used to
    // come back with zero files with no error at all. Handle both shapes.
    let files_array = if js_sys::Array::is_array(&v) {
        js_sys::Array::from(&v)
    } else {
        js_sys::Reflect::get(&v, &"files".into())
            .map(|f| js_sys::Array::from(&f))
            .unwrap_or_else(|_| js_sys::Array::new())
    };
    let paths: Vec<String> = files_array
        .iter()
        .filter_map(|p| p.as_string())
        .collect();

    let total = paths.len();
    let mut picked = Vec::with_capacity(total);
    for (i, path) in paths.into_iter().enumerate() {
        let name = path
            .rsplit('/')
            .next()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("image-{}", i + 1));
        set_status.set(format!("📥 Loading {}/{}: \"{name}\"…", i + 1, total));

        // Read the bytes through plugin-fs (understands file:// and content://).
        let read_args = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&read_args, &"path".into(), &path.clone().into());
        let bytes = tauri_invoke_timeout("plugin:fs|read_file", read_args.into(), 15_000)
            .await
            .map_err(|e| format!("Could not read \"{name}\": {e}"))?;
        let arr = js_sys::Uint8Array::new(&bytes);
        let size = arr.length() as f64;

        // Wrap the bytes in a File so preview + save reuse the same paths.
        let mime = guess_image_mime(&name);
        let opts = web_sys::FilePropertyBag::new();
        opts.set_type(mime);
        let file = web_sys::File::new_with_u8_array_sequence_and_options(
            &js_sys::Array::of1(&arr),
            &name,
            &opts,
        )
        .map_err(|e| format!("Could not wrap \"{name}\": {e:?}"))?;
        let url = Url::create_object_url_with_blob(&file).unwrap_or_default();

        picked.push(Picked {
            name,
            size,
            mime: mime.to_string(),
            url,
            is_image: true,
            rel_path: path, // native pick DOES give a real path/URI — show it
            file,
        });
    }
    Ok(picked)
}
