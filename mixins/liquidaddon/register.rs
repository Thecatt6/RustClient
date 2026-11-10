fn register_liquidaddon(mods: &mut Vec<NativeMod>) {
    mods.push(NativeMod {
        id: String::from("liquidaddon"),
        title: String::from("LiquidAddon Rust"),
        description: String::from("Rust port for Horizon; settings are stored per instance."),
        keys: Vec::new(),
    });
}
