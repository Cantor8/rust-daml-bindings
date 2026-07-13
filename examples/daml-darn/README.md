[![Documentation](https://docs.rs/daml-darn/badge.svg)](https://docs.rs/daml-darn/0.3.0)
[![Crate](https://img.shields.io/crates/v/daml-darn.svg)](https://crates.io/crates/daml-darn/0.3.0)
![maintenance-status](https://img.shields.io/badge/maintenance-experimental-blue.svg)

# Darn

Tools for working with Daml Archives.

## Install

```shell
cargo install daml-darn
```

## Usage

```shell
Tools for working with Daml Archives and ledgers

Usage: daml-darn <COMMAND>

Commands:
  package  Show DAR package details
  intern   Show interned strings and dotted names in a DAR
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Package Usage

```shell
Show DAR package details

Usage: daml-darn package <DAR>

Arguments:
  <DAR>  Path to the DAR file

Options:
  -h, --help  Print help
```

### Intern Usage

```shell
Show interned strings and dotted names in a DAR

Usage: daml-darn intern [OPTIONS] <DAR>

Arguments:
  <DAR>  Path to the DAR file

Options:
  -s, --string          Show interned strings
  -d, --dotted          Show interned dotted names
  -i, --index <INDEX>   Restrict output to these intern indices (comma-separated)
  -f, --show-mangled    Include names that start with `$` (compiler-mangled)
      --order-by-index  Sort output by intern index
      --order-by-name   Sort output by rendered name (default)
  -h, --help            Print help
```

## Examples

### List packages

```shell
daml-darn package TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar
```

Outputs (abridged; the main package is highlighted green in the terminal):

```
+---------------------------------+---------+------------------------------------------------------------------+------+
| name                            | version | package_id                                                       | lf   |
+---------------------------------+---------+------------------------------------------------------------------+------+
| daml-prim-DA-Internal-Erased    | 1.0.0   | 0e4a572ab1fb94744abb02243a6bbed6c78fc6e3c8d3f60c655f057692a62816 | v2.1 |
| TestingTypes                    | 3.0.0   | 0fabbe7b63f2c6a9b453e027757d61de625aafa01a4a8b4bb122e9b8481dfa00 | v2.1 |
| daml-stdlib                     | 3.4.11  | 3b25c9b08ac6d895417c604fc0ee4b7e47ef974ff8fa43f139daa43bb431fefc | v2.1 |
| ...                             |         |                                                                  |      |
+---------------------------------+---------+------------------------------------------------------------------+------+
```

### Show interned dotted names

```shell
daml-darn intern -d TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar
```

Outputs (abridged):

```
+-------+-------------+---------------------+
| index | rendered    | segments            |
+-------+-------------+---------------------+
| 0     | Fuji.Asset  | Fuji(0).Asset(1)    |
| 248   | Fuji.Types  | Fuji(0).Types(376)  |
+-------+-------------+---------------------+
```

## License

`daml-darn` is distributed under the terms of the Apache License (Version 2.0).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in time by you, as defined
in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

See [LICENSE](LICENSE) for details.

Copyright 2022-2026
