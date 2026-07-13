//! Renderer-level unit tests.
//!
//! Drive the codegen renderer against the LF2 fixture DAR and
//! assert specific patterns in the emitted token stream. These run
//! as part of the default `cargo test` — no sandbox required.
//!
//! The tests act as regression coverage for bugs the v2 migration
//! has already produced once:
//!
//! * v2 template addressing: `template_id()` must address by
//!   package-name, not package-id.
//! * Interfaces: each declared interface produces a trait + a
//!   `impl Trait for ContractId {}` for every implementing template
//!   + a `<interface>_<choice>_command` method per inherited choice.
//! * Cross-package `package_name` resolution at the convert layer —
//!   every cross-package path must carry a non-empty package-name
//!   segment.
//! * Variant-with-record-payload constructors produce a synthetic
//!   `pub mod <variant>` containing the payload records.

use daml_codegen::generator::{ModuleMatcher, RenderMethod};
use daml_codegen::renderer::quote_archive;
use daml_lf::DarFile;

const FIXTURE_DAR: &str = "../daml-lf/test_resources/TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar";

/// Render the fixture archive with the codegen-demo's module
/// filter set. Returns the emitted Rust source as a string.
///
/// Mirrors what `examples/codegen-demo/build.rs` does at build
/// time, minus writing to disk.
fn render_demo_modules() -> String {
    let dar = DarFile::from_file(FIXTURE_DAR).expect("read fixture DAR");
    let matchers: Vec<&str> = vec![r"^Fuji\.Asset$", r"^Fuji\.Types$", r"^DA\.Internal\.Template$"];
    dar.apply(|archive| {
        let matcher = ModuleMatcher::new(&matchers).expect("matcher regex");
        quote_archive(archive, &matcher, &RenderMethod::Full).to_string()
    })
    .expect("apply archive")
}

fn assert_contains(rendered: &str, needle: &str, why: &str) {
    assert!(
        rendered.contains(needle),
        "{why}\n  needle: `{needle}`\n  not found in rendered output (first 400 chars: {})",
        rendered.get(..400.min(rendered.len())).unwrap_or(""),
    );
}

#[test]
fn template_id_uses_package_name() {
    // Regression: `Asset::template_id()` must address the template
    // by package-name, not package-id. The participant
    // accepts either form, but only the name form survives a
    // package-upgrade. The renderer should never emit
    // `from_package_id`.
    let r = render_demo_modules();
    assert_contains(&r, "fn template_id () -> DamlIdentifier", "Asset must expose template_id()");
    assert_contains(&r, "DamlIdentifier :: from_package_name", "template_id() must address by package-name");
    assert!(!r.contains("from_package_id"), "renderer should not emit any from_package_id call in v2 codegen",);
}

#[test]
fn variant_record_payload_lives_in_synthetic_module() {
    // Follow-up 2 regression: `data Shape = Circle { radius : Decimal }
    // | Rectangle { ... } | Polygon [...]` compiles to LF with two
    // synthetic payload records `Shape.Circle` / `Shape.Rectangle`.
    // The renderer must emit them under a `pub mod shape` so the
    // variant's `crate :: ... :: shape :: Circle` references resolve.
    let r = render_demo_modules();
    assert_contains(&r, "pub mod shape", "synthetic payload sub-module must be emitted");
    assert_contains(&r, "pub struct Circle", "Circle payload record must be emitted");
    assert_contains(&r, "pub struct Rectangle", "Rectangle payload record must be emitted");
    // The variant `Shape` itself references the payload records via
    // the synthetic submodule path.
    assert_contains(
        &r,
        "Circle (crate :: testing_types :: fuji :: types :: shape :: Circle)",
        "Shape::Circle variant must reference shape::Circle by full path",
    );
    assert_contains(
        &r,
        "Rectangle (crate :: testing_types :: fuji :: types :: shape :: Rectangle)",
        "Shape::Rectangle variant must reference shape::Rectangle by full path",
    );
}

#[test]
fn cross_package_paths_carry_package_name() {
    // Follow-up 1 regression: a tycon that points into another
    // package must render with that package's name as the leading
    // path segment. Empty segments (`crate :: :: ...`) indicate the
    // convert layer failed to resolve `package_name` and the
    // renderer fell through to an empty `Cow`.
    let r = render_demo_modules();
    // The Asset template's archive_command takes
    // DA.Internal.Template.Archive, which lives in the daml-stdlib
    // package; the rendered path must carry that package's name.
    assert_contains(&r, "ghc_stdlib_da_internal_template", "cross-package path must include the target package's name");
    // The hard invariant: no rendered `crate :: ` path should have
    // an empty segment.
    assert!(!r.contains("crate :: :: "), "rendered output contains an empty path segment (unresolved package_name)",);
}

#[test]
fn interface_trait_and_impl_emitted() {
    // Regression: each Daml interface produces a Rust trait, and
    // every template that implements the interface gets
    // an `impl Trait for <Template>ContractId` block.
    let r = render_demo_modules();
    assert_contains(&r, "pub trait Holding", "Holding interface must produce a Rust trait");
    assert_contains(&r, "fn interface_id () -> DamlIdentifier", "interface trait must expose interface_id()");
    assert_contains(
        &r,
        "impl crate :: testing_types :: fuji :: asset :: Holding for AssetContractId",
        "AssetContractId must impl the Holding interface trait",
    );
}

#[test]
fn interface_addressed_choice_methods_emitted() {
    // Regression: a contract that implements an interface gets
    // `<interface>_<choice>_command` methods on its contract-id
    // (one per interface choice, including inherited Archive). The
    // method dispatches via the interface's `interface_id()`, not
    // the template's `template_id()`.
    let r = render_demo_modules();
    assert_contains(&r, "pub fn holding_reassign_command", "AssetContractId must expose holding_reassign_command");
    assert_contains(
        &r,
        "pub fn holding_archive_command",
        "AssetContractId must expose the inherited holding_archive_command",
    );
    assert_contains(
        &r,
        "as crate :: testing_types :: fuji :: asset :: Holding > :: interface_id ()",
        "interface-addressed command method must dispatch via interface_id()",
    );
}

#[test]
fn module_matcher_filters_out_unmatched_modules() {
    // Sanity check: the matcher filter actually restricts which
    // modules emit their contents. Render with only Fuji.Asset
    // included; Fuji.Types data types must not appear.
    let dar = DarFile::from_file(FIXTURE_DAR).expect("read fixture DAR");
    let only_asset = dar
        .apply(|archive| {
            let matcher = ModuleMatcher::new(&[r"^Fuji\.Asset$"]).expect("matcher");
            quote_archive(archive, &matcher, &RenderMethod::Full).to_string()
        })
        .expect("apply");
    // Fuji.Asset's templates / interfaces should be present.
    assert_contains(&only_asset, "pub struct Asset", "Fuji.Asset's Asset must render");
    assert_contains(&only_asset, "pub trait Holding", "Fuji.Asset's Holding must render");
    // Fuji.Types's variant + records must NOT render.
    assert!(!only_asset.contains("pub enum Shape"), "Fuji.Types::Shape leaked despite matcher filtering it out",);
    assert!(!only_asset.contains("pub struct Painted"), "Fuji.Types::Painted leaked despite matcher filtering it out",);
    // The synthetic `pub mod shape` is data inside Fuji.Types and
    // must follow the same filtering rule.
    assert!(
        !only_asset.contains("pub mod shape"),
        "synthetic shape sub-module leaked despite Fuji.Types being filtered out",
    );
}

#[test]
fn enum_constructors_round_trip_via_damlvalue() {
    // Regression: Daml enums get a Rust enum whose
    // `From<X> for DamlValue` and `TryFrom<DamlValue> for X` impls
    // round-trip every declared constructor. The fixture's `Color`
    // enum has three constructors.
    let r = render_demo_modules();
    assert_contains(&r, "pub enum Color", "Color enum must be emitted");
    for ctor in ["Red", "Green", "Blue"] {
        assert_contains(
            &r,
            &format!("Color :: {ctor} => DamlValue :: new_enum"),
            &format!("Color::{ctor} must round-trip via DamlValue::new_enum"),
        );
        assert_contains(
            &r,
            &format!("\"{ctor}\" => Ok (Color :: {ctor})"),
            &format!("deserialize_from must accept the \"{ctor}\" constructor"),
        );
    }
}

#[test]
fn rendered_archive_compiles_to_balanced_braces() {
    // Cheap structural smoke test: the renderer's output must have
    // balanced braces. An off-by-one in module nesting or template
    // emission usually shows up here long before it shows up as a
    // missing-identifier error downstream.
    let r = render_demo_modules();
    let open = r.matches('{').count();
    let close = r.matches('}').count();
    assert_eq!(open, close, "unbalanced braces in rendered output: {open} open, {close} close");
    let paren_open = r.matches('(').count();
    let paren_close = r.matches(')').count();
    assert_eq!(paren_open, paren_close, "unbalanced parens in rendered output: {paren_open} open, {paren_close} close",);
}
