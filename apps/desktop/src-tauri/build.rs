fn main() {
    // Tauri embeds the configuration, the icon and the Windows manifest; Mosaïk only builds for
    // Windows, and elsewhere the crate is an empty stub so the workspace still builds.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        tauri_build::build();
    }
}
