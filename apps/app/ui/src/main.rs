mod api;
mod history;
mod icons;
mod manufacturer;
mod nav;
mod offline;
mod screens;
mod ui;
mod update;
mod verdict;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(nav::App);
}
