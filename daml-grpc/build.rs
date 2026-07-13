use itertools::Itertools;
use std::error;
use std::fs;
use std::io::Error;
use std::path::Path;
use std::path::PathBuf;
#[allow(unused_imports)]
use tonic_prost_build as _;

const ALL_PROTO_SRC_PATHS: &[&str] = &[
    "com/daml/ledger/api/v2",
    "com/daml/ledger/api/v2/admin",
    "com/daml/ledger/api/v2/testing",
    "com/daml/ledger/api/v2/interactive",
    "com/daml/ledger/api/v2/interactive/transaction/v1",
    "google/protobuf",
    "google/rpc",
];
const PROTO_ROOT_PATH: &str = "resources/protobuf";

fn main() -> Result<(), Box<dyn error::Error>> {
    let root = Path::new(PROTO_ROOT_PATH);
    let all_protos = get_all_protos(root, ALL_PROTO_SRC_PATHS)?;
    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(all_protos.as_slice(), &[root.to_path_buf()])?;
    Ok(())
}

fn get_all_protos(root: &Path, src_paths: &[&str]) -> Result<Vec<PathBuf>, Error> {
    src_paths.iter().map(|s| get_protos_from_dir(root, Path::new(s))).fold_ok(vec![], |mut acc: Vec<PathBuf>, v| {
        acc.extend(v);
        acc
    })
}

fn get_protos_from_dir(root: &Path, dir: &Path) -> Result<Vec<PathBuf>, Error> {
    fs::read_dir(root.join(dir))?
        .filter_map(|entry| match entry {
            Ok(d) => match d.path().extension() {
                Some(a) if a == "proto" => Some(Ok(d.path())),
                _ => None,
            },
            Err(e) => Some(Err(e)),
        })
        .collect()
}
