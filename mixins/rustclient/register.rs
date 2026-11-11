fn register_rustclient(mods: &mut Vec<NativeMod>) {
    mods.push(NativeMod {
        id: String::from("rustclient"),
        title: String::from("RustClient"),
        description: String::from(
            "Independent Rust rewrite for Horizon; settings are stored per instance.",
        ),
        keys: Vec::new(),
    });
}
