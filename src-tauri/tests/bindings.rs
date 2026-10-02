//! The renderer's TypeScript bindings are generated from the Rust IPC layer.
//!
//! This test is the gate. It exports the real bindings to a temporary file and
//! compares that file with the checked-in one. A hand edit, a changed command
//! signature, or a renamed event fails here rather than at runtime in the app.
#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]

use std::path::PathBuf;

use quota_desktop_lib::ipc::bindings::{DEFAULT_DESTINATION, export_to, registry};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_committed_bindings_match_what_the_exporter_produces() {
    let generated = crate_dir().join("../src/generated/bindings.generated.ts");
    let exported = export_to(&registry(), &generated);
    assert!(
        exported.is_ok(),
        "the exporter must be able to write its output: {}",
        exported.err().map_or(String::new(), |error| error.detail)
    );

    let expected = crate_dir().join(DEFAULT_DESTINATION);
    let produced =
        std::fs::read_to_string(&generated).expect("generated bindings must be readable");
    let committed =
        std::fs::read_to_string(&expected).expect("committed bindings must be readable");

    assert_eq!(
        produced, committed,
        "the committed bindings are stale; `bun tauri icon`-style regeneration replaces them \
         and nothing in that file is edited by hand"
    );

    std::fs::remove_file(&generated).expect("the temporary file must be removable");
}
