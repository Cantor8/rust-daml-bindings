![ci](https://github.com/fujiapple852/rust-daml-bindings/actions/workflows/ci.yml/badge.svg)
[![Documentation](https://docs.rs/daml/badge.svg)](https://docs.rs/daml/0.3.0)
[![Crate](https://img.shields.io/crates/v/daml.svg)](https://crates.io/crates/daml/0.3.0)
![maintenance-status](https://img.shields.io/badge/maintenance-experimental-blue.svg)

# Rust Bindings for Daml

Unofficial Rust bindings and tools for [Daml](https://daml.com).

## Crates

The project provides the following crates:

| crate                                                       | description                                        |
|-------------------------------------------------------------|----------------------------------------------------|
| [daml](https://crates.io/crates/daml/0.3.0)                 | Daml prelude & common entry point                  |
| [daml-grpc](https://crates.io/crates/daml-grpc/0.3.0)       | Daml Ledger gRPC API (v2) bindings                 |
| [daml-codegen](https://crates.io/crates/daml-codegen/0.3.0) | Generate Rust gRPC API bindings from Daml archives |
| [daml-derive](https://crates.io/crates/daml-derive/0.3.0)   | Macros for generating Rust gRPC bindings from Daml |
| [daml-macro](https://crates.io/crates/daml-macro/0.3.0)     | Helper macros for working with Daml gRPC values    |
| [daml-util](https://crates.io/crates/daml-util/0.3.0)       | Utilities for working with Daml ledgers            |
| [daml-lf](https://crates.io/crates/daml-lf/0.3.0)           | Library for working with Daml-LF 2.x archives      |

## Usage

Applications should always depend on the `daml` crate directly and specify the appropriate features to enable the
required functionality:

```toml
[dependencies]
daml = { version = "0.3.0", features = [ "full" ] }
```

See the [documentation](https://docs.rs/daml/0.3.0) for the full set of feature flags available.

## Example Applications

Several example applications are available in
the [`examples`](https://github.com/fujiapple852/rust-daml-bindings/tree/master/examples) directory showcasing various
features of the library. Additionally, most crates provide comprehensive integration tests which demonstrate usage.

## Integration tests

The default `cargo test` only runs unit tests and the lightweight DAR-loading
integration tests under `daml-lf`. The live-sandbox smoke test (`daml-grpc`'s
end-to-end create + exercise-via-interface against a real Canton sandbox)
is gated behind an opt-in feature.

To run it, start the sandbox in one terminal and the test in another, both
from inside the Nix dev shell:

```sh
# terminal 1 — boots Canton 3.5.1 with the topology in canton/sandbox.*
nix develop --command canton-sandbox

# terminal 2 — runs the smoke test against localhost:5011
nix develop --command cargo test -p daml-grpc --features integration --test integration -- --nocapture
```

The Phase 7 LF2 fixture DAR
(`daml-lf/test_resources/TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar`) is
uploaded by the test itself; no manual `daml ledger upload-dar` is needed.

## Minimum Supported Rust Version

The current MSRV is **1.75** (required by the modern `tonic` / `prost`
dependencies pulled in for the v2 protos).

## Supported Daml Version

This library targets the **v2 Ledger API**, exposed by **Canton 3.5.1**, with
DARs compiled to **Daml-LF 2.x** by the **Daml SDK 3.4.x** toolchain. Both
Canton and the SDK are provisioned via the project's `flake.nix`; see the
integration-test workflow above.

## Changelog

Please see the [CHANGELOG](https://github.com/fujiapple852/rust-daml-bindings/blob/master/CHANGELOG.md) for a release
history.

## License

This library is distributed under the terms of the Apache License (Version 2.0).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in time by you, as defined
in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

See [LICENSE](LICENSE) for details.

Copyright 2022