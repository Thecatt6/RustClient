# LiquidAddon — Horizon port (Rust)

A separate Rust port of [Thecatt6/LiquidAddon](https://github.com/Thecatt6/LiquidAddon) for
[Horizon](https://github.com/horizon-rust/horizon), the Minecraft: Java Edition 26.3 client written
from scratch in Rust. The original Kotlin/Fabric add-on is left untouched and remains the behavior
reference. This repository is GPL-3.0-only.

This is a staged port, not a translation of all of LiquidBounce. The practical scope is the 13
modules and shared AI/settings/commands used by the pinned LiquidAddon revision; unrelated
LiquidBounce modules are out of scope. See [`PORTING.md`](PORTING.md) for the hook map and risks.

## Current status

**Work in progress; no installable or in-game-validated `.hnmod` yet.** The active slice is the
client-free **Math branch of `AimEngine.kt`**:

- `mixins/liquidaddon/core` contains the Rust workspace crate: rotation/vector maths, AABB and
  crosshair geometry, target-point selection, held/clamped aim offset, prediction, bubble weight,
  the stateful Math curve, GCD quantisation, hand layer and cycle guard;
- `mixins/liquidaddon/horizon-mixin.toml` is a Horizon bundle skeleton. It registers LiquidAddon
  on Options > Mods and adds a **Rotation mode: Visible/Silent** setting button. Clicks persist to
  `horizon-mods/.horizon-mod-data/liquidaddon/aim_rotation_mode`; fresh or unknown values default to `Visible`;
- the mode is a runtime `AimConfig` value, not a compile-time switch. The settings UI and persistence
  are implemented, but the gameplay aim/controller hook is not: until that adapter consumes the
  stored mode, toggling the preference does not change player or packet rotations. Silent rotation
  is not claimed to work.

The source tree currently contains **98 Rust unit tests**: 67 earlier client-free port cases and 31
new tests for the Math/geometry/settings slice. **They have not been compiled or run in this
sandbox** because `cargo`, `rustc` and `rustfmt` are unavailable here. Run them with the pinned
Rust toolchain before treating the slice as verified.

The crate lives inside the bundle on purpose: `horizon-mixins` only stages bundle-relative crates,
and rejects crate paths that escape the bundle. The root Cargo workspace still runs the core tests
without requiring a game.

## Layout

```
Cargo.toml                         root workspace and minimum Rust version
rust-toolchain.toml                exact compiler/tooling pin (1.95.0)
mixins/liquidaddon/
  horizon-mixin.toml               pinned native bundle; mod/settings hooks
  register.rs, settings*.rs        Horizon Options > Mods integration
  core/                             game-independent Rust core and unit tests
    src/aim/                        AI policy, Math engine, geometry, rotations, settings
    src/combat/                     double-click scheduler
    src/hud/                        jump-reset HUD tag
    src/rng.rs                      deterministic SplitMix64
compatibility.json                 exact source/target/API pins
PORTING.md                        hook map, per-module plan and known incompatibilities
```

## Rotation setting

Open **Options > Mods > LiquidAddon** and click **Rotation mode: Visible** or **Rotation mode:
Silent**. The native settings hook stores the selected tag using Horizon's persistent setting helper
under the safe one-file key `aim_rotation_mode`. Visible is the default and the fail-open value for
unrecognized tags. The core exposes `RotationMode` through `AimConfig` at runtime; the remaining
rotation adapter must still apply that choice to a real gameplay path.

## Tests and checks

From the repository root, with Rust 1.95.0 installed:

```sh
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets
```

For the bundle, use the pinned Horizon checkout and its `horizon-mixins` revision:

```sh
horizon-mixins plan --workspace /path/to/horizon --mods mixins --out plan.json
horizon-mixins check --workspace /path/to/horizon --mods mixins \
  --out target/liquidaddon-check --trust-native-mods
```

A successful plan/check is still not an in-game test. In particular, visible rotation needs a real
rotation application hook, and silent rotation needs packet recording to prove that the camera and
server-facing rotation stay correctly separated.

## Pins and scope

[`compatibility.json`](compatibility.json) pins Horizon `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca`,
`horizon-mixins` `5a64af938ec86b00d53d16a408aabc0038fb3e57`, Minecraft 26.3, Rust 1.95.0, and the
LiquidAddon source commit `c128aaf0ffb98a815661d24381d1f548508e0b80`.

Still to do: compile and verify this slice; connect the Math engine to Horizon target/player state
and rotation hooks; then port the inference/training/model `.params` branch, remaining shared AI,
the 13 module behaviors, settings/commands, and their tests. The user-facing Visible/Silent choice
must remain persistent and GUI-editable throughout that integration.

Before using any feature, check the rules of the server you play on. Horizon's native mods are
native code with the player's rights and are not sandboxed; this project does not make a server allow
anything its rules prohibit.
