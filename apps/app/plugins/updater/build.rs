// No commands: the shell decides what may be downloaded (`update_*` commands in src-tauri).
const COMMANDS: &[&str] = &[];

fn main() {
    // Android only: on iPhone, SideStore installs the new versions.
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
