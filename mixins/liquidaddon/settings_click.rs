fn liquidaddon_settings_click(game: &mut Game<'_>, mod_id: &str, button: usize) {
    if mod_id != "liquidaddon" || button != 0 {
        return;
    }

    let mode = game
        .native_setting(
            "liquidaddon",
            liquidaddon_core::aim::settings::ROTATION_MODE_KEY,
        )
        .as_deref()
        .and_then(liquidaddon_core::aim::RotationMode::parse_tag)
        .unwrap_or_default()
        .next();
    game.native_set_setting(
        "liquidaddon",
        liquidaddon_core::aim::settings::ROTATION_MODE_KEY,
        mode.tag(),
    );
}
