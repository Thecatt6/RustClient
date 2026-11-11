fn rustclient_settings(game: &mut Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id != "rustclient" {
        return;
    }

    let mode = game
        .native_setting(
            "rustclient",
            rustclient_core::aim::settings::ROTATION_MODE_KEY,
        )
        .as_deref()
        .and_then(rustclient_core::aim::RotationMode::parse_tag)
        .unwrap_or_default();
    let mut label = String::from("Rotation mode: ");
    label.push_str(mode.tag());
    buttons.push(label);
}
