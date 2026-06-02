#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use daml_lf::element::DamlVisitableElement;
use daml_lf::element::{DamlElementVisitor, DamlEnum};
use daml_lf::DamlLfResult;
use daml_lf::DarFile;
use daml_lf::LanguageVersion;
use daml_lf::{DarEncryptionType, DarManifestFormat, DarManifestVersion};
use std::collections::HashSet;

/// Path to the canonical LF2 fixture DAR. Compiled with Daml SDK
/// 3.4.11 against LF target 2.1; see test_resources/README.md for
/// the rebuild procedure.
const FIXTURE_DAR: &str = "test_resources/TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar";

#[test]
pub fn test_fat_dar_manifest() -> DamlLfResult<()> {
    let dar = DarFile::from_file(FIXTURE_DAR)?;
    assert_eq!(DarManifestVersion::V1, dar.manifest().version());
    assert_eq!("damlc", dar.manifest().created_by());
    assert_eq!("TestingTypes", dar.manifest().dalf_main().split('-').next().unwrap());
    assert_eq!(DarManifestFormat::DamlLf, dar.manifest().format());
    assert_eq!(DarEncryptionType::NotEncrypted, dar.manifest().encryption());
    assert_eq!(LanguageVersion::V2_1, *dar.main().payload().language_version());
    Ok(())
}

#[test]
pub fn test_contains_modules() -> DamlLfResult<()> {
    let dar = DarFile::from_file(FIXTURE_DAR)?;
    assert!(dar.main().payload().contains_module("Fuji.Types"));
    assert!(dar.main().payload().contains_module("Fuji.Asset"));
    Ok(())
}

#[test]
fn test_apply_dar() -> DamlLfResult<()> {
    let dar = DarFile::from_file(FIXTURE_DAR)?;
    let name = dar.apply(|archive| archive.name().to_owned())?;
    assert_eq!("TestingTypes-3.0.0", name);
    Ok(())
}

#[test]
fn test_apply_dalf_dependency() -> DamlLfResult<()> {
    let dar = DarFile::from_file(FIXTURE_DAR)?;
    // LF2 PackageMetadata is mandatory, so every dalf dependency
    // carries its real package name. The first dep on this DAR is
    // a daml-prim sub-module.
    let any_dep = dar.dependencies.first().expect("at least one dalf dependency");
    let name = any_dep.apply(|package| package.name().to_owned())?;
    assert!(name.starts_with("daml-prim"), "dalf name: {name}");
    Ok(())
}

#[test]
fn test_apply_payload() -> DamlLfResult<()> {
    let mut dar = DarFile::from_file(FIXTURE_DAR)?;
    let payload = dar.dependencies.swap_remove(0).payload;
    let name = payload.apply(|package| package.name().to_owned())?;
    // Same observation as `test_apply_dalf_dependency`: LF2's
    // mandatory PackageMetadata propagates through the payload-only
    // apply path too.
    assert!(name.starts_with("daml-prim"), "payload name: {name}");
    Ok(())
}

#[test]
fn test_convert_dar() -> DamlLfResult<()> {
    let dar = DarFile::from_file(FIXTURE_DAR)?;
    let archive = dar.to_owned_archive()?;
    assert_eq!("TestingTypes-3.0.0", archive.name());
    Ok(())
}

#[test]
fn test_visitor_finds_enum() -> DamlLfResult<()> {
    #[derive(Default)]
    pub struct GatherEnumsVisitor(HashSet<String>);
    impl DamlElementVisitor for GatherEnumsVisitor {
        fn pre_visit_enum<'a>(&mut self, data_enum: &'a DamlEnum<'a>) {
            self.0.insert(data_enum.name().to_owned());
        }
    }
    let mut visitor = GatherEnumsVisitor::default();
    let dar = DarFile::from_file(FIXTURE_DAR)?;
    dar.apply(|archive| archive.accept(&mut visitor))?;
    // `Color` is declared in Fuji.Types and re-used as a field on
    // both `Painted` and `Asset` — the visitor sees the enum
    // declaration once regardless.
    assert!(visitor.0.contains("Color"));
    Ok(())
}

#[test]
fn test_cross_package_name_resolution() -> DamlLfResult<()> {
    // Every `DamlTyConName::Absolute` reachable from the converted
    // archive should carry the right `package_name` -- both for
    // self-references (within the fixture's own package) and for
    // cross-package references (into daml-stdlib / daml-prim).
    // Before the archive-level name table landed, only self-refs
    // were resolved.
    use daml_lf::element::{DamlElementVisitor, DamlTyConName};

    #[derive(Default)]
    struct CollectAbs {
        // (package_id, package_name)
        seen: HashSet<(String, String)>,
        // Any tycons where we have a non-empty package_id but
        // empty package_name -- these are the bug.
        unresolved: HashSet<String>,
    }
    impl DamlElementVisitor for CollectAbs {
        fn pre_visit_tycon_name<'a>(&mut self, name: &'a DamlTyConName<'a>) {
            if let DamlTyConName::Absolute(abs) = name {
                self.seen.insert((abs.package_id().to_owned(), abs.package_name().to_owned()));
                if !abs.package_id().is_empty() && abs.package_name().is_empty() {
                    self.unresolved.insert(abs.package_id().to_owned());
                }
            }
        }
    }

    let mut visitor = CollectAbs::default();
    let dar = DarFile::from_file(FIXTURE_DAR)?;
    let loaded_pkg_ids: HashSet<String> = std::iter::once(dar.main.hash.clone())
        .chain(dar.dependencies.iter().map(|d| d.hash.clone()))
        .collect();
    dar.apply(|archive| archive.accept(&mut visitor))?;
    // Unresolved is only a bug if the unresolved package-id is one
    // we actually loaded. References to package-ids outside the
    // archive (e.g. an interned package not shipped with this dar)
    // are still empty by design.
    let real_unresolved: HashSet<&String> =
        visitor.unresolved.iter().filter(|id| loaded_pkg_ids.contains(*id)).collect();
    assert!(
        real_unresolved.is_empty(),
        "package_name unresolved for {} loaded package id(s): {:?}",
        real_unresolved.len(),
        real_unresolved,
    );
    // Sanity: we should have seen many distinct (pkg_id, pkg_name)
    // pairs -- one per package the fixture transitively touches.
    assert!(visitor.seen.len() >= 3, "expected at least 3 distinct tycon packages, saw {}", visitor.seen.len());
    Ok(())
}

#[test]
fn test_visitor_finds_interface_and_template() -> DamlLfResult<()> {
    use daml_lf::element::{DamlInterface, DamlTemplate};

    #[derive(Default)]
    struct Gather {
        templates: HashSet<String>,
        interfaces: HashSet<String>,
    }
    impl DamlElementVisitor for Gather {
        fn pre_visit_template<'a>(&mut self, t: &'a DamlTemplate<'a>) {
            self.templates.insert(t.name().to_owned());
        }
        fn pre_visit_interface<'a>(&mut self, i: &'a DamlInterface<'a>) {
            self.interfaces.insert(i.name().to_owned());
        }
    }
    let mut visitor = Gather::default();
    let dar = DarFile::from_file(FIXTURE_DAR)?;
    dar.apply(|archive| archive.accept(&mut visitor))?;
    assert!(visitor.templates.contains("Asset"), "templates: {:?}", visitor.templates);
    assert!(visitor.interfaces.contains("Holding"), "interfaces: {:?}", visitor.interfaces);
    Ok(())
}
