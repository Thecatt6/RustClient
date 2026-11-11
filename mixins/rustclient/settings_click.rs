fn rustclient_settings_click(game: &mut Game<'_>, mod_id: &str, button: usize) {
    if mod_id != "rustclient" || button != 0 {
        return;
    }

    let mode = game
        .native_setting(
            "rustclient",
            rustclient_core::aim::settings::ROTATION_MODE_KEY,
        )
        .as_deref()
        .and_then(rustclient_core::aim::RotationMode::parse_tag)
        .unwrap_or_default()
        .next();
    game.native_set_setting(
        "rustclient",
        rustclient_core::aim::settings::ROTATION_MODE_KEY,
        mode.tag(),
    );
}
