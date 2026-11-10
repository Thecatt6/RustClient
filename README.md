# LiquidAddon — Horizon port (Rust)

A separate Rust rewrite of [Thecatt6/LiquidAddon](https://github.com/Thecatt6/LiquidAddon) for
[Horizon](https://github.com/horizon-rust/horizon), the Minecraft: Java Edition 26.3 client written
from scratch in Rust. The original Fabric add-on is left untouched and remains the reference.

Rust and Kotlin do not share a runtime, a `RotationManager` or a `ChatFormatting`, so this is not a
transliteration of 12 000 lines: it is a port in two halves.

| Half | Lives in | Holds |
|---|---|---|
| **Core** | `crates/liquidaddon-core` | the decisions, with the Kotlin tests they came with: when a follow-up click may fire, how much of an AI answer to trust, when a sequence of mouse steps has become a pattern, what the HUD says |
| **Bundle** | `mixins/` (not written yet) | the numbers and the effects: read the player's rotation and target, then act through Horizon's hooks or through source mixins |

The split follows the source: half of `LiquidAddon`'s aiming package is already client-free Kotlin
with its own test file, and those files translate one-to-one. The rest is LiquidBounce plumbing, and
that has to be rebuilt against Horizon's game state rather than translated.

## Status

**The core is ported and tested; there is no installable `.hnmod` yet.** A native mod on Horizon is
a `horizon-mixin.toml` plus helper `.rs` files that
[horizon-mixins](https://github.com/horizon-rust/horizon-mixins) compiles into the game's own source,
pinned to one Horizon commit — that bundle is the next step, and [`PORTING.md`](PORTING.md) maps
every module to the hooks it needs.

Ported so far, 67 tests, all translated from the Kotlin suite of the same name:

| Core module | Ported from | Tests |
|---|---|---|
| `combat::double_clicks` | `modules/doubleclicks/DoubleClickScheduler.kt` | 10 |
| `aim::hand_gate` | `aiming/HandGate.kt` | 12 |
| `aim::mouse_pattern_guard` | `aiming/MousePatternGuard.kt` | 7 |
| `aim::ai_stats` | `aiming/AiStats.kt` | 10 |
| `aim::ai_reach` | `aiming/AiReach.kt` | 7 |
| `aim::ai_drift` | `aiming/AiDrift.kt` | 4 |
| `aim::ai_features` | `aiming/AiFeatures.kt` | 5 |
| `aim::ai_hand` | `aiming/AiHand.kt` | 3 |
| `hud::jump_reset` | `modules/jumpreset/JumpResetHud.kt` | 7 |
| `rng` | (new: a seeded `SplitMix64` for the two places Kotlin took a `Random`) | 2 |

Still to port: `AimEngine.kt` (2 620 lines — the rotation generator itself), the 13 module files,
the settings/commands, and the model/telemetry plumbing. `PORTING.md` has the order and the reason
for it.

## Layout

```
Cargo.toml                     workspace, no dependencies anywhere
crates/liquidaddon-core/       game-independent logic, `cargo test` runs it
  src/aim/                     AiDrift, AiFeatures, AiHand, AiReach, AiStats, HandGate, MousePatternGuard
  src/combat/                  DoubleClickScheduler
  src/hud/                     JumpReset HUD tag
  src/rng.rs                   seeded SplitMix64
mixins/                        the Horizon bundle (horizon-mixin.toml + helpers) — to be written
compatibility.json             the exact revisions this port targets
PORTING.md                     hook map, per-module plan, known incompatibilities
```

## Running the tests

```sh
cargo test --workspace          # 67 tests, no game, no network, no dependencies
cargo fmt --all --check
cargo clippy --workspace --all-targets
```

The crate is `std`-only and dependency-free on purpose: it has to stay testable here, and the
eventual bundle should not add a Cargo graph to every player's build. Rust **1.95.0** is pinned
(`rust-version`), the same toolchain horizon-mixins and Horizon use.

## Pins

[`compatibility.json`](compatibility.json):

- Horizon `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca`, with its default `horizon-mixins`
  `5a64af938ec86b00d53d16a408aabc0038fb3e57`;
- source `Thecatt6/LiquidAddon` `c128aaf0ffb98a815661d24381d1f548508e0b80`, GPL-3.0-only, the
  licence this port keeps.

## Before you install it anywhere

Horizon ships no auto-clicking, no aim assist and no wall-hack of its own, and its
[`pvp.md`](https://github.com/horizon-rust/horizon/blob/main/docs/pvp.md) says why: those features
would stop a fresh install from being welcome on ordinary servers. A native mod can still do all of
it — it is native code with the player's rights, always on, and not sandboxed — which is exactly why
the launcher shows a trust notice and the registry reviews a mod before listing it. Nothing in this
repository changes what a server allows: check the rules of the server you play on before you run
any of this, and keep the modules off anywhere they are not permitted.

## Port order

1. **Ported:** the client-free Kotlin — schedulers, gates, formatters, the AI policy around the
   model.
2. Settings/state on the Horizon side, plus the typed `Host` helpers the bundle needs (`player`,
   `entity_*`, `raycast`, `view_hit`, `held`).
3. The rotation path — the one thing Horizon does not expose as a hook (see
   [`PORTING.md`](PORTING.md#the-rotation-problem)) — then `AimEngine`, `Aimassist`, `AimbotAI`.
4. The combat and projectile modules, in the groups `PORTING.md` lists, each with the vanilla path
   it replaces preserved where the source does.
5. Package a `.hnmod` against the pins, build it with `horizon-mixins`, and test it in the matching
   launcher build.
