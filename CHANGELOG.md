# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0]

A clean break to **Canton 3.5.1 / Ledger API v2 / Daml-LF 2.x**.
Wire- and API-incompatible with the 0.2.x line.

### Added

- v2 gRPC service surface: Command / Update / State /
  EventQuery / Contract / CommandInspection /
  IdentityProviderConfig / Version / Package(Management) /
  Party(Management) / User(Management) / ParticipantPruning /
  Time. `Update` envelope (Transaction / Reassignment /
  TopologyTransaction) replaces v1's Transaction /
  TransactionTree pair.
- `daml-lf` targets **Daml-LF 2.x** end-to-end: interfaces,
  exceptions, `FailureCategory`, and the 13 LF2 interface
  update ops.
- Codegen / derive: package-name-addressed `template_id()`,
  interface marker traits with `impl <Interface> for
  <ContractId> {}`, `<iface>_<choice>_command(...)` methods,
  `#[DamlInterface]` attribute, `#[DamlTemplate(package_name)]`.
- `daml-util`: Canton v2 JWT builder (`DamlCantonTokenBuilder`,
  HS256 / RS256 / ES256), plus `DamlPackages::into_dar` with
  optional main-package selection and LF2-tree-walking
  dependency filter.
- `DamlArchive::validate()` catches malformed archives at load
  time; `DamlLfConvertError::UnsupportedFeatureUsed` fires when
  an archive uses an LF2 feature the convert layer hasn't
  opted into.
- Nix flake provisions Canton 3.5.1 + Daml SDK 3.4.11 + a
  `canton-sandbox` wrapper; integration smoke test exercises
  Create + interface-addressed Exercise end-to-end.

### Changed

- Ledger API v2 only — no v1 compat shim.
- Deserialize now *moves* fields via `DamlRecord::take_field`
  instead of `.field(name)?.to_owned()`; large-payload records
  no longer pay an O(payload) clone per field on `try_into()`.
- `DamlError` derives `thiserror::Error` and forwards
  `source()` for wrapping variants (tonic status / transport /
  IO / URI / timeout).
- `DamlValue::partial_cmp` is total across variants; `impl Ord
  for DamlValue` no longer panics on cross-variant compares
  (fixed a `BTreeMap<DamlValue, _>` foot-gun for `GenMap`).
- `DamlValue::Numeric` wire encoding preserves the natural
  stored scale (was padding every value to 37 decimals /
  flipping small magnitudes to scientific notation).
- `DamlVariant` now preserves `variant_id` on the wire
  round-trip; `DamlStatus` surfaces the full `details` chain;
  `DamlSynchronizerTime` fails on missing `record_time`
  instead of defaulting to the Unix epoch.
- `DamlPackages::into_dar` signature is now
  `(main_id, filter_deps, style)`.
- Naming polish across the public surface:
  `ExerciseByKeyCommand` variant → `ExerciseByKey`;
  `DamlSimpleExecutorBuilder::application_id` → `user_id`;
  version service returns a named `DamlLedgerApiVersion`
  struct rather than a tuple.
- `[workspace.dependencies]` centralises the 12 crates used by
  ≥ 2 members; edition 2024, MSRV 1.96.

### Removed

- `daml-json`, `daml-bridge` crates and `examples/daml-oas`
  (JSON is now Canton's own JSON Ledger API).
- v1-shaped auth helper (`daml-util/src/sandbox_auth.rs`);
  replaced by `canton_auth.rs`.
- Daml-LF 1.x archive support.
- v1 integration test trees (`daml-grpc/tests/{grpc,common}/`,
  `daml-derive/tests/`); the live-sandbox surface is
  `daml-grpc/tests/integration.rs` under `--features
  integration`.
- Dead code caught during review: `Executor` trait
  (`daml-grpc`), `DamlCantonTokenError::Expiry`,
  `TrySwapRemove` helper, plus scattered dead bindings and
  redundant `map_err` calls.

### Fixed

- `DamlLfHashFunction::Sha256` is now honoured when reading a
  DAR manifest (was previously ignored).
- LF2 feature flags no longer silently default to optimistic
  values — `UnsupportedFeatureUsed` fires when a payload uses
  something the convert layer hasn't opted into.
- `DamlTextMap` / `GenMap` `PartialOrd` / `PartialEq` walk
  `(key, value)` pairs — previously key-only, so maps that
  differed in values compared equal.
- Codegen's identifier sanitiser treats `gen` as reserved (Rust
  2024) and prepends `_` when a name starts with a digit,
  closing a `Ident::new` panic path on odd package / archive
  names.
- `daml-derive`'s `syn::Path` splitter only strips a leading
  `crate::` segment (was always dropping the first segment,
  silently losing user-written `foo::bar::MyType` paths).

## [0.2.2] - 2022-03-08

### Changed

- Updated documentation for all crates

## [0.2.1] - 2022-03-07

### Changed

- Updated documentation for all crates

## [0.2.0] - 2022-03-04

### Added

- Published `daml-oas` crate
- Published `daml-darn` crate

### Changed

- Improved documentation for all crates
- replace `Vec<DamlValue>` with alias `DamlList<DamlValue>` in `DamlValue::List`

## Fixed

- Fixed many broken doc-links across all crates

## [0.1.1] - 2022-03-03

### Changed

- Fixed broken documentation links and added many missing doc-comments

## [0.1.0] - 2022-03-01

### Added

- Initial release of `rust-daml-bindings`

[0.2.2]: https://github.com/fujiapple852/rust-daml-bindings/compare/0.2.1...0.2.2

[0.2.1]: https://github.com/fujiapple852/rust-daml-bindings/compare/0.2.0...0.2.1

[0.2.0]: https://github.com/fujiapple852/rust-daml-bindings/compare/0.1.1...0.2.0

[0.1.1]: https://github.com/fujiapple852/rust-daml-bindings/compare/0.1.0...0.1.1

[0.1.0]: https://github.com/fujiapple852/rust-daml-bindings/compare/0.0.0...0.1.0
