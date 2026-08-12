mod app;
mod model;
mod pick;
mod save;
mod tauri_api;
mod way2;

use app::App;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
