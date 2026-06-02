//! Build-script-driven Daml codegen for the demo binary.
//!
//! Runs `daml::codegen::generator::daml_codegen` over the Phase 7
//! LF2 fixture DAR (`TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar`)
//! and writes the generated Rust modules to `src/autogen/`. The
//! demo's `main.rs` includes the entry-point module via `include!`.
//!
//! Re-running `cargo build` after the DAR changes regenerates the
//! module tree; the `cargo:rerun-if-changed` line below tells
//! cargo to do that.

use daml::codegen::generator::{daml_codegen, ModuleOutputMode, RenderMethod};

const DAR_PATH: &str = "../../daml-lf/test_resources/TestingTypes-3_0_0-sdk_3_4_11-lf_2_1.dar";
const OUTPUT_PATH: &str = "src/autogen";

fn main() {
    println!("cargo:rerun-if-changed={DAR_PATH}");
    // Filter codegen to the fixture's own modules. Without this
    // the renderer would also emit Rust types for every dalf
    // dependency (daml-stdlib / daml-prim, GHC.* internals, etc.)
    // — most of which the type renderer doesn't handle gracefully
    // (recursive types in `CallStack` / `Down` / similar).
    daml_codegen(
        DAR_PATH,
        OUTPUT_PATH,
        // Fuji.Asset is the only module whose types this demo
        // exercises directly; Fuji.Types is brought in so the
        // variant `Shape` (with record-payload constructors
        // `Circle` / `Rectangle`) lands in the generated tree, and
        // DA.Internal.Template is included so the generated
        // `archive_command` / `holding_archive_command` signatures
        // (which take a `DA.Internal.Template.Archive`) can be
        // rendered.
        &["^Fuji\\.Asset$", "^Fuji\\.Types$", "^DA\\.Internal\\.Template$"],
        RenderMethod::Full,
        ModuleOutputMode::Combined,
    )
    .expect("failed to generate code for Daml archive");
}
