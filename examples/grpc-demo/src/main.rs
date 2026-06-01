//! Daml v2 gRPC API demo (placeholder).
//!
//! The v1 implementation of this demo (PingPong via
//! `TransactionService.GetTransactions` + sandbox JWT auth + reset)
//! was retired in the v2 migration. A full v2 rewrite uses:
//!
//! - [`DamlCommandService::submit_and_wait`] for fire-and-forget
//!   submissions, returning `DamlSubmitAndWaitOutcome` with the
//!   resulting `update_id` and `completion_offset`.
//! - [`DamlUpdateService::get_updates`] to subscribe to a stream of
//!   transactions / reassignments / topology updates scoped by
//!   `DamlTransactionFormat` (ACS-delta or LedgerEffects shape).
//! - [`DamlStateService::get_active_contracts`] for the initial
//!   ACS snapshot at a given offset.
//! - [`DamlCommandFactory`] to build the `Commands` envelope with
//!   the workflow-id / application-id / parties / dedup window.
//! - Identifier addressing via package-name (`#<package-name>`)
//!   rather than package-id hash; see
//!   [`daml_grpc::data::identifier::DamlIdentifier::from_package_name`].
//!
//! The Phase 7 LF2 DAR fixture and the Phase 8 integration harness
//! together will provide everything this demo needs to run end-to-end
//! against the local Canton sandbox (`canton-sandbox` in the
//! `nix develop` shell).

fn main() {
    println!("grpc-demo is a v2 rewrite stub. See module docs.");
}
