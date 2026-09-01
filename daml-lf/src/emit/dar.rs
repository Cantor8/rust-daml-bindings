//! Packaging a built archive as a dar.
//!
//! A participant is given packages as dar files: a zip holding the archives
//! and a manifest naming the main one.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::error::DamlLfResult;

use super::build::{build_payload, encode_archive};
use super::schema;

const MANIFEST_PATH: &str = "META-INF/MANIFEST.MF";

/// Build a dar holding the package described by `package`.
///
/// The bytes are ready to hand to a participant, or to write to a `.dar` file.
pub fn build_dar(package: &schema::Package) -> DamlLfResult<Vec<u8>> {
    let (payload_bytes, hash) = build_payload(package)?;
    let archive_bytes = encode_archive(payload_bytes, &hash);
    // Dalf names carry the package id, so two versions of a package can sit
    // side by side in the same dar.
    let dalf_name = format!("{}-{hash}.dalf", package.name);

    let mut dar = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();

    dar.start_file(MANIFEST_PATH, options)?;
    dar.write_all(manifest(&dalf_name).as_bytes())?;
    dar.start_file(&dalf_name, options)?;
    dar.write_all(&archive_bytes)?;

    Ok(dar.finish()?.into_inner())
}

fn manifest(dalf_name: &str) -> String {
    format!(
        "Manifest-Version: 1.0\n\
         Created-By: daml-lf\n\
         Main-Dalf: {dalf_name}\n\
         Dalfs: {dalf_name}\n\
         Format: daml-lf\n\
         Encryption: non-encrypted\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::schema::{Field, FieldType, Module, Package, Template};
    use crate::DarFile;

    fn package() -> Package {
        Package {
            name: "RoadrunnerExample".to_owned(),
            version: "1.0.0".to_owned(),
            modules: vec![Module {
                data_types: Vec::new(),
                interfaces: Vec::new(),
                name: "Example.Iou".to_owned(),
                templates: vec![Template {
                    key: None,
                    name: "Iou".to_owned(),
                    fields: vec![
                        Field::new("issuer", FieldType::Party),
                        Field::new("amount", FieldType::Int64),
                    ],
                    signatories: vec!["issuer".to_owned()],
                    observers: Vec::new(),
                    choices: Vec::new(),
                }],
            }],
        }
    }

    #[test]
    fn a_built_dar_reads_back() {
        let bytes = build_dar(&package()).expect("builds");

        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(&bytes).expect("written");
        let dar = DarFile::from_file(file.path()).expect("reads back");

        let (name, package_id, templates) = dar
            .apply(|archive| {
                let package = archive.main_package().expect("a main package");
                let templates: Vec<String> = package
                    .root_module()
                    .child_modules()
                    .flat_map(|module| module.child_modules())
                    .flat_map(|module| module.data_types())
                    .filter_map(|data| match data {
                        crate::element::DamlData::Template(template) =>
                            Some(template.name().to_owned()),
                        _ => None,
                    })
                    .collect();
                (
                    package.name().to_owned(),
                    package.package_id().to_owned(),
                    templates,
                )
            })
            .expect("applies");
        assert_eq!(name, "RoadrunnerExample");
        assert_eq!(package_id.len(), 64);
        assert_eq!(templates, vec!["Iou".to_owned()]);
    }
}
