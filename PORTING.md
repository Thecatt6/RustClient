# Porting notes

Everything below was checked against the pinned Horizon checkout
(`fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca`) and the pinned LiquidAddon commit
(`c128aaf0ffb98a815661d24381d1f548508e0b80`). Where something is still a guess it says so.

## Pins

| Thing | Revision |
|---|---|
| Source | `Thecatt6/LiquidAddon` `c128aaf0ffb98a815661d24381d1f548508e0b80` (GPL-3.0) |
| Target | `horizon-rust/horizon` `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca` |
| Mixins | `horizon-rust/horizon-mixins` `5a64af938ec86b00d53d16a408aabc0038fb3e57` (the default that Horizon commit declares) |
| Game | Minecraft: Java Edition 26.3 |
| Toolchain | Rust 1.95.0 (`rust-version` in `Cargo.toml`) |

## What a Horizon native mod actually is

Not a Fabric JAR and not a mod-loader plugin: a **source-mixin bundle**. A directory with a
`horizon-mixin.toml` and helper `.rs` files, transformed into the game's own source at build time by
[horizon-mixins](https://github.com/horizon-rust/horizon-mixins) and compiled with it. Helpers are
compiled *inside* the target function, so they see private fields and can bind `self`.

```toml
[mod]
id = "liquidaddon"
version = "0.1.0"
priority = 100

[[injection]]
id = "double-clicks"
target = "horizon_client::game::native_hooks::Game<'_>::native_interaction_key"
kind = "HEAD"
helper = "double_clicks.rs"
bindings = [
  { name = "game", ty = "&mut Game<'_>", value = "self" },
]
```

Injection kinds available at that revision: `HEAD` (and cancellable `HEAD`), `BEFORE_CALL`,
`AFTER_CALL`, `AFTER_ASSIGN`, `MODIFY_EXPRESSION_VALUE`, `RETURN`, `MODIFY_ARG`, `MODIFY_ARGS`,
`MODIFY_CONSTANT`, `MODIFY_VARIABLE`, `WRAP_OPERATION`, `OVERWRITE`. `OVERWRITE` conflicts with
every other hook on the same function, so prefer `HEAD`/`RETURN`. Several hooks ride the same
function: same site runs priority-descending, mod id, then injection id.

Two more facts that shape the port:

- **API gating.** A bundle that uses a named hook declares its API numbers (`"hooks": [138, 360]`)
  in its `.hnmod`'s `horizon-native.json`; the launcher refuses a bundle whose hooks the pinned
  Horizon does not have. `native-hooks.json` in the Horizon tree is the authoritative list (432
  hooks, 174 helpers at this revision) — the doc comments in the source occasionally carry an older
  number, so take the number from `docs/native-mods.md` / `native-hooks.json`.
- **Trust.** Native mods are native code with the player's rights, always on, not sandboxed. The
  launcher shows a trust notice and the registry reviews a mod before listing it. Builds are pinned
  to one Horizon commit and the launcher may refuse or warn across commits.

## The rotation problem

This is the single decision the whole aim half of the port hangs on, so it comes before the modules.

Horizon has **no hook that writes the player's rotation**. What it has:

- `native_camera_pose` (API 402) — moves the *rendered* camera. Its own documentation is explicit:
  "the player, where they aim, the first-person hand and which chunks load stay the player's". Using
  it for aim assist would move the picture and not the hit.
- `native_interaction_key` (API 138) — lets a mod fire an attack or a use, but not aim it.
- `native_movement_input` (API 360) — forward/strafe/jump/sneak, not yaw/pitch.

What the client actually sends is one `(yaw, pitch)` pair. In the pinned source that pair is
`Game::yaw` / `Game::pitch`, read by `Game::remote_send_state` in
`crates/horizon-client/src/game/remote.rs` and handed to `PositionSender::update`, which emits
`MovePlayerRot`/`MovePlayerPosRot`. The same fields feed the camera, `view_vector` and
`native_view_hit`. So:

1. **The honest first target** is a `HEAD` mixin on `remote_send_state`, binding `self` as
   `&mut Game<'_>` and adjusting `self.yaw`/`self.pitch` for the tick. That is *visible* aim: the
   camera turns too, exactly like LiquidBounce with its rotation manager in visible mode, and it is
   the only variant whose semantics can be stated without lying.
2. **Silent aim** (packets turn, camera does not) needs the local `(yaw, pitch)` the function reads
   to be replaced, or `self.yaw` to be set at `HEAD` and restored at `RETURN`. The second works only
   because the packet is built inside the body — but it also means the camera sample that happens
   later in the frame sees the restored value, and *that* ordering has to be proven, not assumed. Do
   not ship silent aim on the strength of "the tests pass"; prove it with the launcher's packet
   recording.

Either way the mixin is pinned to a private method of `Game`, so it is the most fragile target in
the port. Put it behind one module (`rotation`) with a version check and a build that fails loudly,
rather than sprinkling `self.yaw = …` through the modules.

## Verified hook map (client side)

The client hooks are the only ones that exist when you play on someone else's server. Every
server-side hook in `native-hooks.json` (`attack_entity`, `hurt_enemy` 59, `living_knockback`,
`player_arm_swing` 308, `left_click_block` 66, `right_click_block` 46, `use_item` 13 …) fires only
when **Horizon is the server** — single player, a world opened to LAN, or a Horizon dedicated
server. A module that needs one of those is a single-player/LAN feature or needs a client-side
substitute.

| Hook | API | Side | Contract | Signature at `fdc87ae` |
|---|---|---|---|---|
| `native_interaction_key` | 138 | client | cancellable | `(button: i32, key_mapping: &str, hand: Hand, swing_hand: &mut bool, canceled: &mut bool)` |
| `native_movement_input` | 360 | client | listen, mutable | `(forward: &mut f32, strafe: &mut f32, jump: &mut bool, sneak: &mut bool)` |
| `native_camera_pose` | 402 | client | listen | `(partial: f32, position: &mut [f64; 3], yaw: &mut f32, pitch: &mut f32)` |
| `native_client_packet` | 408 | client | read-only | `(packet: &ClientboundPlayPacket)` |
| `native_send_packet` | 409 | client | action | `(packet: ServerboundPlayPacket) -> Result<(), String>` (`game/native_net.rs`) |
| `native_mouse_button_pre` | 132 | client | cancellable | `(button: i32, pressed: bool, modifiers: u32, canceled: &mut bool)` |
| `native_frame` | 1 | client | listen | `(elapsed: Duration)` |
| `native_tick` | 2 | client | listen | `()` |

`native_interaction_key` details that matter for the click modules: `button` 0 is attack, 1 is use
(*once per hand tried*, main hand first), 2 is pick block; attack fires when an attack is allowed and
again each tick a held attack key keeps digging. **A cancelled attack or use still swings the hand**
unless the mod also clears `swing_hand` (it arrives `true`). Cancelling a use ends the click, so the
off hand gets no turn.

Client-side reads the bundle can use without a hook (`Host` helpers, documented under `helpers` in
`native-hooks.json`): `player` (`NativePlayer { x, y, z, yaw, pitch, … }`), `entity`,
`entity_rotation`, `entity_velocity`, `entity_bounding_box`, `entity_eye_position`,
`entity_view_vector`, `entity_distance_to_sqr` (API 379), `entity_kind` (373), `held`,
`living_hand_item`, `client_entities`, `raycast`, `view_hit` (116), `camera`, `block_collision`,
`playing`, `tick_stats`.

## Scope: what "all of LiquidBounce" can mean

LiquidBounce is a whole client — roughly 300 k lines of Kotlin across a module/settings framework, a
click GUI, an event bus, a rotation manager, HUD/element renderers, commands, config serialisation
and the deep-learning engine — and this add-on is a thin layer on top of it. Porting "everything" is
not one job, and it is not what the 13 modules need. The realistic dependency list, in the order the
add-on actually touches them:

| LiquidBounce primitive | What the add-on uses it for | Where it lands here |
|---|---|---|
| `Module` / `Toggleable` / `Choice` settings | every module's state | Horizon-side settings struct + the enums from `AimEngine.kt` |
| `RotationManager` | aim assist, hit flick, scaffold | the rotation mixin described above |
| `EventBus` + `EventMotion`/`EventAttack` | module hooks | Horizon's typed hooks, no bus |
| `CombatSample` / `DeepLearningEngine` / `.params` models | `AimbotAI`, `AutoPearlAI` | new Rust inference + [the input order](crates/liquidaddon-core/src/aim/ai_features.rs) |
| `CommandRegistrar`, chat formatting, `ClickGUI` | `/liquidaddon`, HUD tags | `Host::send_chat` / a mod screen; the HUD tags are plain strings already |

Everything else in LiquidBounce (its own KillAura, Scaffold, Disabler, exploits…) is out of scope
unless a module here depends on it. A useful rule for this repository: *if the pinned LiquidAddon
does not call it, it is not part of the port.*

## Module-by-module plan

`Core` is what already lives in `crates/liquidaddon-core`; everything else is work.

| Module (source) | Core | Client-side path | Notes / risks |
|---|---|---|---|
| `DoubleClicks` (226 l.) | ✅ `combat::double_clicks` | `interaction_key` (138): replay the planned follow-up through the vanilla path; keep `swing_hand`/`canceled` rules | Whole scheduling policy already ported; the bundle only supplies "over an entity / mining / miss cooldown" and performs the click |
| `HitSelect` (101 l.) | — | `interaction_key` (138) to defer/allow an attack; target hurt state from `entity_*` or from `client_packet` (408) | Needs a client-side hurt signal; check whether a helper exposes it before parsing packets |
| `JumpReset` (385 l.) | ✅ `hud::jump_reset` | knockback arrival from `client_packet` (408, `SetEntityMotion` for the local player) and the jump from `movement_input` (360) | Clean mapping: the original keys off receiving knockback and answering on the exact tick |
| `BlockHit` (705 l.) | — | `interaction_key` (138) + `movement_input` (360) + `held` | The original drives sword-block timing; 26.3 has shields, so verify the item semantics before porting the branches |
| `TriggerBotPlus` (959 l.) | — | `interaction_key` (138) + `raycast`/`view_hit` + `entity_*` | Everything it needs exists client-side; the hard part is the filter policy, not the hooks |
| `HitFlick` (722 l.) | — | the rotation path above | Needs the aim core running first |
| `Aimassist` / `AimbotAI` (115 + 102 l. over `AimEngine.kt` 2 620 l.) | partially (`ai_*`, `hand_gate`, `mouse_pattern_guard`) | rotation path above + `player`/`entity_*` | `AimEngine.kt` also holds `AimSettings` (interface), `AimValues`, the ten enums, `AimassistCore` and `AiCoreRegistry`: the settings are a Rust trait + struct, the enums map to tagged enums |
| `AutoPearlAI` (1 474 l.) | — | `held`/`living_hand_item` + `entity_*` + rotation path; projectile maths is plain Rust | Trajectory + inventory choice can be ported and tested as core; keep it out of the bundle |
| `FireballDeflect` (297 l.) | — | `entity_*` for the projectile + `interaction_key` (138) for the swing | Raycast/trajectory maths belongs in core |
| `FireballJumpAI` (835 l.) | — | `entity_*` + `movement_input` (360) + rotation path | Same split as above |
| `ScaffoldLegit` (442 l.) | — | `movement_input` (360), `held`, `block_collision`; placement itself needs a mixin into the client's block-place path | Placement prediction is the open question; start with the "where" (core) and leave the "place" last |
| `FakeLagV2` (1 013 l.) | — | **blocked as designed** | `client_packet` only observes inbound, and `send_packet` adds packets rather than holding the game's own back. A real port needs a mixin at the send site (`PositionSender`/`send`) plus a queue with a tick budget — or the module is dropped. Do not fake it with duplicated packets |

### What the aim engine needs beyond the hooks

`AimEngine.kt` calls LiquidBounce's `DeepLearningEngine` with bundled `.params` models. On Horizon
that is a separate piece of work: read the model format, run the forward pass in Rust (six inputs,
[`ai_features`](crates/liquidaddon-core/src/aim/ai_features.rs) already pins the order), and feed
[`ai_stats`](crates/liquidaddon-core/src/aim/ai_stats.rs) so `/liquidaddon ai` keeps saying the same
thing. Until that exists, the `Engine = Math` branch (Profile A, the curve) is the one to port: it is
self-contained, and the policy files already encode how much of a model to trust once there is one.

## How to verify

1. `cargo test --workspace` here — 67 tests, no game needed.
2. In a checkout of the pinned Horizon: build a bundle with horizon-mixins,
   `horizon-mixins plan --workspace <horizon> --mods <bundle> --out plan.json`, then
   `check … --trust-native-mods` to compile the staged workspace and run its tests. `apply` never
   touches the original checkout.
3. In-game: one module at a time, with the launcher's packet recording on, so the steps that leave
   are visible as packets rather than as intentions.

## Open decisions

- **Visible or silent rotation.** Start visible (see "The rotation problem"); silent only with a
  proven ordering and a recording to back it.
- **One bundle or several.** One `liquidaddon` bundle with API-gated injections is simpler to pin;
  a `liquidaddon-aim` / `liquidaddon-combat` split lets a player take the harmless half.
- **1.8-style clicks against 26.3 rules.** `DoubleClicks`/`TriggerBotPlus` exist to beat a cooldown
  that 26.3 servers police by CPS and rotation checks; the port should keep the original's caps and
  say so in its own docs, not silently raise them.
