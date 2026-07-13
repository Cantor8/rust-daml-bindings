//! Behavioural tests for the daml-derive attribute macros.
//!
//! The doctests in `src/lib.rs` only prove "the macros emit
//! something that compiles." These tests go a step further and
//! assert what the generated code actually *does* — i.e. that the
//! emitted `template_id()`, `create_command()`, `*_command(...)`,
//! and `DamlSerializeFrom` / `DamlDeserializeFrom` impls behave
//! the way the codegen-demo end-to-end flow relies on.
//!
//! Tests live in `tests/` (not inline `#[cfg(test)] mod`) because
//! `daml-derive` is a proc-macro crate and only integration tests
//! can `use` the macros it exports.

#![allow(non_snake_case)]

use daml::prelude::*;

// -------------------------------------------------------------------
// #[DamlEnum]
// -------------------------------------------------------------------

#[DamlEnum]
pub enum Color {
    Red,
    Green,
    Blue,
}

#[test]
fn enum_constructors_round_trip_through_daml_value() {
    // Every declared constructor must `serialize_into` to a
    // `DamlValue::Enum` whose label matches the Rust variant, and
    // must `deserialize_from` the same wire value back to the same
    // Rust variant.
    for original in [Color::Red, Color::Green, Color::Blue] {
        let wire: DamlValue = original.clone().serialize_into();
        let DamlValue::Enum(ref e) = wire else {
            panic!("DamlEnum macro must emit DamlValue::Enum, got {wire:?}");
        };
        let label = e.constructor().to_owned();
        let expected = match original {
            Color::Red => "Red",
            Color::Green => "Green",
            Color::Blue => "Blue",
        };
        assert_eq!(label, expected, "constructor label must match Rust variant");
        let back = Color::deserialize_from(wire).expect("known constructor must deserialize");
        assert_eq!(back, original);
    }
}

#[test]
fn enum_rejects_unknown_constructor() {
    // `DamlEnum` here resolves to the daml-grpc wire type via the
    // prelude (the same name also re-exports the macro at proc-
    // macro position; Rust keeps the two in separate namespaces).
    let bogus = DamlValue::Enum(DamlEnum::new("Yellow", None));
    let err = Color::deserialize_from(bogus).expect_err("unknown constructor must error");
    assert!(matches!(err, DamlError::UnexpectedVariant(_, _)), "got: {err:?}");
}

// -------------------------------------------------------------------
// #[DamlData] (record)
// -------------------------------------------------------------------

#[DamlData]
pub struct Profile {
    pub name: DamlText,
    pub age: DamlInt64,
}

#[test]
fn data_record_round_trips_through_daml_value() {
    let original = Profile {
        name: "Alice".to_string(),
        age: 30,
    };
    let wire: DamlValue = original.clone().serialize_into();
    let DamlValue::Record(ref _r) = wire else {
        panic!("DamlData macro must emit DamlValue::Record, got {wire:?}");
    };
    let back = Profile::deserialize_from(wire).expect("record must deserialize");
    assert_eq!(back.name, original.name);
    assert_eq!(back.age, original.age);
}

#[test]
fn data_record_emits_new_constructor() {
    // The macro is documented to emit `MyData::new(...)`. Verify
    // the constructor exists and produces the same fields as struct
    // literal syntax.
    let via_new = Profile::new("Bob", 41);
    assert_eq!(via_new.name, "Bob");
    assert_eq!(via_new.age, 41);
}

// -------------------------------------------------------------------
// #[DamlVariant]
// -------------------------------------------------------------------

#[DamlData]
pub struct RGBA {
    pub red: DamlInt64,
    pub green: DamlInt64,
    pub blue: DamlInt64,
    pub alpha: DamlInt64,
}

#[DamlVariant]
pub enum Paint {
    Solid,
    Custom(DamlInt64),
    Mix(RGBA),
}

#[test]
fn variant_round_trips_every_constructor() {
    let cases: Vec<Paint> = vec![Paint::Solid, Paint::Custom(7), Paint::Mix(RGBA::new(10, 20, 30, 255))];
    for original in cases {
        let wire: DamlValue = original.clone().serialize_into();
        let DamlValue::Variant(ref v) = wire else {
            panic!("DamlVariant macro must emit DamlValue::Variant for {original:?}, got {wire:?}");
        };
        let ctor = v.constructor().to_owned();
        let expected_ctor = match &original {
            Paint::Solid => "Solid",
            Paint::Custom(_) => "Custom",
            Paint::Mix(_) => "Mix",
        };
        assert_eq!(ctor, expected_ctor, "constructor label must match Rust variant");
        let back = Paint::deserialize_from(wire).expect("known variant must deserialize");
        assert_eq!(back, original);
    }
}

#[test]
fn variant_rejects_unknown_constructor() {
    let bogus = DamlValue::Variant(DamlVariant::new("Glittery", Box::new(DamlValue::new_unit()), None));
    let err = Paint::deserialize_from(bogus).expect_err("unknown variant must error");
    assert!(matches!(err, DamlError::UnexpectedVariant(_, _)), "got: {err:?}");
}

// -------------------------------------------------------------------
// #[DamlTemplate] addressing — package_name vs package_id
// -------------------------------------------------------------------

#[DamlTemplate(package_name = "MyApp", module_name = "Fuji.PingPong")]
pub struct Ping {
    pub sender: DamlParty,
    pub receiver: DamlParty,
    pub count: DamlInt64,
}

#[DamlChoices]
impl Ping {
    #[ResetCount]
    fn reset_count(&self, _new_count: DamlInt64) {}
}

#[DamlTemplate(package_id = "deadbeefcafe", module_name = "Fuji.Legacy")]
pub struct LegacyPing {
    pub sender: DamlParty,
}

#[test]
fn template_id_uses_package_name_when_provided() {
    let id = Ping::template_id();
    // The from_package_name addressing prefixes the package ref
    // with `#`; verify both halves of the wire shape.
    assert!(id.is_package_name(), "package_name addressing should be in effect");
    assert_eq!(id.package_name(), Some("MyApp"));
    assert_eq!(id.module_name(), "Fuji.PingPong");
    assert_eq!(id.entity_name(), "Ping");
}

#[test]
fn template_id_falls_back_to_package_id_when_no_name() {
    let id = LegacyPing::template_id();
    assert!(!id.is_package_name(), "package_id addressing should be in effect");
    assert_eq!(id.package_id(), Some("deadbeefcafe"));
    assert_eq!(id.module_name(), "Fuji.Legacy");
    assert_eq!(id.entity_name(), "LegacyPing");
}

#[test]
fn template_new_constructor_round_trips_through_create_command() {
    let ping = Ping::new("Alice", "Bob", 0);
    let cmd = ping.create_command();
    // The DamlCreateCommand must address the same template
    // identifier that `template_id()` would produce.
    assert_eq!(*cmd.template_id(), Ping::template_id());
}

#[test]
fn template_value_round_trips_through_daml_value() {
    let original = Ping::new("Alice", "Bob", 42);
    let wire: DamlValue = original.clone().serialize_into();
    let DamlValue::Record(_) = wire else {
        panic!("template payload must serialize to DamlValue::Record");
    };
    let back = Ping::deserialize_from(wire).expect("record must deserialize");
    assert_eq!(back.sender, original.sender);
    assert_eq!(back.receiver, original.receiver);
    assert_eq!(back.count, original.count);
}

// -------------------------------------------------------------------
// #[DamlChoices]
// -------------------------------------------------------------------

#[test]
fn choice_command_method_emits_exercise_command() {
    // The #[DamlChoices] macro generates
    // `<choice>_command(&self, arg)` on the *ContractId* type. The
    // method must produce a `DamlExerciseCommand` that addresses
    // the *template* by template_id (not the choice argument's
    // identifier) and carries the choice's original case name.
    let cid = PingContractId::try_from(DamlContractId::new("test-cid")).expect("construct cid");
    let cmd = cid.reset_count_command(7);
    assert_eq!(*cmd.template_id(), Ping::template_id(), "exercise must address the template");
    assert_eq!(cmd.contract_id(), "test-cid");
    assert_eq!(cmd.choice(), "ResetCount", "choice name must match the Daml choice");
}

// -------------------------------------------------------------------
// #[DamlInterface] + #[DamlTemplate(implements = ...)]
//
// The codegen emits `impl <crate>::<pkg>::<module>::<Iface> for
// <Template>ContractId {}` blocks — a fully-qualified path
// starting at `crate::`. So the test types must live in modules
// whose names match the snake_case'd package-name / module-path
// segments. We mirror the codegen-demo layout for that reason.
// -------------------------------------------------------------------

mod my_app {
    pub mod fuji {
        pub mod holding {
            use daml::prelude::*;

            #[DamlInterface(package_name = "MyApp", module_name = "Fuji.Holding")]
            pub struct HoldingMarker;
        }

        pub mod asset {
            use daml::prelude::*;

            // `<pkg>:<Module.Path>:<Entity>` -> resolves to
            // `crate::my_app::fuji::holding::HoldingMarker`.
            #[DamlTemplate(
                package_name = "MyApp",
                module_name = "Fuji.Asset",
                implements = "MyApp:Fuji.Holding:HoldingMarker"
            )]
            pub struct Asset {
                pub issuer: DamlParty,
                pub owner: DamlParty,
            }
        }
    }
}

#[test]
fn interface_id_uses_package_name_addressing() {
    // `interface_id()` is a default method on the generated trait;
    // we call it through the implementing template's contract-id
    // (which exists because the template uses `implements = ...`).
    use my_app::fuji::asset::AssetContractId;
    use my_app::fuji::holding::HoldingMarker;
    let id = <AssetContractId as HoldingMarker>::interface_id();
    assert!(id.is_package_name());
    assert_eq!(id.package_name(), Some("MyApp"));
    assert_eq!(id.module_name(), "Fuji.Holding");
    assert_eq!(id.entity_name(), "HoldingMarker");
}

#[test]
fn template_with_implements_emits_interface_impl() {
    // Presence of the `impl HoldingMarker for AssetContractId`
    // block is verified at compile time: the trait bound below
    // fails to resolve if the macro skipped the impl emission.
    use my_app::fuji::asset::AssetContractId;
    use my_app::fuji::holding::HoldingMarker;
    fn assert_impl<T: HoldingMarker>() {}
    assert_impl::<AssetContractId>();
}
