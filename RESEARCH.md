# Research: Cross-Platform Photo / File Picker with One Codebase

**Goal:** Build a single-codebase app for **Web + Android + iOS** that supports:

- Photo selection (single & multiple)
- File selection
- Preview of selected media

**Chosen stack:** Leptos (Rust UI) + Tauri v2 (Android / iOS / Desktop shell), with the web build running as a pure browser app (WASM, no Tauri).

---

## 1. Key Insight: How Much Code Is Actually Shared?

100% identical code for *every* feature is not possible, but that does not mean writing three separate apps.

| Layer                   | Shared across Web / Android / iOS |
| ----------------------- | --------------------------------- |
| UI (views, components)  | ~100% same                        |
| Business logic (state, signals) | ~100% same                |
| Platform-specific layer (pickers, native APIs) | Small, isolated part |

Simple UI and state code is identical everywhere:

```rust
view! { <button>"Hello"</button> }

let (count, set_count) = signal(0);
```

The photo/file picker is different per platform because each platform exposes a different native API:

| Platform | Native API                          |
| -------- | ----------------------------------- |
| Web      | `<input type="file">` (Browser API) |
| Android  | Android Photo Picker API            |
| iOS      | PHPicker / Photos framework         |

**Professional pattern:** the whole app calls a single function, e.g. `pick_files()`, and only its internals branch per platform:

```
pick_files()
     ├── Web   → <input type="file">
     └── Tauri → @tauri-apps/plugin-dialog (native picker)
```

This keeps screens, buttons, and business logic 100% shared (~95%+ of total code), with platform checks confined to one small file instead of `if android / if web` scattered everywhere.

---

## 2. The Ways to Implement the Picker

### Way 1: `<input type="file">` Everywhere (Simplest)

A Tauri app is a webview internally, so the HTML file input can work on all targets — the browser opens its own picker, iOS uses WKWebView's native picker.

- ✅ One code path, no abstraction at all, least effort
- ⚠️ Returns `File` blobs/bytes, not filesystem paths — enough for preview and upload, not for direct filesystem access
- ⚠️ File chooser behavior in Tauri's Android webview (wry) needs verification — historically tricky ([wry issue #87](https://github.com/tauri-apps/wry/issues/87))

### Way 2: `pick_files()` Abstraction — Recommended ⭐

Web build falls back to `<input type="file">`; Tauri builds use the official [@tauri-apps/plugin-dialog](https://v2.tauri.app/plugin/dialog/).

```js
import { open } from "@tauri-apps/plugin-dialog";

const files = await open({
  multiple: true,
  filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }]
});
```

- ✅ Real native pickers on Android / iOS / Desktop; multiple selection and filters supported
- ✅ Official, maintained plugin
- ⚠️ Two code paths — requires a small platform layer
- ⚠️ Tauri plugins do **not** work in a plain browser (no Tauri runtime there), hence the web fallback
- ⚠️ Android **folder** picker is not implemented yet ([tauri issue #14587](https://github.com/tauri-apps/tauri/issues/14587)) — file/image picking works fine
- ℹ️ iOS `mode` option requires iOS 14+

### Way 3: `rfd` Crate (Rust-Only Path)

[rfd](https://github.com/PolyMeilex/rfd) is a pure-Rust file dialog library supporting desktop **and** WASM/browser via [`AsyncFileDialog`](https://docs.rs/rfd/latest/rfd/struct.AsyncFileDialog.html) — one Rust API for web + desktop.

- ✅ Truly identical Rust code for Web + Desktop, no JS interop
- ⚠️ On WASM only the async API is available
- ❌ No Android / iOS support — must be combined with Way 2 for mobile

### Way 4: Community Mobile Plugins (Native Gallery UX)

For a real gallery/photos UI instead of a file dialog:

| Plugin | Platform | Notes |
| ------ | -------- | ----- |
| [tauri-plugin-android-fs](https://github.com/aiueo13/tauri-plugin-android-fs) | Android | File/image picking, reasonably active |
| [file-picker-android](https://github.com/Berrysoft/file-picker-android) | Android | Simple file picker |
| `tauri-plugin-ios-photos` | iOS | Photos album/asset management |
| [tauri-plugin-camera](https://github.com/nanderstabel/tauri-plugin-camera) | Mobile | Take photo or pick from gallery |

- ✅ More "native app" feel than the dialog picker
- ⚠️ Community-maintained — verify quality and maintenance before depending on them

### Way 5: Custom Native Plugin (Maximum Control)

Use Tauri v2's [mobile plugin system](https://v2.tauri.app/develop/plugins/develop-mobile/) to wire up the [Android Photo Picker](https://developer.android.com/training/data-storage/shared/photo-picker) in Kotlin and PHPicker in Swift yourself.

- ✅ Perfect native UX, full control (selection limits, media types, no photo permission needed)
- ❌ Most work — requires writing both Kotlin and Swift

> There is also a web-only variant — `window.showOpenFilePicker()` (File System Access API) — but Safari support is limited, so production apps don't rely on it.

---

## 3. Browser APIs Relevant to File Picking

"Web Browser API" is not one single API — it is an umbrella term. The browser exposes many built-in **Web APIs** to JavaScript (full list on [MDN](https://developer.mozilla.org/en-US/docs/Web/API)), just like Android exposes the Photo Picker API and iOS exposes PHPicker.

For file/photo picking, three matter:

1. **`<input type="file">`** — the oldest and most compatible way. Backed by the browser's File API; supports `multiple` and `accept="image/*"`. On mobile browsers, clicking it opens the native picker (Gallery/Files on Android, Photos on iOS Safari).
2. **File API** — for reading the selected files: `File`, `Blob`, `FileReader`, and `URL.createObjectURL()` (used for previews).
3. **File System Access API** — `window.showOpenFilePicker()` can open a picker directly from JS and even write files, but Safari support is limited.

In Leptos, these are reached from Rust/WASM via the `web-sys` crate — `view! { <input type="file" .../> }` ultimately uses the same browser File API.

---

## 4. Recommendation

**Start with Way 2** (with Way 1 embedded as the web fallback) — the industry-standard pattern:

- ~95% shared code
- One small `pick_files()` platform layer
- If a native gallery UX is needed later, swap only that layer for Way 4/5 without touching the rest of the app

---

## 5. Feature Checklist for Experiments

Features to try for file & image access, with what to use per platform. Tick each cell as it is verified.

### A. Picking

| # | Feature | Web (browser) | Tauri Desktop | Tauri Android | Tauri iOS |
|---|---------|--------------|---------------|---------------|-----------|
| 1 | Pick single file | `<input type="file">` | plugin-dialog `open()` | plugin-dialog | plugin-dialog |
| 2 | Pick multiple files | `<input multiple>` | `open({ multiple: true })` | same | same |
| 3 | Pick images only (filter) | `accept="image/*"` | `filters: [{ extensions }]` | same | same |
| 4 | Pick any file type | `<input type="file">` | `open()` no filter | same | same |
| 5 | Pick a folder | ❌ (only File System Access API, no Safari) | `open({ directory: true })` | ❌ not implemented ([#14587](https://github.com/tauri-apps/tauri/issues/14587)) | `open({ directory: true })` |

### B. Reading & Preview

| # | Feature | How |
|---|---------|-----|
| 6 | Show image preview | Web: `URL.createObjectURL(file)` · Tauri: `convertFileSrc(path)` or read bytes → blob |
| 7 | File metadata (name, size, MIME) | Web: `File` object · Tauri: path + `plugin-fs` `stat()` |
| 8 | Large file handling | Web: streams (`file.stream()`) · Tauri: read in chunks via fs plugin — verify memory usage with big photos |

### C. Permissions & Platform Behavior

| # | Feature | What to verify |
|---|---------|----------------|
| 9 | Android permissions | plugin-dialog picker should need **no** storage permission (uses system picker); verify on device |
| 10 | iOS Photos permission | PHPicker-based picker needs no permission prompt; verify `mode` option (iOS 14+) |
| 11 | Drag & drop files | Web: `dragover`/`drop` events · Tauri desktop: `onDragDropEvent` — desktop only |
| 12 | `<input type="file">` inside Tauri webview | The Way 1 experiment — test especially on Android (wry file chooser) |

**Suggested order:** 1 → 2 → 3 → 6 (the core requirement: multi-photo pick + preview), then 7–8, then the platform checks 9–12. Item 12 decides whether Way 1 alone is viable.

---

## 6. Task Mapping

**Task:** Option to Access Images, Photos, Directory and Documents.

| Task part | Checklist features | How |
|---|---|---|
| Images / Photos | 1, 2, 3, 6 | plugin-dialog with image filters + web `<input accept="image/*">`; preview via object URL / `convertFileSrc` |
| Documents | 4 (or 3 with pdf/doc filters) | plugin-dialog filters + web `accept=".pdf,.doc,.docx"` |
| Directory | 5 | Desktop/iOS: `open({ directory: true })` · **Android: needs [tauri-plugin-android-fs](https://github.com/aiueo13/tauri-plugin-android-fs)** (plugin-dialog has no Android folder picker) · Web: `webkitdirectory` fallback (no Safari `showDirectoryPicker`) |
| Required checks | 7, 9, 10, 12 | metadata display, Android/iOS permission behavior, Way 1 webview test |

**Not relevant to this task:** 8 (large files — nice-to-have), 11 (drag & drop — optional extra).

**Conclusion:** Images/Photos/Documents are fully covered by Way 2 (plugin-dialog + web input fallback). Directory is the hard part — on Android it requires a Way 4 community plugin (or a custom plugin), because the official dialog plugin cannot pick folders there.

---

## 7. Saving Files (research — not implemented yet)

**Goal:** a "save" feature — write a picked/processed file back to the device. Target platforms: **Web + Android + iOS**.

### The Ways to Save

#### Save Way 1: `<a download>` (Web)

```html
<a href={blob_url} download="photo.jpg">Save</a>
```

The browser writes the blob to the user's Downloads folder.

- ✅ Works in every browser, zero dependencies — the web branch regardless of what mobile uses
- ❌ Does **not** work inside Tauri's Android WebView — downloads need a native `DownloadListener`, and blob URLs can't be handed to it anyway
- ❌ Not reliable on iOS webview either

#### Save Way 2: `tauri-plugin-fs` → app-private folder ⭐

```js
writeFile("photo.jpg", bytes, { baseDir: BaseDirectory.AppData })
```

- ✅ **One identical code path for Android AND iOS** (desktop too)
- ✅ No permission prompt — the app owns its private directory
- ⚠️ File lands in the app's private folder: invisible in Android's Files app / Downloads
- ℹ️ On iOS this can be upgraded to user-visible: add `UIFileSharingEnabled` + `LSSupportsOpeningDocumentsInPlace` to Info.plist and the app's Documents folder appears in the Files app

#### Save Way 3: plugin-dialog `save()` + plugin-fs — user-visible "Save As" ⭐ (all Tauri platforms)

```js
const path = await save({ filters: [{ name: "Images", extensions: ["png"] }] });
await writeFile(path, bytes);   // plugin-fs handles every path format
```

Native "Save As" returns a location, plugin-fs writes the bytes to it.

- ✅ **Works on desktop, Android AND iOS** — verified in the [dialog plugin docs](https://v2.tauri.app/plugin/dialog/) (platform table; the only mobile caveat is the folder picker) and in the plugin source: Android's `DialogPlugin.kt` implements `saveFileDialog()` via SAF `ACTION_CREATE_DOCUMENT`, iOS's `DialogPlugin.swift` has `saveFileDialog()` too
- ✅ On Android this **is** the SAF "Save to…" system screen → file lands where the user chooses (Downloads, Drive, …), user-visible, no permission
- ⚠️ Returned "path" differs per platform: desktop = real path, iOS = `file://` URI, **Android = `content://` URI** — so the write **must** go through plugin-fs (`std::fs` can't open content URIs); per the docs, "the filesystem plugin works with any path format out of the box"
- ⚠️ Two plugins + capability permissions to wire up

> **Correction note:** an earlier version of this section claimed `save()` is desktop-only. That's wrong for current Tauri 2 — both mobile implementations exist. This also means `tauri-plugin-android-fs` (Save Way 4) is **not needed for saving**; it remains relevant only for Android *folder picking*.

#### Save Way 4: `tauri-plugin-android-fs` — SAF save dialog (Android, user-visible)

The same community plugin needed for Android folder picking also wraps SAF's `ACTION_CREATE_DOCUMENT` (`Picker::save_file`): a native "Save As" opens, the user picks a location (Downloads, Drive, …), the plugin writes the bytes there.

- ✅ Real user-visible save on Android, no storage permission (the user's location pick *is* the grant)
- ⚠️ Extra community plugin + Rust-side wiring; Android-only branch

### Per-platform summary

| Platform | Minimum (2 branches) | User-visible save |
|---|---|---|
| Web | `<a download>` | same |
| Android | plugin-fs → app folder (hidden) | Save Way 3 — official `save()` opens SAF "Save to…" |
| iOS | plugin-fs → Documents | Save Way 3, or just Info.plist flags → Documents visible in Files app |

### Key insights

1. **No single API covers all platforms** — same story as picking: web needs its own branch because a browser has neither a filesystem API nor the Tauri runtime.
2. **Minimum for Web + Android + iOS = 2 branches**: `save_file()` → browser: `<a download>` · Tauri: `plugin-fs writeFile` (identical on both mobile platforms).
3. **No permission prompts anywhere** — writing to the app's own folder needs none, and SAF's save dialog follows the picker model (out-of-process system UI; the user's choice is the grant). Permission popups only appear when an app wants blanket storage access, which saving never needs.
4. **User-visible save = Save Way 3 with the official plugins** (dialog `save()` + fs), on mobile too. The community `tauri-plugin-android-fs` is only needed for Android *folder picking*, not saving.

## Sources

- [Tauri Dialog plugin](https://v2.tauri.app/plugin/dialog/)
- [@tauri-apps/plugin-dialog JS reference](https://v2.tauri.app/reference/javascript/dialog/)
- [Tauri plugins list (platform support table)](https://v2.tauri.app/plugin/)
- [Tauri mobile plugin development](https://v2.tauri.app/develop/plugins/develop-mobile/)
- [Android folder picker issue (tauri #14587)](https://github.com/tauri-apps/tauri/issues/14587)
- [rfd GitHub](https://github.com/PolyMeilex/rfd) · [rfd AsyncFileDialog docs](https://docs.rs/rfd/latest/rfd/struct.AsyncFileDialog.html)
- [tauri-plugin-android-fs](https://github.com/aiueo13/tauri-plugin-android-fs)
- [file-picker-android](https://github.com/Berrysoft/file-picker-android)
- [tauri-plugin-camera](https://github.com/nanderstabel/tauri-plugin-camera)
- [wry (Tauri webview library)](https://github.com/tauri-apps/wry) · [wry issue #87](https://github.com/tauri-apps/wry/issues/87)
- [Android Photo Picker](https://developer.android.com/training/data-storage/shared/photo-picker)
- [MDN Web APIs](https://developer.mozilla.org/en-US/docs/Web/API)
- [Tauri fs plugin](https://v2.tauri.app/plugin/file-system/) · [BaseDirectory reference](https://v2.tauri.app/reference/javascript/api/namespacepath/#basedirectory)
- [tauri-plugin-android-fs Picker::save_file docs](https://docs.rs/tauri-plugin-android-fs/latest/tauri_plugin_android_fs/)
- [SAF ACTION_CREATE_DOCUMENT](https://developer.android.com/training/data-storage/shared/documents-files#create-file)
- [MDN `<a download>`](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/a#download)
