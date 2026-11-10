# LiquidAddon — Horizon port

A separate Rust rewrite of [Thecatt6/LiquidAddon](https://github.com/Thecatt6/LiquidAddon) for Horizon. The original LiquidBounce add-on is left untouched.

## Status

This is the **port foundation**, not an installable `.hnmod` yet. The first game-independent component, the bounded `DoubleClickScheduler`, has been translated with its Kotlin behavior and regression cases. The Horizon hook adapter and gameplay modules are still to be ported.

The source add-on currently contains these 13 modules:

- Aimassist, AimbotAI
- AutoPearlAI, FireballDeflect, FireballJumpAI
- BlockHit, DoubleClicks, HitFlick, HitSelect, TriggerBotPlus
- FakeLagV2
- JumpReset, ScaffoldLegit

The shared aim/AI implementation, training/telemetry helpers, settings, commands and supporting tests are part of the port scope too.

## Pinned target

The compatibility pins are in [`compatibility.json`](compatibility.json):

- Horizon: `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca` (the `fdc87ae` build you supplied).
- `horizon-mixins`: `5a64af938ec86b00d53d16a408aabc0038fb3e57`, the default revision declared by that Horizon commit.
- Game: Minecraft Java 26.3.

Native Horizon mods are Rust source-mixin bundles, not Fabric JARs. This project will use the GPL-3.0-only license of the original LiquidAddon.

## Current Rust core

The initial port is in `mixins/liquidaddon/runtime.rs`. It contains the scheduler, the attack/use safety decisions, and tests corresponding to `DoubleClickSchedulerTest.kt`.

Run the tests when Rust is installed:

```sh
cargo test --manifest-path horizon-liquidaddon/Cargo.toml
```

## Port order

1. Port and test game-independent logic.
2. Add the Horizon-native settings/state and source-hook adapter.
3. Port the modules in functional groups, preserving vanilla action paths where the original does so.
4. Package and validate `.hnmod` against the pinned Horizon and `horizon-mixins` revisions; then test in the matching launcher build.

See [`PORTING.md`](PORTING.md) for the hook map and known compatibility work.