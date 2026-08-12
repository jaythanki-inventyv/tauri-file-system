//! Saving a picked file back to the device.
//! - Browser: `<a download>` click → the browser's own Downloads.
//! - Tauri:   "Save As" dialog (SAF on Android) → bytes streamed to disk
//!            in 1 MB chunks via plugin-fs, with % progress.

use crate::model::{format_size, Picked};
use crate::tauri_api::{close_resource, is_tauri, tauri_invoke_timeout};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

/// Yield ≥1 frame so Leptos can flush signal updates to the DOM before we
/// continue. Uses `setTimeout(16)` — more reliable than `requestAnimationFrame`
/// on Android WebView (rAF can stall when the paint cycle isn't running).
async fn yield_frame() {
    sleep_ms(16).await;
}

/// Sleep for `ms` milliseconds. Used to *guarantee* the progress bar is
/// visible for a perceptible moment — a single 16ms frame yield is only a
/// scheduling hint, not a paint guarantee: on a busy device (native IPC,
/// GC) the browser can skip straight past it, so a tiny/instant save would
/// sometimes flash 0%→100% with no visible bar at all, and sometimes not.
/// Holding for ~150ms makes this consistent every time instead of flaky.
async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = leptos::prelude::window()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    let _ = JsFuture::from(promise).await;
}

/// Web branch of saving: a programmatic `<a download>` click — the browser
/// writes the blob to its Downloads folder.
fn trigger_browser_download(url: &str, name: &str) {
    let Ok(a) = document().create_element("a") else { return };
    let _ = a.set_attribute("href", url);
    let _ = a.set_attribute("download", name);
    if let Some(el) = a.dyn_ref::<web_sys::HtmlElement>() {
        el.click();
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// Minimal ZIP writer (no compression, "stored" entries) — enough for
/// bundling the picked files into one archive.
struct ZipWriter {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

impl ZipWriter {
    fn new() -> Self {
        Self { out: Vec::new(), central: Vec::new(), count: 0 }
    }

    fn u16(buf: &mut Vec<u8>, v: u16) { buf.extend_from_slice(&v.to_le_bytes()); }
    fn u32(buf: &mut Vec<u8>, v: u32) { buf.extend_from_slice(&v.to_le_bytes()); }

    fn add(&mut self, name: &str, data: &[u8]) {
        let offset = self.out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name_bytes = name.as_bytes();

        // Local file header
        Self::u32(&mut self.out, 0x0403_4B50);
        Self::u16(&mut self.out, 20);                    // version needed
        Self::u16(&mut self.out, 0x0800);                // flags: UTF-8 names
        Self::u16(&mut self.out, 0);                     // method: stored
        Self::u32(&mut self.out, 0);                     // mod time/date
        Self::u32(&mut self.out, crc);
        Self::u32(&mut self.out, size);                  // compressed
        Self::u32(&mut self.out, size);                  // uncompressed
        Self::u16(&mut self.out, name_bytes.len() as u16);
        Self::u16(&mut self.out, 0);                     // extra len
        self.out.extend_from_slice(name_bytes);
        self.out.extend_from_slice(data);

        // Central directory entry
        Self::u32(&mut self.central, 0x0201_4B50);
        Self::u16(&mut self.central, 20);                // version made by
        Self::u16(&mut self.central, 20);                // version needed
        Self::u16(&mut self.central, 0x0800);
        Self::u16(&mut self.central, 0);
        Self::u32(&mut self.central, 0);
        Self::u32(&mut self.central, crc);
        Self::u32(&mut self.central, size);
        Self::u32(&mut self.central, size);
        Self::u16(&mut self.central, name_bytes.len() as u16);
        Self::u16(&mut self.central, 0);                 // extra
        Self::u16(&mut self.central, 0);                 // comment
        Self::u16(&mut self.central, 0);                 // disk
        Self::u16(&mut self.central, 0);                 // internal attrs
        Self::u32(&mut self.central, 0);                 // external attrs
        Self::u32(&mut self.central, offset);
        self.central.extend_from_slice(name_bytes);
        self.count += 1;
    }

    fn finish(mut self) -> Vec<u8> {
        let cd_offset = self.out.len() as u32;
        let cd_size = self.central.len() as u32;
        self.out.extend_from_slice(&self.central);
        // End of central directory
        Self::u32(&mut self.out, 0x0605_4B50);
        Self::u16(&mut self.out, 0);
        Self::u16(&mut self.out, 0);
        Self::u16(&mut self.out, self.count);
        Self::u16(&mut self.out, self.count);
        Self::u32(&mut self.out, cd_size);
        Self::u32(&mut self.out, cd_offset);
        Self::u16(&mut self.out, 0);
        self.out
    }
}

/// Web "Download All": Safari allows only ONE download per user click, so
/// bundle everything into a single ZIP and download that.
pub async fn download_all_zip(
    files: &[Picked],
    set_status: WriteSignal<String>,
    set_progress: WriteSignal<Option<u8>>,
) -> Result<String, String> {
    let total = files.len();
    let mut zip = ZipWriter::new();
    let mut used_names: Vec<String> = Vec::new();
    set_progress.set(Some(0));
    for (i, f) in files.iter().enumerate() {
        set_status.set(format!("🗜️ Zipping {}/{}: \"{}\"…", i + 1, total, f.name));
        let buf = JsFuture::from(f.file.array_buffer())
            .await
            .map_err(|_| format!("Could not read \"{}\"", f.name))?;
        let bytes = js_sys::Uint8Array::new(&buf).to_vec();
        // Duplicate names would overwrite each other inside the zip.
        let mut name = f.name.clone();
        if used_names.contains(&name) {
            name = format!("{}-{}", i + 1, name);
        }
        used_names.push(name.clone());
        zip.add(&name, &bytes);
        set_progress.set(Some((((i + 1) * 100) / total.max(1)) as u8));
    }
    let zip_bytes = zip.finish();
    let zip_len = zip_bytes.len() as f64;

    let arr = js_sys::Uint8Array::from(zip_bytes.as_slice());
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("application/zip");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(
        &js_sys::Array::of1(&arr),
        &opts,
    )
    .map_err(|e| format!("Could not build zip blob: {e:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap_or_default();
    trigger_browser_download(&url, "files.zip");
    let _ = web_sys::Url::revoke_object_url(&url);

    Ok(format!(
        "⬇️ files.zip ({}, {} file(s)) sent to browser downloads",
        format_size(zip_len),
        total
    ))
}

/// Save one picked file; returns the status message to show.
/// Reports progress (0–100 %) through `set_progress` while writing.
/// `progress_label` is an optional "N/total" prefix for Download-All so the
/// count stays visible across both phases below (e.g. "2/6 — ").
pub async fn save_picked(
    f: &Picked,
    progress_label: &str,
    set_status: WriteSignal<String>,
    set_progress: WriteSignal<Option<u8>>,
) -> Result<String, String> {
    if !is_tauri() {
        trigger_browser_download(&f.url, &f.name);
        return Ok(format!("⬇️ \"{}\" sent to browser downloads", f.name));
    }

    // Distinct from the "writing" message below — this phase is waiting on
    // YOU (pick a folder + tap Save), not doing any work. Saying "Saving…"
    // here made a normal wait for user input look like a frozen app.
    set_status.set(format!("{progress_label}👉 Choose where to save \"{}\"…", f.name));

    // 1. "Save As" — user picks the location. Android shows the SAF
    //    "Save to…" screen and returns a content:// URI.
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&options, &"defaultPath".into(), &f.name.clone().into());

    // ⚠️ KNOWN ISSUE — Android SAF dedup produces "10mb.pdf(1)" instead of
    // "10mb(1).pdf". Root cause (confirmed against tauri-plugin-dialog 2.7.1
    // Kotlin source, DialogPlugin.kt's saveFileDialog): `intent.type` is
    // hardcoded to "*/*" regardless of filters, so the SAF document provider
    // has no MIME type to infer an extension from — it treats the whole
    // EXTRA_TITLE ("10mb.pdf") as an opaque stem and appends "(1)" after all
    // of it. TRIED sending only the stem ("10mb", no extension) instead —
    // confirmed on-device this backfires: with no extension in the name and
    // intent.type="*/*", Android does NOT auto-append ".pdf" even on a
    // *non*-duplicate save, so the file loses its extension entirely. That
    // trade is worse than the cosmetic naming issue, so reverted — send the
    // full name. This is not fixable from our side; see the fix paths below.
    //
    // Correct fix paths (need the plugin's own Kotlin code changed):
    // • PR to tauri-plugin-dialog to set intent.type from parsedTypes[0]
    // • Use tauri-plugin-android-fs (Way 2) which has its own SAF wrapper
    if let Some(ext) = f.name.rsplit('.').next().filter(|e| *e != f.name) {
        let filter = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&filter, &"name".into(), &"File".into());
        let _ = js_sys::Reflect::set(&filter, &"extensions".into(), &js_sys::Array::of1(&ext.into()));
        let _ = js_sys::Reflect::set(&options, &"filters".into(), &js_sys::Array::of1(&filter));
    }
    let args = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&args, &"options".into(), &options);
    // Bounded wait: long enough for a real person to pick a folder, short
    // enough that a lost native callback fails fast and moves on instead of
    // leaving the whole Download All queue looking frozen for a full minute.
    let chosen = tauri_invoke_timeout("plugin:dialog|save", args.into(), 10_000)
        .await
        .map_err(|e| format!("Save dialog failed: {e}"))?;
    // Desktop/iOS resolve this as a plain path string. Android's plugin
    // (confirmed against the installed 2.7.1 Kotlin source) resolves it as
    // `{ file: "content://..." }` instead — calling `.as_string()` on that
    // object silently returns None, which this code used to treat as
    // "user cancelled" even on a real, confirmed save. Handle both shapes.
    let path = if let Some(s) = chosen.as_string() {
        Some(s)
    } else {
        js_sys::Reflect::get(&chosen, &"file".into())
            .ok()
            .and_then(|v| v.as_string())
    };
    let Some(path) = path else {
        // `null`/`undefined` (no `file` field either) = user cancelled.
        return Ok(format!("✖️ Save cancelled for \"{}\"", f.name));
    };

    // Location chosen — now actual work starts, so the message changes to
    // reflect that (this is the phase the progress bar below belongs to).
    set_status.set(format!("{progress_label}💾 Writing \"{}\"…", f.name));

    let total = write_file_bytes(&path, &f.file, set_progress).await?;
    Ok(format!("✅ Saved ({}): {path}", format_size(total as f64)))
}

/// Streams `file`'s bytes to `path` in 1 MB chunks via plugin-fs open/write.
/// One giant write_file IPC payload fails on Android for big files (0-byte
/// result); chunking fixes that AND gives real progress numbers. `path` can
/// be a plain filesystem path or a content:// URI (plugin-fs understands
/// both) — this is what lets Way 2's Android save path reuse this same
/// proven writer after resolving its own file URI a different way.
/// Returns the total byte count written.
pub async fn write_file_bytes(
    path: &str,
    file: &web_sys::File,
    set_progress: WriteSignal<Option<u8>>,
) -> Result<u32, String> {
    let buf = JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| "Could not read the file's bytes".to_string())?;
    let data = js_sys::Uint8Array::new(&buf);
    let total = data.length();

    let open_opts = js_sys::Object::new();
    for flag in ["write", "create", "truncate"] {
        let _ = js_sys::Reflect::set(&open_opts, &(*flag).into(), &true.into());
    }
    let open_args = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&open_args, &"path".into(), &path.into());
    let _ = js_sys::Reflect::set(&open_args, &"options".into(), &open_opts);
    let rid = tauri_invoke_timeout("plugin:fs|open", open_args.into(), 10_000)
        .await
        .map_err(|e| format!("Could not open target file: {e}"))?;

    const CHUNK: u32 = 1_048_576; // 1 MB
    let mut offset: u32 = 0;
    set_progress.set(Some(0));
    sleep_ms(150).await; // guarantee the 0 % bar actually paints before writing starts
    while offset < total {
        let end = (offset + CHUNK).min(total);
        let write_args = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&write_args, &"rid".into(), &rid);
        let _ = js_sys::Reflect::set(&write_args, &"data".into(), &data.subarray(offset, end));
        if let Err(e) = tauri_invoke_timeout("plugin:fs|write", write_args.into(), 15_000).await {
            let _ = close_resource(&rid).await;
            return Err(format!("Write failed at {offset}/{total} bytes: {e}"));
        }
        offset = end;
        set_progress.set(Some(((offset as f64 / total.max(1) as f64) * 100.0) as u8));
        // Yield so Leptos flushes the progress signal to the DOM before the
        // next IPC write — without this the bar jumps straight to 100 %
        // (or never renders at all on Android WebView).
        yield_frame().await;
    }
    // Guarantee the 100 % state is visible too — the caller clears the bar
    // immediately after this function returns, so without a real hold here
    // a fast/tiny save could go 0%→100%→cleared inside one uncommitted paint.
    sleep_ms(150).await;

    // Close the resource handle. On Android, SAF content:// descriptors
    // sometimes return an error from the close IPC even though all bytes
    // were written successfully (the OS already flushed & closed on its
    // side). We log the error but still report success so the UI shows
    // ✅ and the caller clears the progress bar.
    if let Err(e) = close_resource(&rid).await {
        leptos::logging::log!("[save] close_resource warning (file was written): {e:?}");
    }

    Ok(total)
}
