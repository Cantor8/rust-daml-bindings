[![Documentation](https://docs.rs/daml-codegen/badge.svg)](https://docs.rs/daml-codegen/0.3.0)
[![Crate](https://img.shields.io/crates/v/daml-codegen.svg)](https://crates.io/crates/daml-codegen/0.3.0)
![maintenance-status](https://img.shields.io/badge/maintenance-experimental-blue.svg)

# Daml Codegen

A library and a tool for generating Rust types from `daml` code.

This crate provides:

- A code generator backend for the of custom attributes and procedural macros defined in
  the [`daml-derive`](https://docs.rs/daml-derive/0.3.0/daml_derive/) crate
- A [`daml_codegen`](https://docs.rs/daml-codegen/0.3.0/daml_codegen/generator/fn.daml_codegen.html) function which is
  designed to be used from `build.rs` files
- A standalone codegen cli tool

## Library

This crate should not be used directly when used as a library, instead you should depend on
the [`daml`](https://crates.io/crates/daml/0.3.0) crate and enable the `codegen` feature:

```toml
[dependencies]
daml = { version = "0.3.0", features = [ "codegen" ] }
```

## Install

```shell
cargo install daml-codegen
```

## Usage

```shell
Daml GRPC Ledger API Code Generator

Usage: daml-codegen [OPTIONS] <dar>

Arguments:
  <dar>  Path to the input DAR file

Options:
  -o, --output-dir <output>        Output directory (default: '.')
  -f, --module-filter <filter>...  Regex(es) matched against fully-qualified module names
                                   (e.g. 'MyPackage\.Foo\..*'). Only modules matching any
                                   of the regexes are emitted; without --module-filter,
                                   every module is emitted.
  -i, --render-intermediate        Render an intermediate representation instead of the
                                   full Rust types (debugging aid)
  -c, --combine-modules            Emit one Rust file per DAR (default: one file per module)
  -h, --help                       Print help
  -V, --version                    Print version
```

## Example

To generate Rust types from `MyModel.dar` combined into a single output file under
`src/autogen/`:

```shell
daml-codegen MyModel.dar --combine-modules -o src/autogen
```

## License

`daml-codegen` is distributed under the terms of the Apache License (Version 2.0).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in time by you, as defined
in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

See [LICENSE](LICENSE) for details.

Copyright 2022-2026