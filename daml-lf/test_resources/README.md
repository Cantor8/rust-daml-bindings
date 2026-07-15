# `daml-lf` LF2 test fixtures

The canonical LF2 DAR sits next to this README:

```
TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar
```

It is consumed by `tests/integration_tests.rs` (and downstream
codegen / derive tests once they migrate). The `.daml` sources
that produced it live under [`src/`](src) — see
[`src/daml.yaml`](src/daml.yaml) for the project config and
[`src/daml/Fuji/`](src/daml/Fuji) for the modules.

## Rebuilding the fixture

The fixture is checked in as a binary because `dpm build` needs
the Daml SDK (not the LF2-only `daml-lf` crate). To regenerate
after editing the `.daml` sources:

```sh
# From inside the nix dev shell (dpm is on PATH). The first
# `dpm build` downloads SDK 3.4.11 — the sdk-version pinned in
# daml.yaml — into ~/.dpm; that step needs network access.
cd daml-lf/test_resources/src
dpm build
cp .daml/dist/TestingTypes-3.0.0.dar \
   ../TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar
rm -rf .daml
```

The SDK version comes from `daml.yaml`'s `sdk-version` field. The
fixture targets LF 2.1 via the `--target=2.1` build option (LF
2.dev features land in `daml-lf` later as needed).

## Coverage

The fixture is deliberately small but exercises every shape the
LF2 conversion layer needs to handle:

| Shape                    | Module        | Notes                                |
|--------------------------|---------------|--------------------------------------|
| Record                   | Fuji.Types    | `Profile`                            |
| Variant                  | Fuji.Types    | `Shape`                              |
| Enum                     | Fuji.Types    | `Color`                              |
| Nested record / variant  | Fuji.Types    | `Painted`                            |
| Template + choice        | Fuji.Asset    | `Asset` + consuming `Transfer`       |
| Interface + view + choice| Fuji.Asset    | `Holding` + `HoldingView` + `Reassign` |
| Interface instance       | Fuji.Asset    | `Asset` implements `Holding`         |

Contract keys are intentionally **not** exercised — LF 2.1
dropped key support, and adding a key triggers a
`Contract keys` compiler error.

## Why not generate at build time?

Two reasons:

1. `dpm build` needs the Daml SDK (the repo's `flake.nix`
   provides `dpm`, which fetches the SDK on first use), but most
   CI runs only depend on the Nix-pinned
   Rust toolchain. Checking the DAR in keeps `cargo test` working
   without spinning up the JVM.
2. The fixture changes rarely; build-time regeneration would make
   tests slow without buying determinism.
