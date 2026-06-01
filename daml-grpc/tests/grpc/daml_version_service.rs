use crate::common::ping_pong::{initialize_wallclock, new_wallclock_sandbox, TestResult};

#[tokio::test]
async fn test_get_version() -> TestResult {
    let _lock = initialize_wallclock().await;
    let ledger_client = new_wallclock_sandbox().await?;
    let (version, features) = ledger_client.version_service().get_ledger_api_version().await?;
    assert!(!version.is_empty(), "participant must report a version");
    let features = features.expect("v2 participants always return a features descriptor");
    // Sanity-check a few feature fields the Canton 3.5.x sandbox is known to
    // populate; this catches a wholly empty FeaturesDescriptor without
    // overspecifying the exact participant configuration.
    assert!(features.user_management.is_some());
    assert!(features.party_management.is_some());
    Ok(())
}
