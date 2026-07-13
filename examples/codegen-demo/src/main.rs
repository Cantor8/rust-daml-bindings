//! Daml codegen demo — same end-to-end flow as `grpc-demo`, but
//! against **typed** Rust representations of the LF2 fixture's
//! templates and interfaces.
//!
//! `build.rs` runs `daml::codegen::generator::daml_codegen` over
//! the fixture DAR and writes the generated tree to `src/autogen/`.
//! This binary `include!`s the generated entry-point module at
//! crate root and then uses `Asset::new(...)`,
//! `Asset::create_command()`, `AssetContractId::holding_reassign_command(...)`
//! to drive the ledger.
//!
//! Prerequisites: `nix develop --command canton-sandbox` running
//! in another terminal.

#![allow(non_snake_case)]

include!("autogen/testing_types_3_0_0.rs");

use anyhow::{anyhow, Result};
use std::convert::TryFrom;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use daml::grpc_api::data::command::DamlCommand;
use daml::grpc_api::data::event::DamlEvent;
use daml::grpc_api::{DamlCommandFactory, DamlGrpcClientBuilder};
use daml::prelude::*;

use crate::testing_types::fuji::asset::{Asset, AssetContractId, Color, Reassign};

const SANDBOX_URI: &str = "http://localhost:5011";
const FIXTURE_DAR: &str = "../../daml-lf/test_resources/TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar";
const APP_ID: &str = "codegen-demo";

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== codegen-demo: typed v2 flow against the fixture DAR ===\n");

    let client = DamlGrpcClientBuilder::uri(SANDBOX_URI)
        .connect_timeout(Some(Duration::from_secs(10)))
        .timeout(Duration::from_secs(30))
        .connect()
        .await?;
    println!("connected to {SANDBOX_URI}");

    // Upload the same DAR the build.rs codegen ran against.
    // The sandbox keeps packages across runs; if this exact DAR is
    // already vetted from a previous run we get
    // `KNOWN_PACKAGE_VERSION`, which is fine — the participant
    // already has what we need.
    let dar_bytes = std::fs::read(FIXTURE_DAR)?;
    match client
        .package_management_service()
        .upload_dar_file(
            dar_bytes,
            "codegen-demo-upload",
            daml::grpc_api::data::package::DamlVettingChange::VetAllPackages,
            "",
        )
        .await
    {
        Ok(()) => println!("uploaded {FIXTURE_DAR}"),
        Err(daml::grpc_api::data::DamlError::GrpcStatusError(s))
            if s.message().contains("KNOWN_PACKAGE_VERSION") =>
        {
            println!("DAR already known to participant — skipping upload");
        },
        Err(e) => return Err(e.into()),
    }

    // Allocate run-unique parties.
    let tag = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let alice = client
        .party_management_service()
        .allocate_party(format!("Alice-{tag}"), None, "", "", "")
        .await?
        .party
        .clone();
    let bob = client
        .party_management_service()
        .allocate_party(format!("Bob-{tag}"), None, "", "", "")
        .await?
        .party
        .clone();
    println!("alice = {alice}");
    println!("bob   = {bob}\n");

    // Typed create: Asset::new builds the Rust value; create_command
    // turns it into the `DamlCreateCommand` the gRPC wrapper expects.
    let asset = Asset::new(alice.as_str(), bob.as_str(), "REF-1", 42, Color::Red);
    println!("creating Asset {{ issuer: {alice}, owner: {bob}, ref: REF-1, quantity: 42, color: Red }} ...");

    let create_factory = command_factory(&alice, "codegen-demo-create");
    let create_commands = create_factory.make_command(DamlCommand::Create(asset.create_command()));
    let create_tx = client.command_service().submit_and_wait_for_transaction(create_commands, None).await?;

    let created = create_tx
        .events
        .into_iter()
        .find_map(|e| match e {
            DamlEvent::Created(c) => Some(c),
            _ => None,
        })
        .ok_or_else(|| anyhow!("Create tx had no Created event"))?;
    let cid = AssetContractId::try_from(DamlContractId::new(created.contract_id.clone()))?;
    println!("  created: {}\n", created.contract_id);

    // Exercise the Holding.Reassign choice via the inherited
    // interface method. The generated method name encodes the
    // interface so it doesn't collide with Asset's own choices —
    // `holding_reassign_command`.
    println!("exercising AssetContractId::holding_reassign_command(target = alice) ...");
    let exercise_factory = command_factory(&bob, "codegen-demo-reassign");
    let exercise_commands =
        exercise_factory.make_command(DamlCommand::Exercise(cid.holding_reassign_command(Reassign::new(alice.clone()))));
    let exercise_tx = client
        .command_service()
        .submit_and_wait_for_transaction(exercise_commands, None)
        .await?;
    println!("  exercised. update_id: {}\n", exercise_tx.update_id);

    println!("=== done ===");
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
