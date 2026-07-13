//! Daml v2 gRPC API demo.
//!
//! Walks through the full submitter-side surface of the v2 Ledger
//! API against the local Canton sandbox and the `TestingTypes`
//! fixture DAR:
//!
//! 1. Connect (no TLS, no auth — Canton dev mode).
//! 2. Check the participant's reported ledger-API version.
//! 3. Upload the fixture DAR if it's not already present.
//! 4. Allocate Alice + Bob with run-unique party-id hints.
//! 5. Create an `Asset` owned by Bob, issued by Alice.
//! 6. Exercise `Reassign` on the resulting contract via the
//!    `Holding` interface (the v2 "exercise-via-interface"
//!    pattern, addressed by interface package-name).
//! 7. Query the active-contracts page to confirm what's live.
//!
//! Prerequisites:
//!
//! ```sh
//! nix develop --command canton-sandbox   # in another terminal
//! ```
//!
//! Run:
//!
//! ```sh
//! cd examples/grpc-demo
//! nix develop ../.. --command cargo run
//! ```

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use daml::grpc_api::data::command::{DamlCommand, DamlCreateCommand, DamlExerciseCommand};
use daml::grpc_api::data::event::DamlEvent;
use daml::grpc_api::data::filter::{DamlEventFormat, DamlFilters};
use daml::grpc_api::data::package::DamlVettingChange;
use daml::grpc_api::data::value::{DamlEnum, DamlRecordBuilder, DamlValue};
use daml::grpc_api::data::DamlIdentifier;
use daml::grpc_api::{DamlCommandFactory, DamlGrpcClientBuilder};

const SANDBOX_URI: &str = "http://localhost:5011";
const FIXTURE_DAR: &str = "../../daml-lf/test_resources/TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar";
const PACKAGE_NAME: &str = "TestingTypes";
const APP_ID: &str = "grpc-demo";

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== grpc-demo: v2 Ledger API walk-through ===\n");

    // 1. Connect
    println!("connecting to {SANDBOX_URI} ...");
    let client = DamlGrpcClientBuilder::uri(SANDBOX_URI)
        .connect_timeout(Some(Duration::from_secs(10)))
        .timeout(Duration::from_secs(30))
        .connect()
        .await?;

    // 2. VersionService
    let version_info = client.version_service().get_ledger_api_version().await?;
    println!("  ledger API version: {}\n", version_info.version);

    // 3. Upload the fixture DAR
    println!("uploading {FIXTURE_DAR} ...");
    let dar_bytes = std::fs::read(FIXTURE_DAR)?;
    client
        .package_management_service()
        .upload_dar_file(dar_bytes, "grpc-demo-upload", DamlVettingChange::VetAllPackages, "")
        .await?;
    println!("  uploaded.\n");

    // 4. Allocate parties
    let tag = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let alice_hint = format!("Alice-{tag}");
    let bob_hint = format!("Bob-{tag}");
    println!("allocating parties (hints: {alice_hint}, {bob_hint}) ...");
    let alice = client
        .party_management_service()
        .allocate_party(&alice_hint, None, "", "", "")
        .await?;
    let bob = client.party_management_service().allocate_party(&bob_hint, None, "", "", "").await?;
    let alice_party = alice.party.clone();
    let bob_party = bob.party.clone();
    println!("  alice = {alice_party}");
    println!("  bob   = {bob_party}\n");

    // 5. Create an Asset
    println!("creating Fuji.Asset.Asset (issuer = alice, owner = bob) ...");
    let asset_template_id = DamlIdentifier::from_package_name(PACKAGE_NAME, "Fuji.Asset", "Asset");
    let asset_record = DamlRecordBuilder::new()
        .add_field("issuer", DamlValue::new_party(alice_party.as_str()))
        .add_field("owner", DamlValue::new_party(bob_party.as_str()))
        .add_field("ref", DamlValue::Text("DEMO-1".to_string()))
        .add_field("quantity", DamlValue::Int64(7))
        // `Color::Red` — enum type-id elided because Canton's
        // package-name addressing only applies to templates and
        // interfaces (the engine infers Color from the field type).
        .add_field("color", DamlValue::Enum(DamlEnum::new("Red", None)))
        .build();
    let create_factory = command_factory(&alice_party, "grpc-demo-wf-create");
    let create_commands = create_factory.make_command(DamlCommand::Create(DamlCreateCommand::new(
        asset_template_id,
        asset_record,
    )));
    let create_tx = client.command_service().submit_and_wait_for_transaction(create_commands, None).await?;
    let asset_cid = create_tx
        .events
        .iter()
        .find_map(|e| match e {
            DamlEvent::Created(c) => Some(c.contract_id.clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("Create tx had no Created event"))?;
    println!("  created contract: {asset_cid}");
    println!("  update_id:        {}\n", create_tx.update_id);

    // 6. Exercise Reassign via the Holding interface
    println!("exercising Holding.Reassign(target = alice) on the asset ...");
    let holding_id = DamlIdentifier::from_package_name(PACKAGE_NAME, "Fuji.Asset", "Holding");
    let exercise_arg = DamlValue::new_record(
        DamlRecordBuilder::new().add_field("target", DamlValue::new_party(alice_party.as_str())).build(),
    );
    let exercise_factory = command_factory(&bob_party, "grpc-demo-wf-exercise");
    let exercise_commands = exercise_factory.make_command(DamlCommand::Exercise(DamlExerciseCommand::new(
        holding_id,
        asset_cid.clone(),
        "Reassign",
        exercise_arg,
    )));
    let exercise_tx = client
        .command_service()
        .submit_and_wait_for_transaction(exercise_commands, None)
        .await?;
    println!("  exercised. update_id: {}\n", exercise_tx.update_id);

    // 7. Active-contracts snapshot scoped to Alice
    println!("querying StateService.GetActiveContractsPage (filter: alice) ...");
    let mut filters_by_party = HashMap::new();
    filters_by_party.insert(alice_party.clone(), DamlFilters::default());
    let page = client
        .state_service()
        .get_active_contracts_page(
            None,
            DamlEventFormat {
                filters_by_party,
                filters_for_any_party: None,
                verbose: true,
            },
            Some(100),
            None,
        )
        .await?;
    println!(
        "  page returned {} entries at offset {:?}.",
        page.active_contracts.len(),
        page.active_at_offset
    );

    println!("\n=== done ===");
    Ok(())
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
