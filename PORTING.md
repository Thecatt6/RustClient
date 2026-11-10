# Porting notes

## Source and target pins

- Source: `Thecatt6/LiquidAddon`, revision `c128aaf0ffb98a815661d24381d1f548508e0b80`.
- Target Horizon: `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca`.
- Matching default `horizon-mixins`: `5a64af938ec86b00d53d16a408aabc0038fb3e57`.

The port is kept as its own Cargo project under this directory; it does not replace or modify the LiquidBounce add-on source.

## Feature inventory and integration risks

| Area | LiquidAddon features | Porting notes |
|---|---|---|
| Aim and shared AI | `Aimassist`, `AimbotAI`, aim engine, model selection/training, telemetry | The Kotlin engine depends on LiquidBounce's `RotationManager`, event bus, `DeepLearningEngine`, model manager and bundled `.params` models. These need Rust equivalents and a separate player-rotation hook; changing Horizon's rendered camera is not the same as changing the player's server-facing rotation. |
| Combat/input | `BlockHit`, `DoubleClicks`, `HitFlick`, `HitSelect`, `TriggerBotPlus` | Horizon at the pinned commit has a cancellable interaction-key hook (API 138), but target selection, attack cooldowns, swing behavior and vanilla block-dig semantics must be reimplemented against Horizon's own game state. |
| Projectiles | `AutoPearlAI`, `FireballDeflect`, `FireballJumpAI` | Rebuild trajectory/raycast, entity tracking, inventory selection and item-use logic on Horizon types; do not translate Minecraft/Fabric calls directly. |
| Movement and placement | `JumpReset`, `ScaffoldLegit` | The pinned build has a movement-input hook (API 360); ground/hurt timing and block placement still need Horizon-specific adapters and tests. |
| Network | `FakeLagV2` | The pinned build's client-packet hook (API 408) observes incoming server packets only, and `native_send_packet` (API 409) is a send helper. Neither is by itself an interception/buffering hook for the game's existing outgoing packets, so this feature needs a separately reviewed source mixin or an explicitly reduced design. |

## Hook facts verified against `fdc87ae`

- `native_interaction_key` (API 138): attack/use/pick interaction, cancellable.
- `native_movement_input` (API 360): local movement input before application.
- `native_camera_pose` (API 402): changes the rendered world camera; the hook documentation says the player's aim and first-person hand remain the player's.
- `native_client_packet` (API 408): observes decoded server-to-client play packets, read-only.
- `native_send_packet` (API 409): sends a serverbound play packet from a mod; it does not intercept the game's other sends.

These hook points are a starting map, not proof that a feature is already ported. Each module needs behavior tests against the pinned game source.