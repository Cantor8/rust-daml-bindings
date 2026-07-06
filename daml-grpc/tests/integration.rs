//! v2 Ledger API integration tests.
//!
//! Gated behind `--features integration` so the default
//! `cargo test` doesn't try to dial a participant. To run:
//!
//! 1. In one terminal: `nix develop --command canton-sandbox`
//!    (starts the sandbox on `localhost:5011`).
//! 2. In another: `cargo test -p daml-grpc --features integration`.
//!
//! Coverage is one `#[tokio::test]` per service, plus an
//! end-to-end smoke test ([`smoke_test_end_to_end`]) that
//! exercises the create / interface-addressed exercise / ACS path
//! together. Each per-service test:
//!
//!   * dials the sandbox afresh,
//!   * uploads the fixture DAR if needed (idempotent),
//!   * allocates its own parties with a per-process unique tag, so
//!     parallel runs and re-runs don't collide.
//!
//! A handful of services need participant configuration the stock
//! sandbox doesn't carry (static-time clock, multi-synchronizer
//! reassignment, command-inspection store). Those tests document
//! the expected `UNIMPLEMENTED` / `INVALID_ARGUMENT` outcome and
//! pass when the participant returns it.

#![cfg(feature = "integration")]

use std::collections::HashMap;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use chrono::Duration as ChronoDuration;
use futures::StreamExt;

use daml_grpc::data::command::{DamlCommand, DamlCreateCommand, DamlExerciseCommand};
use daml_grpc::data::event::DamlEvent;
use daml_grpc::data::filter::{
    DamlEventFormat, DamlFilters, DamlTransactionFormat, DamlTransactionShape, DamlUpdateFormat,
};
use daml_grpc::data::identity_provider::DamlIdentityProviderConfig;
use daml_grpc::data::inspection::DamlCommandState;
use daml_grpc::data::offset::DamlLedgerOffset;
use daml_grpc::data::package::DamlVettingChange;
use daml_grpc::data::party::DamlObjectMeta;
use daml_grpc::data::update::DamlUpdate;
use daml_grpc::data::user::{DamlUser, DamlUserRight};
use daml_grpc::data::value::{DamlEnum, DamlRecord, DamlRecordBuilder, DamlValue};
use daml_grpc::data::{DamlError, DamlIdentifier};
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

fn transaction_format_for(party: &str) -> DamlTransactionFormat {
    DamlTransactionFormat {
        event_format: event_format_for(party),
        transaction_shape: DamlTransactionShape::AcsDelta,
    }
}

fn update_format_for(party: &str) -> DamlUpdateFormat {
    DamlUpdateFormat {
        include_transactions: Some(transaction_format_for(party)),
        include_reassignments: None,
        include_topology_events: None,
    }
}

/// Nanosecond-precision tag derived from wall-clock time. Used to
/// suffix party hints, user ids, IDP ids etc. so each test run
/// (and parallel test cases within a run) gets a fresh namespace.
fn unique_tag() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}

/// Allocate a fresh party with a unique hint and return the
/// canonical id chosen by the participant.
async fn alloc_party(client: &DamlGrpcClient, hint_prefix: &str) -> Result<String> {
    let hint = format!("{hint_prefix}-{}", unique_tag());
    Ok(client
        .party_management_service()
        .allocate_party(&hint, None, "", "", "")
        .await?
        .party)
}

/// Upload the fixture DAR. Canton accepts re-uploading the same DAR
/// as a no-op, so callers can invoke this from every test without
/// coordinating.
async fn ensure_dar_uploaded(client: &DamlGrpcClient, submission_id: &str) -> Result<()> {
    let dar_bytes = std::fs::read(FIXTURE_DAR)?;
    client
        .package_management_service()
        .upload_dar_file(dar_bytes, submission_id, DamlVettingChange::VetAllPackages, "")
        .await?;
    Ok(())
}

/// Read the package-id of the `TestingTypes` package by matching its
/// declared name in the participant's `ListKnownPackages` view.
async fn testing_types_package_id(client: &DamlGrpcClient) -> Result<String> {
    Ok(client
        .package_management_service()
        .list_known_packages()
        .await?
        .into_iter()
        .find(|p| p.name == PACKAGE_NAME)
        .ok_or_else(|| anyhow!("participant doesn't know the {PACKAGE_NAME} package"))?
        .package_id)
}

/// Submit a Create of `Fuji.Asset.Asset` and wait for the
/// transaction. Returns the `(contract_id, update_id, offset)`
/// triple — enough for downstream services (Contract / Event /
/// Update) to chase the freshly created contract.
async fn create_asset(
    client: &DamlGrpcClient,
    issuer: &str,
    owner: &str,
    workflow_id: &str,
) -> Result<(String, String, DamlLedgerOffset)> {
    let cmd = DamlCommand::Create(DamlCreateCommand::new(
        asset_template_id(),
        asset_record(issuer, owner, "REF", 42),
    ));
    let commands = command_factory(issuer, workflow_id).make_command(cmd);
    let tx = client
        .command_service()
        .submit_and_wait_for_transaction(commands, None)
        .await?;
    let cid = tx
        .events
        .iter()
        .find_map(|e| match e {
            DamlEvent::Created(c) => Some(c.contract_id.clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("Create tx had no Created event"))?;
    Ok((cid, tx.update_id, tx.offset))
}

/// True when `e` is a gRPC `Status` carrying the given code.
fn is_status(e: &DamlError, code: tonic::Code) -> bool {
    matches!(e, DamlError::GrpcStatusError(s) | DamlError::GrpcPermissionError(s) if s.code() == code)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke_test_end_to_end() -> Result<()> {
    let client = connect().await?;

    // ----- 1. VersionService -----
    let version_info = client.version_service().get_ledger_api_version().await?;
    println!("sandbox ledger-api version: {}", version_info.version);
    assert!(!version_info.version.is_empty(), "version string should be non-empty");

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
    let tag = unique_tag();
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

// ---------------------------------------------------------------------------
// Per-service coverage. One `#[tokio::test]` per ledger-API service.
// Tests are independent — each dials the sandbox afresh, uploads the
// DAR if needed, and allocates its own parties. Order doesn't matter.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn version_service_get_ledger_api_version() -> Result<()> {
    let client = connect().await?;
    let version_info = client.version_service().get_ledger_api_version().await?;
    assert!(!version_info.version.is_empty(), "version string must be non-empty");
    println!(
        "version={}, features.user_management={:?}",
        version_info.version,
        version_info.features.is_some(),
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_management_service_all_methods() -> Result<()> {
    let client = connect().await?;
    ensure_dar_uploaded(&client, "pkg-mgmt-upload").await?;

    // validate_dar_file: validates without committing; uploading the
    // same DAR twice would re-vet, but validation has no side effects.
    let dar_bytes = std::fs::read(FIXTURE_DAR)?;
    client.package_management_service().validate_dar_file(dar_bytes, "pkg-mgmt-validate", "").await?;

    // list_known_packages: TestingTypes must appear after the upload above.
    let listed = client.package_management_service().list_known_packages().await?;
    assert!(
        listed.iter().any(|p| p.name == PACKAGE_NAME),
        "{PACKAGE_NAME} should appear in list_known_packages, saw {:?}",
        listed.iter().map(|p| &p.name).collect::<Vec<_>>(),
    );

    // update_vetted_packages with `dry_run = true`: exercises the RPC
    // without touching topology, which avoids interfering with later
    // tests / runs. The change is a no-op Unvet of a synthetic
    // (package-id, name) tuple that doesn't exist on the participant;
    // dry-run accepts it because no validation against real state runs.
    use daml_grpc::data::package::{DamlVettedPackagesChange, DamlVettedPackagesRef};
    let outcome = client
        .package_management_service()
        .update_vetted_packages(
            vec![DamlVettedPackagesChange::Unvet {
                packages: vec![DamlVettedPackagesRef {
                    package_id: "0000".to_owned(),
                    package_name: String::new(),
                    package_version: String::new(),
                }],
            }],
            /* dry_run */ true,
            "",
            None,
            std::iter::empty(),
        )
        .await?;
    println!("vetting dry-run outcome: past={:?} new={:?}", outcome.past_vetted_packages.is_some(), outcome.new_vetted_packages.is_some());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_service_all_methods() -> Result<()> {
    let client = connect().await?;
    ensure_dar_uploaded(&client, "pkg-svc-upload").await?;
    let pkg_id = testing_types_package_id(&client).await?;

    // list_packages: should include the TestingTypes package-id.
    let ids = client.package_service().list_packages().await?;
    assert!(ids.iter().any(|id| id == &pkg_id), "list_packages missing TestingTypes id");

    // get_package: fetch the on-wire payload.
    let pkg = client.package_service().get_package(&pkg_id).await?;
    assert!(!pkg.payload().is_empty(), "package payload should be non-empty");

    // get_package_status: registered + willing-to-use.
    let status = client.package_service().get_package_status(&pkg_id).await?;
    println!("package status: {status:?}");

    // list_vetted_packages: at least one synchronizer should report a
    // vetting entry; page size 50 is plenty for a fresh sandbox.
    let page = client.package_service().list_vetted_packages(None, None, "", 50).await?;
    println!("vetted-packages page: {} synchronizer entries", page.vetted_packages.len());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn party_management_service_all_methods() -> Result<()> {
    let client = connect().await?;

    // get_participant_id: stable across the lifetime of the participant.
    let pid = client.party_management_service().get_participant_id().await?;
    assert!(!pid.is_empty(), "participant id must be non-empty");
    println!("participant id: {pid}");

    // allocate_party: covered by the helper.
    let alice = alloc_party(&client, "party-mgmt-alice").await?;

    // get_parties: read-back the allocated party. Empty IDP id =
    // default IDP, which the helper allocates under.
    let details = client
        .party_management_service()
        .get_parties(vec![alice.clone()], "")
        .await?;
    assert!(
        details.iter().any(|p| p.party == alice),
        "get_parties should include the freshly-allocated party",
    );

    // list_known_parties: page through; the participant returns at
    // least the party we just allocated.
    let page = client.party_management_service().list_known_parties("", "", 200, "").await?;
    assert!(page.party_details.iter().any(|p| p.party == alice), "list_known_parties missing alice");

    // update_party_details: annotate the party. Annotations are
    // participant-local; the participant returns the new details
    // with a bumped resource_version.
    let mut new_details = details.into_iter().find(|p| p.party == alice).unwrap();
    new_details.local_metadata = Some(DamlObjectMeta {
        resource_version: String::new(),
        annotations: [("smoke-test".to_owned(), "yes".to_owned())].into_iter().collect(),
    });
    let updated = client
        .party_management_service()
        .update_party_details(new_details, ["local_metadata.annotations".to_owned()])
        .await?;
    assert_eq!(
        updated.local_metadata.as_ref().and_then(|m| m.annotations.get("smoke-test")).map(String::as_str),
        Some("yes"),
        "annotation should round-trip",
    );

    // update_party_identity_provider_id: move alice from default IDP
    // to itself (default→default). Accepts the no-op move and exercises
    // the RPC wiring without needing an external IDP to be configured.
    client
        .party_management_service()
        .update_party_identity_provider_id(alice, "", "")
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn state_service_all_methods() -> Result<()> {
    let client = connect().await?;
    let alice = alloc_party(&client, "state-svc-alice").await?;

    // get_ledger_end: any non-negative offset (BEGIN on a fresh
    // sandbox, > BEGIN after even one upload/allocation).
    let end = client.state_service().get_ledger_end().await?;
    assert!(end.value() >= 0, "ledger end offset should be >= 0");

    // get_latest_pruned_offsets: both fields are 0 on a fresh sandbox.
    let pruned = client.state_service().get_latest_pruned_offsets().await?;
    assert!(pruned.participant_pruned_up_to_inclusive.value() >= 0);

    // get_connected_synchronizers: returns the local "da" alias.
    let syncs = client
        .state_service()
        .get_connected_synchronizers(&alice, "", "")
        .await?;
    assert!(!syncs.is_empty(), "expected at least one connected synchronizer");

    // get_active_contracts_page: empty result is fine — alice hasn't
    // observed anything yet.
    let _page = client
        .state_service()
        .get_active_contracts_page(None, event_format_for(&alice), Some(50), None)
        .await?;

    // get_active_contracts (streaming): take a couple of items off
    // the stream and stop. `active_at_offset = BEGIN` is the
    // "everything in the ACS as of the start" sentinel.
    use futures::stream::StreamExt;
    let state_svc = client.state_service();
    let mut stream = state_svc
        .get_active_contracts(DamlLedgerOffset::BEGIN, event_format_for(&alice), None)
        .await?;
    // We don't expect alice to witness anything; just verify the
    // stream terminates cleanly (Canton closes the stream once it has
    // sent the ACS snapshot + checkpoint).
    let mut taken = 0usize;
    while let Some(_evt) = stream.next().await {
        taken += 1;
        if taken > 32 {
            break;
        }
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn command_service_submit_and_wait_variants() -> Result<()> {
    let client = connect().await?;
    ensure_dar_uploaded(&client, "cmd-svc-upload").await?;
    let alice = alloc_party(&client, "cmd-svc-alice").await?;
    let bob = alloc_party(&client, "cmd-svc-bob").await?;

    // submit_and_wait_for_transaction is exercised by the smoke test
    // and by create_asset(); this one focuses on the no-result
    // submit_and_wait variant.
    let create = DamlCommand::Create(DamlCreateCommand::new(
        asset_template_id(),
        asset_record(&alice, &bob, "REF-SAW", 7),
    ));
    let commands = command_factory(&alice, "cmd-svc-saw").make_command(create);
    let resp = client.command_service().submit_and_wait(commands).await?;
    println!("submit_and_wait completion_offset={:?}", resp.completion_offset);
    // skip: submit_and_wait_for_reassignment — needs multi-synchronizer
    // (and a tee party between two synchronizers). The single-domain
    // sandbox doesn't have one.
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn command_submission_service_submit_request() -> Result<()> {
    let client = connect().await?;
    ensure_dar_uploaded(&client, "cmd-sub-upload").await?;
    let alice = alloc_party(&client, "cmd-sub-alice").await?;
    let bob = alloc_party(&client, "cmd-sub-bob").await?;

    let create = DamlCommand::Create(DamlCreateCommand::new(
        asset_template_id(),
        asset_record(&alice, &bob, "REF-SUB", 1),
    ));
    let commands = command_factory(&alice, "cmd-sub-wf").make_command(create);
    let returned_command_id = client.command_submission_service().submit_request(commands).await?;
    assert!(!returned_command_id.is_empty(), "submit_request must echo the command_id");
    // skip: submit_reassignment — multi-synchronizer only.
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn command_completion_service_get_completion_stream() -> Result<()> {
    let client = connect().await?;
    ensure_dar_uploaded(&client, "cmd-cmp-upload").await?;
    let alice = alloc_party(&client, "cmd-cmp-alice").await?;
    let bob = alloc_party(&client, "cmd-cmp-bob").await?;

    // Snapshot the ledger end *before* submitting so the completion
    // stream's `begin_exclusive` is positioned ahead of any older
    // activity in this sandbox process.
    let before = client.state_service().get_ledger_end().await?;
    let create = DamlCommand::Create(DamlCreateCommand::new(
        asset_template_id(),
        asset_record(&alice, &bob, "REF-CMP", 1),
    ));
    let commands = command_factory(&alice, "cmd-cmp-wf").make_command(create);
    let command_id = commands.command_id.clone();
    client.command_submission_service().submit_request(commands).await?;

    // The stream sends a Completion for our just-submitted command;
    // grab the first item that mentions our command_id (or any item
    // and stop after one — Canton interleaves checkpoints).
    let completion_svc = client.command_completion_service();
    let mut stream = completion_svc
        .get_completion_stream(APP_ID, vec![alice.clone()], before)
        .await?;
    let mut saw_completion = false;
    let timeout = tokio::time::sleep(Duration::from_secs(15));
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            _ = &mut timeout => break,
            item = stream.next() => match item {
                Some(Ok(daml_grpc::data::completion::DamlCompletionResponse::Completion(c))) => {
                    println!("got completion command_id={} update_id={}", c.command_id, c.update_id);
                    if c.command_id == command_id {
                        saw_completion = true;
                        break;
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(anyhow!("completion stream error: {e:?}")),
                None => break,
            }
        }
    }
    assert!(saw_completion, "expected a completion for command_id={command_id} within 15s");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn command_inspection_service_get_command_status() -> Result<()> {
    let client = connect().await?;
    ensure_dar_uploaded(&client, "cmd-insp-upload").await?;
    let alice = alloc_party(&client, "cmd-insp-alice").await?;
    let bob = alloc_party(&client, "cmd-insp-bob").await?;

    // Submit something so a status row exists, then query by command-id
    // prefix. CommandInspectionService requires the participant's
    // command-inspection store to be enabled; on a stock sandbox it
    // may return UNIMPLEMENTED. Accept either outcome.
    let create = DamlCommand::Create(DamlCreateCommand::new(
        asset_template_id(),
        asset_record(&alice, &bob, "REF-INSP", 1),
    ));
    let commands = command_factory(&alice, "cmd-insp-wf").make_command(create);
    let command_id = commands.command_id.clone();
    let _ = client.command_service().submit_and_wait(commands).await;

    match client
        .command_inspection_service()
        .get_command_status(&command_id, DamlCommandState::Unspecified, 10)
        .await
    {
        Ok(rows) => {
            println!("command-inspection rows: {}", rows.len());
        }
        Err(e) if is_status(&e, tonic::Code::Unimplemented) => {
            println!("command inspection is not enabled on this participant (UNIMPLEMENTED), accepting");
        }
        Err(e) if is_status(&e, tonic::Code::FailedPrecondition) => {
            println!("command inspection is not enabled (FAILED_PRECONDITION), accepting");
        }
        Err(e) => return Err(anyhow!("unexpected error: {e:?}")),
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_service_all_methods() -> Result<()> {
    let client = connect().await?;
    ensure_dar_uploaded(&client, "upd-svc-upload").await?;
    let alice = alloc_party(&client, "upd-svc-alice").await?;
    let bob = alloc_party(&client, "upd-svc-bob").await?;

    let before = client.state_service().get_ledger_end().await?;
    let (_cid, update_id, offset) = create_asset(&client, &alice, &bob, "upd-svc-wf").await?;
    let after = client.state_service().get_ledger_end().await?;

    // get_update_by_id: fetch the transaction we just submitted.
    let by_id = client
        .update_service()
        .get_update_by_id(&update_id, update_format_for(&alice))
        .await?;
    assert!(matches!(by_id, DamlUpdate::Transaction(_)), "expected a Transaction update, saw {by_id:?}");

    // get_update_by_offset: same payload via offset.
    let by_offset = client
        .update_service()
        .get_update_by_offset(offset, update_format_for(&alice))
        .await?;
    assert!(matches!(by_offset, DamlUpdate::Transaction(_)));

    // get_updates_page: bounded paged read from `before` -> `after`.
    let page = client
        .update_service()
        .get_updates_page(
            Some(before),
            Some(after),
            Some(50),
            update_format_for(&alice),
            /* descending */ false,
            None,
        )
        .await?;
    assert!(!page.updates.is_empty(), "expected at least one update in [before, after]");

    // get_updates (streaming): bounded `[before, after]`. Drain the
    // stream and assert the create's update_id appears.
    use futures::StreamExt;
    let update_svc = client.update_service();
    let mut stream = update_svc
        .get_updates(before, Some(after), update_format_for(&alice), false)
        .await?;
    use daml_grpc::data::update::DamlUpdateResponse;
    let mut saw = false;
    while let Some(item) = stream.next().await {
        let resp = item?;
        if let DamlUpdateResponse::Update(DamlUpdate::Transaction(t)) = resp {
            if t.update_id == update_id {
                saw = true;
            }
        }
    }
    assert!(saw, "streamed updates didn't include our create's update_id");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn contract_service_get_contract() -> Result<()> {
    let client = connect().await?;
    ensure_dar_uploaded(&client, "ctr-svc-upload").await?;
    let alice = alloc_party(&client, "ctr-svc-alice").await?;
    let bob = alloc_party(&client, "ctr-svc-bob").await?;
    let (cid, _, _) = create_asset(&client, &alice, &bob, "ctr-svc-wf").await?;

    let created = client
        .contract_service()
        .get_contract(&cid, vec![alice.clone(), bob.clone()])
        .await?;
    assert_eq!(created.contract_id, cid, "fetched contract id should round-trip");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn event_query_service_get_events_by_contract_id() -> Result<()> {
    let client = connect().await?;
    ensure_dar_uploaded(&client, "evt-svc-upload").await?;
    let alice = alloc_party(&client, "evt-svc-alice").await?;
    let bob = alloc_party(&client, "evt-svc-bob").await?;
    let (cid, _, _) = create_asset(&client, &alice, &bob, "evt-svc-wf").await?;

    let events = client
        .event_query_service()
        .get_events_by_contract_id(&cid, event_format_for(&alice))
        .await?;
    assert!(events.created.is_some(), "fresh contract should have a create event");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn time_service_get_and_set() -> Result<()> {
    let client = connect().await?;
    // The stock canton-sandbox runs in wall-clock mode where
    // TimeService is unavailable; Canton returns UNIMPLEMENTED /
    // FAILED_PRECONDITION. On a static-time participant both calls
    // succeed and we round-trip a small advance.
    match client.time_service().get_time().await {
        Ok(now) => {
            let advance = now + ChronoDuration::seconds(1);
            client.time_service().set_time(now, advance).await?;
            let read_back = client.time_service().get_time().await?;
            assert!(read_back >= advance, "set_time should have advanced the clock");
        }
        Err(e)
            if is_status(&e, tonic::Code::Unimplemented)
                || is_status(&e, tonic::Code::FailedPrecondition) =>
        {
            println!("TimeService unavailable on wall-clock sandbox ({e}); accepting");
        }
        Err(e) => return Err(anyhow!("unexpected TimeService error: {e:?}")),
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn participant_pruning_service_prune() -> Result<()> {
    let client = connect().await?;
    // Pruning has tight preconditions (offset must lie strictly
    // before ledger end, after the configured pruning delay).
    // `prune_up_to = 0` is rejected as out-of-range; we expect an
    // INVALID_ARGUMENT / FAILED_PRECONDITION style response that
    // still exercises the RPC end-to-end.
    let result = client
        .participant_pruning_service()
        .prune(DamlLedgerOffset::BEGIN, "prune-smoke", false)
        .await;
    match result {
        Ok(()) => println!("prune accepted at BEGIN (no-op participant)"),
        Err(e)
            if is_status(&e, tonic::Code::InvalidArgument)
                || is_status(&e, tonic::Code::FailedPrecondition)
                || is_status(&e, tonic::Code::Unimplemented) =>
        {
            println!("prune rejected as expected on a fresh sandbox: {e}");
        }
        Err(e) => return Err(anyhow!("unexpected prune error: {e:?}")),
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn identity_provider_config_service_lifecycle() -> Result<()> {
    let client = connect().await?;
    let idp_id = format!("idp-test-{}", unique_tag());

    let created = client
        .identity_provider_config_service()
        .create_identity_provider_config(DamlIdentityProviderConfig {
            identity_provider_id: idp_id.clone(),
            is_deactivated: true, // deactivated so it can't accept tokens
            issuer: format!("https://issuer.example/{idp_id}"),
            jwks_url: "https://example.invalid/jwks".to_owned(),
            audience: "test-audience".to_owned(),
        })
        .await?;
    assert_eq!(created.identity_provider_id, idp_id);

    let fetched = client
        .identity_provider_config_service()
        .get_identity_provider_config(&idp_id)
        .await?;
    assert_eq!(fetched.issuer, format!("https://issuer.example/{idp_id}"));

    let listed = client.identity_provider_config_service().list_identity_provider_configs().await?;
    assert!(listed.iter().any(|c| c.identity_provider_id == idp_id), "IDP should appear in list");

    let updated = client
        .identity_provider_config_service()
        .update_identity_provider_config(
            DamlIdentityProviderConfig {
                identity_provider_id: idp_id.clone(),
                is_deactivated: true,
                issuer: format!("https://issuer.example/{idp_id}"),
                jwks_url: "https://example.invalid/jwks-updated".to_owned(),
                audience: "test-audience-2".to_owned(),
            },
            ["jwks_url".to_owned(), "audience".to_owned()],
        )
        .await?;
    assert_eq!(updated.audience, "test-audience-2");

    client.identity_provider_config_service().delete_identity_provider_config(&idp_id).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_management_service_lifecycle() -> Result<()> {
    let client = connect().await?;
    let alice = alloc_party(&client, "user-mgmt-alice").await?;
    let user_id = format!("user-test-{}", unique_tag());

    // create_user with one initial right.
    let created = client
        .user_management_service()
        .create_user(
            DamlUser {
                id: user_id.clone(),
                primary_party: alice.clone(),
                is_deactivated: false,
                metadata: None,
                identity_provider_id: String::new(),
                primary_party_authentication: false,
            },
            [DamlUserRight::CanActAs(alice.clone())],
        )
        .await?;
    assert_eq!(created.id, user_id);

    // get_user
    let fetched = client.user_management_service().get_user(&user_id, "").await?;
    assert_eq!(fetched.primary_party, alice);

    // update_user: deactivate.
    let updated = client
        .user_management_service()
        .update_user(
            DamlUser {
                id: user_id.clone(),
                primary_party: alice.clone(),
                is_deactivated: true,
                metadata: None,
                identity_provider_id: String::new(),
                primary_party_authentication: false,
            },
            ["is_deactivated".to_owned()],
        )
        .await?;
    assert!(updated.is_deactivated);

    // list_users: page size 1000 is plenty for a sandbox.
    let page = client.user_management_service().list_users("", 1000, "").await?;
    assert!(page.users.iter().any(|u| u.id == user_id), "list_users should include our test user");

    // grant_user_rights: add ParticipantAdmin.
    let _ = client
        .user_management_service()
        .grant_user_rights(&user_id, [DamlUserRight::ParticipantAdmin], "")
        .await?;

    // list_user_rights
    let rights = client.user_management_service().list_user_rights(&user_id, "").await?;
    assert!(rights.iter().any(|r| matches!(r, DamlUserRight::ParticipantAdmin)));

    // revoke_user_rights
    let _ = client
        .user_management_service()
        .revoke_user_rights(&user_id, [DamlUserRight::ParticipantAdmin], "")
        .await?;

    // update_user_identity_provider_id: default→default is a no-op
    // that exercises the RPC without needing a second IDP.
    client.user_management_service().update_user_identity_provider_id(&user_id, "", "").await?;

    // delete_user
    client.user_management_service().delete_user(&user_id, "").await?;
    Ok(())
}
