#[cfg(target_os = "android")]
use tauri_plugin_android_fs::AndroidFsExt;

// Way 2 / Android-only save path: avoids tauri-plugin-dialog's per-file
// startActivityForResult round trip (the source of intermittent
// "no response from the native side" timeouts). The user picks the
// destination folder ONCE (a single Activity transition, this pair's
// android_pick_save_dir), then every file's target is created through a
// plain async coroutine call (android_create_file_path) — no further
// native dialogs, so there is nothing left to race on for files 2..N.
// The actual bytes are then written by the frontend's existing, proven
// chunked plugin:fs|open/write path (save.rs::write_file_bytes) — these
// two commands only resolve *where* to write, never touch file contents.

#[cfg(target_os = "android")]
#[tauri::command]
async fn android_pick_save_dir(
  app: tauri::AppHandle,
) -> Result<Option<tauri_plugin_android_fs::FsUri>, String> {
  app
    .android_fs_async()
    .picker()
    .pick_dir(None, false)
    .await
    .map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn android_create_file_path(
  app: tauri::AppHandle,
  dir: tauri_plugin_android_fs::FsUri,
  name: String,
) -> Result<String, String> {
  let uri = app
    .android_fs_async()
    .create_new_file(&dir, &name, None)
    .await
    .map_err(|e| e.to_string())?;
  let file_path: tauri_plugin_fs::FilePath = uri.into();
  Ok(file_path.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let builder = tauri::Builder::default()
    // Save Way 3 (used on the main page): dialog gives the user-chosen
    // location ("Save As" — SAF on Android), fs writes the bytes there.
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_fs::init())
    .plugin(tauri_plugin_opener::init());

  #[cfg(target_os = "android")]
  let builder = builder
    .plugin(tauri_plugin_android_fs::init())
    .invoke_handler(tauri::generate_handler![
      android_pick_save_dir,
      android_create_file_path
    ]);

  builder
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
