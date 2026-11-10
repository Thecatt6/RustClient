fn liquidaddon_settings(game: &mut Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id != "liquidaddon" {
        return;
    }

    let mode = game
        .native_setting(
            "liquidaddon",
            liquidaddon_core::aim::settings::ROTATION_MODE_KEY,
        )
        .as_deref()
        .and_then(liquidaddon_core::aim::RotationMode::parse_tag)
        .unwrap_or_default();
    buttons.push(format!("Rotation mode: {}", mode.tag()));
}
