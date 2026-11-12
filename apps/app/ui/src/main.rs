mod api;
mod icons;
mod nav;
mod screens;
mod ui;
mod verdict;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(nav::App);
}
