# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-06-01

A clean break to **Canton 3.5.1 / Ledger API v2 / Daml-LF 2.x**.
Every crate, surface, and feature flag in this release is wire-
and API-incompatible with the 0.2.x line.

### Added

- `daml-grpc` rewritten around the v2 Ledger API services:
  `CommandService` (with `SubmitAndWait` /
  `SubmitAndWaitForTransaction` /
  `SubmitAndWaitForReassignment`), `UpdateService`,
  `StateService`, `EventQueryService`, `ContractService`,
  `InteractiveSubmissionService`, `CommandInspectionService`,
  `IdentityProviderConfigService`, plus the upgraded
  `VersionService` / `PackageService` / `PackageManagementService`
  / `PartyManagementService` / `UserManagementService` /
  `ParticipantPruningService` / `TimeService`. The `Update`
  envelope (Transaction / Reassignment / TopologyTransaction)
  replaces v1's `Transaction` / `TransactionTree` pair.
- `daml-lf` switched to **Daml-LF 2.x**: `daml_lf_2::Package`,
  the v2 expression / Update sub-oneofs (with `DamlInterfaceExpr`
  covering the 13 LF2 interface ops), `DamlInterface` +
  `DamlException` + `DamlInterfaceMethod` on the element layer,
  and `DamlType::FailureCategory` for the new LF2 type-level
  builtin.
- `daml-codegen` / `daml-derive`: package-name-addressed
  `template_id()` (the v2 convention), interface trait emission,
  `impl <Interface> for <TemplateContractId> {}` for every
  declared interface, `<iface>_<choice>_command(...)` exercise
  methods on contract ids, new `#[DamlInterface]` attribute
  macro, and `#[DamlTemplate]`'s `package_name = "..."` knob.
- Nix flake provisions Canton 3.5.1 + Daml SDK 3.4.11 + a
  `canton-sandbox` wrapper script. Integration smoke test
  exercises Create + exercise-via-interface end-to-end against
  the local sandbox.

### Changed

- **Ledger API v2 is the only supported protocol.** v1 services
  / message shapes have no compatibility shim; clients targeting
  Daml Connect 1.x must stay on the 0.2.x line.
- The `daml-grpc` `DamlIdentifier` wraps a `package_ref` field
  that accepts either a package-id hash or a `#<package-name>`
  marker (the v2 wire convention for templates and interfaces).
- `DamlSimpleExecutor`'s `execute_for_transaction_tree` is
  renamed `execute_for_transaction_with_effects`; the
  ledger-effects view is now selected via a `TransactionFormat`
  rather than a dedicated `TransactionTree` RPC.
- MSRV bumped to **Rust 1.75** (required by the modern
  `tonic` / `prost` pulled in for v2 protos).

### Removed

- `daml-json` and `daml-bridge` crates deleted -- the v1 Daml
  JSON API is gone, replaced by a JSON Ledger API auto-generated
  from the v2 protos. Pure-Rust JSON bridging is out of scope.
- `examples/daml-oas` deleted (depended on `daml-json`).
- `daml` umbrella crate's `json`, `bridge`, and `sandbox`
  feature flags removed; `daml-grpc`'s `sandbox` feature
  removed. `full` no longer pulls JSON.
- `daml-util/src/sandbox_auth.rs` deleted (the v1-shaped JWT
  token builder; the Canton v2 claim shape is different and a
  v2 token helper is a planned follow-up).
- Daml-LF 1.x support (DARs at LF 1.6 / 1.7 / 1.8 / 1.14 no
  longer load).
- The pre-existing v1 integration test trees under
  `daml-grpc/tests/{grpc,common}/` and `daml-derive/tests/`
  were dropped wholesale; only `daml-grpc/tests/integration.rs`
  (gated by `--features integration`) is the live-sandbox
  surface in 0.3.0.

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
