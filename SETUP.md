# Setup Log — Commands & Steps

Every command used to build this project, in order, with what it does and why.

## 0. Prerequisites (already installed)

| Tool | Version | Check command | Purpose |
|---|---|---|---|
| Rust + Cargo | 1.96.0 | `rustc --version` | Compiles the app |
| Trunk | 0.21.14 | `trunk --version` | Builds/serves the Leptos WASM web app |
| Tauri CLI | 2.11.4 | `cargo tauri --version` | Builds desktop/Android/iOS shells |
| wasm32 target | — | `rustup target list --installed` | Rust → WebAssembly for the browser |
| Android/iOS targets | — | same | Rust → mobile builds |

If a target is missing: `rustup target add wasm32-unknown-unknown`

## 1. Project scaffold (manual)

Files created by hand (no generator):

| File | Purpose |
|---|---|
| `Cargo.toml` | Rust package: `leptos` (csr), `wasm-bindgen`, `web-sys` (File/FileList/Url APIs) |
| `index.html` | Trunk entry page + all CSS |
| `Trunk.toml` | Dev server config — port **1420** (the port Tauri expects) |
| `src/main.rs` | Mounts the Leptos `App` to `<body>`, declares the modules |
| `src/app.rs` | UI only: pickers, grid, preview overlay, status/progress (the component) |
| `src/model.rs` | `Picked` struct + `format_size` / `guess_image_mime` helpers |
| `src/tauri_api.rs` | Webview↔native bridge: `tauri_invoke`, `is_tauri`, `close_resource` |
| `src/pick.rs` | Picking: `read_with_progress` (browser picks), `pick_images_native` (Tauri) |
| `src/save.rs` | Saving: browser `<a download>` + Tauri Save-As→chunked-write flow |

(The last four files came from a later refactor — originally everything lived in `app.rs`.)
| `.gitignore` | Ignores `target/`, `dist/` |

## 2. Build the web app

```sh
trunk build
```

Compiles Rust → WASM (`cargo build --target wasm32-unknown-unknown` under the hood), bundles JS glue + `index.html` into `dist/`.

**Fix made during first build:** Leptos' typed attribute system rejects the non-standard `webkitdirectory` attribute (compile error `no method named webkitdirectory`). Solution: a `NodeRef` + `Effect` sets the attribute on the element after mount (`src/app.rs`).

## 3. Run the web app (dev server)

```sh
trunk serve
```

Serves at **http://localhost:1420** with auto-rebuild on file changes. This is the pure-browser version — no Tauri involved.

## 4. Add the Tauri shell

```sh
cargo tauri init \
  --app-name tauri-image-files \
  --window-title "Image & Files Picker" \
  --dev-url http://localhost:1420 \
  --frontend-dist ../dist \
  --before-dev-command "trunk serve" \
  --before-build-command "trunk build" \
  --ci
```

Creates `src-tauri/` (Rust binary that opens a native window with a webview pointing at the Leptos app). Flags:

- `--dev-url` — during dev the webview loads Trunk's server
- `--frontend-dist` — for release builds it embeds `dist/`
- `--before-dev-command` / `--before-build-command` — Tauri starts/builds the frontend automatically
- `--ci` — non-interactive (accept defaults)

After init, two adjustments were made:

1. `src-tauri` added to `[workspace] members` in the root `Cargo.toml` so both crates share one workspace.
2. App identifier changed in `src-tauri/tauri.conf.json` from the default `com.tauri.dev` to `com.purv.imagefiles` — Android/iOS init rejects the default identifier.

## 5. Run per platform

```sh
cargo tauri dev              # Desktop (macOS window)
cargo tauri android init     # one-time Android project generation
cargo tauri android dev      # Android emulator/device
cargo tauri ios init         # one-time iOS project generation
cargo tauri ios dev          # iOS simulator/device
```

The web version needs only `trunk serve` (no Tauri).

## 6. iOS run (done)

```sh
xcodebuild -version                          # verify Xcode (26.4)
xcrun simctl list devices available          # list simulators
cargo tauri ios init                         # generates src-tauri/gen/apple/app.xcodeproj
cargo tauri ios dev "iPhone Air"             # build + launch on the named simulator
```

`ios init` generates a full Xcode project under `src-tauri/gen/apple/` (plists, xcodeproj). `ios dev "<simulator name>"` compiles Rust for `aarch64-apple-ios-sim`, runs xcodebuild, boots the simulator, installs and launches the app. Passing the simulator name avoids the interactive device prompt.

## 7. Android setup & run (physical phone, no Android Studio)

Installed via Homebrew + sdkmanager, sizes noted per step:

```sh
# 1. Command-line tools (~150 MB) — provides sdkmanager
brew install --cask android-commandlinetools

# 2. Accept licenses (no download)
yes | sdkmanager --licenses --sdk_root=/opt/homebrew/share/android-commandlinetools

# 3. platform-tools / adb (~20 MB)
sdkmanager --sdk_root=/opt/homebrew/share/android-commandlinetools "platform-tools"

# 4. Platform APIs (~70 MB)
sdkmanager --sdk_root=/opt/homebrew/share/android-commandlinetools "platforms;android-35"

# 5. Build tools (~60 MB)
sdkmanager --sdk_root=/opt/homebrew/share/android-commandlinetools "build-tools;35.0.0"

# 6. NDK (~0.78 GB download, ~3.5 GB on disk) — required to compile Rust for Android
sdkmanager --sdk_root=/opt/homebrew/share/android-commandlinetools "ndk;27.2.12479018"
```

Tip: check a download's size before installing without downloading it:

```sh
curl -sI https://dl.google.com/android/repository/android-ndk-r27c-darwin.zip \
  | grep -i content-length | awk '{printf "%.2f GB\n", $2/1024/1024/1024}'
```

Environment variables (add to `~/.zshrc`):

```sh
export ANDROID_HOME="/opt/homebrew/share/android-commandlinetools"
export NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
```

Phone preparation (Pixel 7 used):

1. Settings → About phone → tap **Build number** 7 times (enables Developer mode)
2. Settings → Developer options → **USB debugging ON**
3. Connect via USB; tap **Allow** on the "Allow USB debugging?" prompt
4. Verify: `adb devices -l` should show the phone as `device` (not `unauthorized`)

Run:

```sh
cargo tauri android init    # one-time: generates src-tauri/gen/android (Gradle project)
cargo tauri android dev     # compiles Rust for aarch64-linux-android, Gradle builds APK, installs & launches on the phone
```

**Troubleshooting — "database or disk is full" (xcodebuild exit 65):** the disk ran out of space mid-build. Freed space by deleting regenerable build caches, then re-ran `ios dev`:

```sh
rm -rf ~/Library/Developer/Xcode/DerivedData/app-*   # this app's Xcode cache
rm -rf target/debug                                  # desktop build cache (~4 GB, rebuilds on next `cargo tauri dev`)
```

## 8. PDF preview — single iframe path

The preview overlay uses one code path for PDFs on every platform: `<iframe src=blob_url>`. The Android user-agent check and its "not supported" fallback message were removed (`src/app.rs`) — no extra dependency bundled. Note: desktop browsers/webviews render the PDF via their built-in viewer; Android's System WebView has no PDF renderer, so the iframe stays blank there.

## 9. Save/Download feature (per-file 💾 + Download All) — Save Way 3

Implements RESEARCH.md §7 **Save Way 3**: user-visible "Save As" via the official plugins on every Tauri platform, `<a download>` on the web.

```
do_save(file)
   ├── browser → programmatic <a download href=blob> click → browser Downloads
   └── Tauri   → plugin-dialog `save` ("Save As"; SAF screen on Android)
                 → returns path (desktop) / file:// (iOS) / content:// (Android)
                 → plugin-fs `write_file` writes the blob bytes there
```

> First version of this feature used Save Way 2 (custom Rust command + `std::fs` → app-private dir). Replaced after re-reading the dialog docs: `save()` **is** supported on Android/iOS (SAF `ACTION_CREATE_DOCUMENT` / `saveFileDialog`), so the file now lands where the user chooses — visible in Downloads/Files.

Backend (`src-tauri/`):

1. `Cargo.toml` — `tauri-plugin-dialog = "2"`, `tauri-plugin-fs = "2"`
2. `src/lib.rs` — both registered: `.plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_fs::init())` (custom `save_file` command removed)
3. `capabilities/default.json` — `dialog:default`, `fs:default`, and `fs:allow-write-file` with scope `{"path": "**"}` (the dialog-chosen location can be anywhere)
4. `tauri.conf.json` — `"withGlobalTauri": true` so the WASM frontend can call `window.__TAURI__.core.invoke` (frontend is Rust, not npm — no JS API package)

Frontend (`src/app.rs`):

1. `Picked` keeps the `web_sys::File` handle (bytes source)
2. `save_picked()`: invoke `plugin:dialog|save` with `defaultPath` = file name → `null` means cancelled → read blob bytes (`array_buffer`) → invoke `plugin:fs|write_file` with the bytes as **raw payload** and the target path URL-encoded in a `path` header (mirrors the official JS binding — checked `plugins-workspace/plugins/fs/guest-js/index.ts`)
3. 💾 button per card (`stop_propagation` so the preview doesn't open) · "⬇️ Download All" runs the saves **sequentially** so the per-file Save As dialogs don't overlap · every step lands in the status line (saving n/total… / ✅ saved path / ✖️ cancelled / ⚠️ error)
4. `index.html` — CSS for `.save-btn`, `.toolbar`, `.dl-all`

Note: web browsers may prompt "allow multiple downloads" on Download All.

### §9 follow-up 4: web Download All → single ZIP (Safari fix)

Safari allows only **one** download per user click — programmatic multi-`<a download>` clicks silently drop all but the first (Chrome just prompts). Fix: on the web branch, Download All now bundles every picked file into one `files.zip` and downloads that. The ZIP is written in plain Rust in save.rs (`ZipWriter` — stored entries, no compression, own CRC32; duplicate names get an index prefix). Tauri branch unchanged (sequential per-file "Save As"). Verified with Playwright in **both Chromium and WebKit** (Safari's engine): exactly one `files.zip`, `unzip -t` reports no errors. Also removed the pick-flow progress bar in the same session (local picks are near-instant; the bar remains for saves).

## 10. Automated testing (Playwright)

Two end-to-end test scripts, in the repo at `tests/web-test.js` and `tests/android-test.js`. Run with `node` (needs `npm i playwright` and, for the web test, a running `trunk serve`; for the Android test, a USB-connected device with the debug app installed).

### 10a. Web build — headless Chromium (`test.js`)

Drives `trunk serve` output in a real browser via Playwright. Generates tiny valid PNG/PDF files on the fly and asserts 16 checks — **all passed**:

- page loads, all 3 pickers present
- multi-image pick (3 PNGs) → status `✅ 3 file(s) loaded`, 3 cards, `<img>` previews
- multi-PDF pick (2) replaces selection; metadata line shows `size · application/pdf`
- **empty change event keeps the previous selection** (the Android cancelled-pick guard)
- preview overlay opens with PDF iframe, closes on backdrop click
- 💾 fires a browser download (`doc1.pdf`) and does **not** open the preview (stop_propagation)
- Download All fires one download per file
- re-picking the same file works (input value reset)
- zero JS console errors

### 10b. On-device Android — Playwright `_android` over adb (`android-test.js`)

Playwright's Android support attaches to the app's WebView through adb (works because debug builds have WebView debugging on). Native UI (SAF dialog) is driven with `uiautomator dump` + `adb shell input tap`. Flow, on a real Pixel 7:

1. force-stop + relaunch app, attach to `com.purv.imagefiles.debug` WebView
2. inject a 3-file pick (2 PNG + 1 PDF) into the real `<input>` via `DataTransfer` + `change` event — exercises the exact app code path without the picker UI
3. assert cards/status/previews render on the device — **passed**
4. tap 💾 → assert the **native SAF "Save As" dialog opened**, find its SAVE button in the uiautomator dump, tap it
5. assert status `✅ Saved: content://…downloads…` and the file exists: `/sdcard/Download/test-a.png`, 70 bytes — **user-visible save verified end-to-end**

### §9 follow-up: save progress % + chunked writes (fixes the 0-byte bug)

`save_picked` no longer sends one giant `write_file` payload. It now streams: `plugin:fs|open` (write/create/truncate) → loop of `plugin:fs|write` with 1 MB `Uint8Array` chunks → `plugin:resources|close`. Each chunk updates a `progress` signal → the status line shows `💾 Saving "x"… 42%` plus a thin progress bar (`.progress-wrap/.progress-bar` in index.html). Capabilities gained `fs:allow-write` and scoped `fs:allow-open`. This both gives real percentage feedback and fixes the large-file 0-byte failure below (the whole-payload IPC was the culprit). Web branch unchanged (browser downloads have no JS-visible progress).

### §9 follow-up 2: pick/upload progress (real bytes)

The `change` handler now processes the list asynchronously and, per file, **actually reads the blob's bytes** chunk-by-chunk via `Blob.stream()` (`read_with_progress` in app.rs, discarding chunks) — so the bar genuinely tracks a 200 MB file instead of flashing 0→100. Status shows `📥 Loading 1/3: "big.pdf" (205.0 MB)… 47%`. Unreadable files (cloud item never downloaded) fail fast and are reported: `⚠️ 2 loaded, 1 unreadable (big.pdf)`. Added web-sys features `ReadableStream`, `ReadableStreamDefaultReader`. Note: the wait *inside* the OS picker (before `change` fires) is still invisible to the page — only `⏳ Waiting for your selection…` is possible there.

### Finding: large-file save writes 0 bytes on Android

`/sdcard/Download/` showed the user's manual saves: an 18 KB PDF saved fine, but two attempts at saving a ~105 MB PDF produced **0-byte files** — the SAF dialog creates the file, then the `write_file` IPC with the whole payload fails on Android (single huge IPC body). Small files are unaffected. Fix if needed: stream the write in chunks via plugin-fs `open` + repeated `write` calls instead of one `write_file`.

### §9 follow-up 3: "Pick Images" opens Photo Library directly (no iOS sheet)

The iOS sheet (Photo Library / Take Photo / Choose Files) on `<input type=file accept="image/*">` is OS UI — HTML can't restrict it. But the dialog plugin's iOS source shows: **image-only filters → `PHPickerViewController`**, i.e. the Photo Library opens directly. So the Pick Images button now branches (`pick_images_native` in app.rs):

- **Tauri (iOS/Android/desktop)**: `plugin:dialog|open` with image-extension filters → iOS: PHPicker (library only, no sheet, no permission prompt) · Android: system image picker → paths → bytes via `plugin:fs|read_file` → wrapped in a `File` (`FilePropertyBag` + guessed MIME) so preview/save reuse the same code. Bonus: the card's "path:" line now shows a real path/URI.
- **Web**: unchanged `<input>` (a browser cannot restrict the OS sheet).

Added capability `fs:allow-read-file` (scope `**`) and web-sys feature `FilePropertyBag`. Documents/Directory pickers unchanged.

### §9 follow-up 5: "Download All" appeared to freeze — root cause + fixes

Investigated with Playwright driving the real Pixel 7 over adb (`tests/android-test.js` pattern): injected multiple files, clicked Download All, and used `uiautomator dump` to find/tap each native SAF "Save" dialog automatically, while watching `adb logcat` for crash/reload markers.

**Root cause: the debug APK wasn't 16 KB page-size aligned.** Every time a native dialog opened (SAF Save-As), Android popped an **"Android App Compatibility"** system warning ("This app isn't 16 KB compatible… LOAD segment not aligned") **on top of the SAVE button**, silently swallowing the tap. Across a multi-file Download All this fires repeatedly, making the app look like it randomly hangs/reloads mid-save. Confirmed via `llvm-readelf -l libapp_lib.so`: LOAD segments were 0x1000 (4 KB) aligned.

**Fix:** added `.cargo/config.toml` with `-Wl,-z,max-page-size=16384` in `rustflags` for all four Android targets. Rebuilt → `llvm-readelf` now shows `0x4000` (16 KB) alignment, and the compatibility dialog no longer appears. Re-ran the same automated Download All test: dialogs now advance correctly file-to-file without the interrupting popup.

**Also added:** `tauri_invoke_timeout()` in `tauri_api.rs` — wraps `tauri_invoke` with `Promise.race` against a `setTimeout`, so any native round-trip that never calls back (SAF dialog, fs open/write/close) surfaces as a clear error after a bound (60s for user-facing dialogs, 10–15s for fs ops) instead of freezing that step forever with no feedback. Applied to `plugin:dialog|save`, `plugin:dialog|open`, `plugin:fs|open`, `plugin:fs|write`, `plugin:fs|read_file`, `plugin:resources|close`.

**Known follow-up (not yet root-caused):** occasionally one file in a rapid multi-file Download All ends up 0 bytes on disk even though the SAF dialog was confirmed — reproduced with both ~1.5s and ~3s gaps between automated taps, so it isn't purely a "too fast for a human" artifact. Doesn't hang the UI (thanks to the timeout guard above) and hasn't been seen with a single Save. Needs more investigation if it shows up in normal manual use.

### Root cause found: random page reloads ("selection disappears")

The APK's embedded `dist/index.html` contained **trunk's live-reload script** (`new WebSocket(...)` + `window.location.reload()`) — `trunk serve` injects it into everything it writes to `dist/`, and a later `cargo tauri android build` embedded that dist. On the phone the script can't reach the dev server, its reconnect logic eventually fires `window.location.reload()`, and the page reloads at a random moment — wiping the selection (looked like "upload finished, then everything vanished"). No crash involved: logcat showed the app process and even the WebView renderer stayed alive across the reload; the only marker was the page-load `integrity` warning appearing again.

**Rule:** never ship a dist written by `trunk serve`. Standalone installs must come from `cargo tauri android build --debug --apk` (its `beforeBuildCommand: trunk build` writes a clean dist — verify with `grep -c "location.reload" dist/index.html` → 0), then `adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`.

### Debugging notes from this session

- `Tauri/Console … integrity attribute` warning = harmless Chromium notice, **but** it fires on page load — seeing it right after picking files means the page reloaded, i.e. Android killed the activity while the picker was open (the "selection disappeared" mystery). Check Developer options → "Don't keep activities" and battery optimization.
- `E tauri::protocol::tauri: Failed to request http://<ip>:1420/` = dev-build APK can't reach the Mac's dev server (different network / server stopped). Dev builds need the server; standalone needs `cargo tauri android build --debug --apk` + `adb install`.
- Phone and Mac on different networks: `adb reverse tcp:1420 tcp:1420` routes the dev server over USB; `Trunk.toml` now binds `0.0.0.0` so both localhost and LAN work.
