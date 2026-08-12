//! The bridge between the webview (this WASM code) and Tauri's native side.
//! Everything native — dialogs, filesystem — goes through `tauri_invoke`.

use leptos::prelude::window;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {
    /// `window.__TAURI__.core.invoke` — exists only inside a Tauri webview
    /// (enabled via `withGlobalTauri` in tauri.conf.json).
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    pub async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Same as `tauri_invoke`, but gives up after `timeout_ms` instead of hanging
/// forever. Some native round-trips (SAF dialogs, fs open/write/close) can
/// occasionally never resolve their JS callback — e.g. Android dropping the
/// callback across an activity transition — which otherwise freezes the UI
/// on that step with no error and no way out. A stuck step now surfaces as
/// a clear error message instead of a silent, permanent hang.
pub async fn tauri_invoke_timeout(
    cmd: &str,
    args: JsValue,
    timeout_ms: i32,
) -> Result<JsValue, String> {
    let mut timer_id = 0;
    let timeout = js_sys::Promise::new(&mut |resolve, _| {
        let cb = wasm_bindgen::closure::Closure::once_into_js(move || {
            let _ = resolve.call1(&JsValue::NULL, &JsValue::from_str("__timeout__"));
        });
        timer_id = window()
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                timeout_ms,
            )
            .unwrap_or(0);
    });

    let cmd_owned = cmd.to_string();
    let race = js_sys::Promise::race(&js_sys::Array::of2(
        &wasm_bindgen_futures::future_to_promise(async move {
            tauri_invoke(&cmd_owned, args).await
        }),
        &timeout,
    ));

    let result = JsFuture::from(race).await;
    window().clear_timeout_with_handle(timer_id);

    match result {
        Ok(v) if v.as_string().as_deref() == Some("__timeout__") => {
            Err(format!("\"{cmd}\" timed out after {timeout_ms}ms (no response from the native side)"))
        }
        Ok(v) => Ok(v),
        Err(e) => Err(format!("{e:?}")),
    }
}

/// Are we running inside the Tauri app (vs a plain browser)?
pub fn is_tauri() -> bool {
    js_sys::Reflect::get(&window(), &"__TAURI__".into())
        .map(|v| !v.is_undefined() && !v.is_null())
        .unwrap_or(false)
}

/// Is this Android? The frontend is one WASM binary shared by every Tauri
/// target (desktop/iOS/Android/web) — there is no compile-time `cfg` to
/// branch on here, only a runtime check. Used to route Way 2's save through
/// the Android-only `tauri-plugin-android-fs` commands.
pub fn is_android() -> bool {
    window()
        .navigator()
        .user_agent()
        .map(|ua| ua.contains("Android"))
        .unwrap_or(false)
}

/// Close a native resource handle (e.g. the file opened for writing).
/// Timeout-guarded: a stuck close must not hang the save loop forever.
pub async fn close_resource(rid: &JsValue) -> Result<JsValue, String> {
    let args = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&args, &"rid".into(), rid);
    tauri_invoke_timeout("plugin:resources|close", args.into(), 8_000).await
}
