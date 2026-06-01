//! v2 Ledger API end-to-end smoke test.
//!
//! Gated behind `--features integration` so the default
//! `cargo test` doesn't try to dial a participant. To run:
//!
//! 1. In one terminal: `nix develop --command canton-sandbox`
//!    (starts the sandbox on `localhost:5011`).
//! 2. In another: `cargo test -p daml-grpc --features integration`.
//!
//! The smoke test exercises:
//!
//!   * connection (no TLS, no auth — Canton dev mode),
//!   * `VersionService.GetLedgerApiVersion`,
//!   * `PackageManagementService.UploadDarFile` (the Phase 7 LF2
//!     fixture DAR),
//!   * `PartyManagementService.AllocateParty` (Alice + Bob),
//!   * `CommandService.SubmitAndWaitForTransaction` driving a
//!     `Create` of `Fuji.Asset.Asset`,
//!   * `CommandService.SubmitAndWaitForTransaction` driving an
//!     `Exercise` of the `Holding` interface's `Reassign` choice
//!     against the just-created contract (template_id addresses
//!     the interface by package-name),
//!   * `StateService.GetActiveContracts` to verify the ACS-delta
//!     view sees the contract.

#![cfg(feature = "integration")]

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, Result};

use daml_grpc::data::command::{DamlCommand, DamlCreateCommand, DamlExerciseCommand};
use daml_grpc::data::event::DamlEvent;
use daml_grpc::data::filter::{DamlEventFormat, DamlFilters};
use daml_grpc::data::package::DamlVettingChange;
use daml_grpc::data::value::{DamlEnum, DamlRecord, DamlRecordBuilder, DamlValue};
use daml_grpc::data::DamlIdentifier;
use daml_grpc::{DamlCommandFactory, DamlGrpcClient, DamlGrpcClientBuilder};

const SANDBOX_URI: &str = "http://localhost:5011";
const FIXTURE_DAR: &str =
    "../daml-lf/test_resources/TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar";
const PACKAGE_NAME: &str = "TestingTypes";
const ASSET_MODULE: &str = "Fuji.Asset";
const ASSET_TEMPLATE: &str = "Asset";
const HOLDING_INTERFACE: &str = "Holding";
const REASSIGN_CHOICE: &str = "Reassign";
const COLOR_RED: &str = "Red";
const APP_ID: &str = "rust-daml-bindings-smoke";

async fn connect() -> Result<DamlGrpcClient> {
    Ok(DamlGrpcClientBuilder::uri(SANDBOX_URI)
        .connect_timeout(Some(Duration::from_secs(10)))
        .timeout(Duration::from_secs(30))
        .connect()
        .await?)
}

fn asset_template_id() -> DamlIdentifier {
    DamlIdentifier::from_package_name(PACKAGE_NAME, ASSET_MODULE, ASSET_TEMPLATE)
}

fn holding_interface_id() -> DamlIdentifier {
    DamlIdentifier::from_package_name(PACKAGE_NAME, ASSET_MODULE, HOLDING_INTERFACE)
}

/// Build a `Color::Red` enum value. Canton's v2 validator restricts
/// `#<package-name>` addressing to templates and interfaces, so we
/// omit the enum's type identifier — the engine infers it from the
/// surrounding record field's declared type.
fn red_color() -> DamlValue {
    DamlValue::Enum(DamlEnum::new(COLOR_RED, None))
}

fn asset_record(issuer: &str, owner: &str, ref_id: &str, quantity: i64) -> DamlRecord {
    DamlRecordBuilder::new()
        .add_field("issuer", DamlValue::new_party(issuer))
        .add_field("owner", DamlValue::new_party(owner))
        .add_field("ref", DamlValue::Text(ref_id.to_string()))
        .add_field("quantity", DamlValue::Int64(quantity))
        .add_field("color", red_color())
        .build()
}

fn command_factory(party: &str, workflow_id: &str) -> DamlCommandFactory {
    DamlCommandFactory::new(
        workflow_id,
        APP_ID,
        vec![party.to_string()],
        Vec::<String>::new(),
        None,
        None,
    )
}

fn event_format_for(party: &str) -> DamlEventFormat {
    let mut filters_by_party = HashMap::new();
    filters_by_party.insert(party.to_string(), DamlFilters::default());
    DamlEventFormat {
        filters_by_party,
        filters_for_any_party: None,
        verbose: true,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke_test_end_to_end() -> Result<()> {
    let client = connect().await?;

    // ----- 1. VersionService -----
    let (version, _features) = client.version_service().get_ledger_api_version().await?;
    println!("sandbox ledger-api version: {version}");
    assert!(!version.is_empty(), "version string should be non-empty");

    // ----- 2. PackageManagementService.UploadDarFile -----
    let dar_bytes = std::fs::read(FIXTURE_DAR)?;
    client
        .package_management_service()
        .upload_dar_file(
            dar_bytes,
            "smoke-dar-upload",
            DamlVettingChange::VetAllPackages,
            "",
        )
        .await?;

    // ----- 3. PartyManagementService.AllocateParty (x2) -----
    // The Canton sandbox keeps allocated parties for the lifetime of
    // the process, so suffix each hint with a unique tag to keep
    // re-runs idempotent. The participant still gets to invent the
    // canonical id (the suffix only seeds the hint).
    use std::time::{SystemTime, UNIX_EPOCH};
    let tag = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let alice_hint = format!("Alice-{tag}");
    let bob_hint = format!("Bob-{tag}");
    let alice = client
        .party_management_service()
        .allocate_party(&alice_hint, None, "", "", "")
        .await?;
    let bob = client
        .party_management_service()
        .allocate_party(&bob_hint, None, "", "", "")
        .await?;
    let alice_party = alice.party.clone();
    let bob_party = bob.party.clone();
    println!("allocated parties: alice={alice_party} bob={bob_party}");

    // ----- 4. Create Asset (Alice as issuer, Bob as owner) -----
    let create_cmd = DamlCommand::Create(DamlCreateCommand::new(
        asset_template_id(),
        asset_record(&alice_party, &bob_party, "REF-001", 100),
    ));
    let create_commands =
        command_factory(&alice_party, "smoke-wf-create").make_command(create_cmd);
    let create_tx = client
        .command_service()
        .submit_and_wait_for_transaction(create_commands, None)
        .await?;
    println!("Create tx: update_id={}", create_tx.update_id);

    let asset_cid: String = create_tx
        .events
        .iter()
        .find_map(|e| match e {
            DamlEvent::Created(c) => Some(c.contract_id.clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("Create tx had no Created event"))?;
    println!("asset contract id: {asset_cid}");

    // ----- 5. Exercise Reassign via Holding (interface-addressed) -----
    let exercise_arg = DamlValue::new_record(
        DamlRecordBuilder::new()
            .add_field("target", DamlValue::new_party(alice_party.as_str()))
            .build(),
    );
    let exercise_cmd = DamlCommand::Exercise(DamlExerciseCommand::new(
        holding_interface_id(),
        asset_cid.clone(),
        REASSIGN_CHOICE,
        exercise_arg,
    ));
    let exercise_commands =
        command_factory(&bob_party, "smoke-wf-exercise").make_command(exercise_cmd);
    let exercise_tx = client
        .command_service()
        .submit_and_wait_for_transaction(exercise_commands, None)
        .await?;
    println!("Exercise (Reassign via Holding) tx: update_id={}", exercise_tx.update_id);

    // ----- 6. StateService.GetActiveContracts (sanity check) -----
    // Use the page (non-streaming) API with `active_at_offset =
    // None` — that delegates to the participant, which picks its
    // current ledger end. Asking for offset 0 (BEGIN) is legal but
    // returns an empty set and on Canton 3.5.1 surfaces a tonic
    // transport error when the stream closes early.
    let page = client
        .state_service()
        .get_active_contracts_page(
            None,
            event_format_for(&alice_party),
            Some(100),
            None,
        )
        .await?;
    println!(
        "ACS page: {} contract entries at offset {:?}",
        page.active_contracts.len(),
        page.active_at_offset
    );

    Ok(())
}

