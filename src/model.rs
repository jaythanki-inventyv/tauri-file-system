//! Data types + small display helpers shared by every module.

/// One picked file, ready to render.
#[derive(Clone)]
pub struct Picked {
    pub name: String,
    pub size: f64,
    pub mime: String,
    /// Temporary blob URL for previewing (`<img src>` / `<iframe src>`).
    pub url: String,
    pub is_image: bool,
    /// Browser picks: `webkitRelativePath` (empty for normal picks).
    /// Native picks: the real path / content:// URI.
    pub rel_path: String,
    /// The File handle itself — needed to read the bytes when saving via Tauri.
    pub file: web_sys::File,
}

/// 1234 → "1.2 KB", 5_300_000 → "5.1 MB"
pub fn format_size(bytes: f64) -> String {
    if bytes >= 1_048_576.0 {
        format!("{:.1} MB", bytes / 1_048_576.0)
    } else if bytes >= 1024.0 {
        format!("{:.1} KB", bytes / 1024.0)
    } else {
        format!("{bytes:.0} B")
    }
}

/// Native picks give only a path, no MIME — guess it from the extension.
pub fn guess_image_mime(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    match () {
        _ if lower.ends_with(".png") => "image/png",
        _ if lower.ends_with(".gif") => "image/gif",
        _ if lower.ends_with(".webp") => "image/webp",
        _ if lower.ends_with(".heic") => "image/heic",
        _ => "image/jpeg",
    }
}
