//! #630: pin the API used by kettle-mini's bridge at f57cbe9.
//!
//! These are compile-time consumer signatures, checked without the sibling
//! checkout, weights, a sidecar process or network access. Update deliberately
//! with the consumer when changing this boundary. They establish source
//! compatibility only, not Mini's behaviour or reading accuracy.

use runner::document::{read_document_parts_limited, DocumentRead, Segment};
use runner::exec::{call_constrained, Endpoint, ModelCallError};
use runner::parse::ParseError;
use runner::reading::{check, Checked, Kind, Reading};
use runner::sidecar::{binary_in, Sidecar, SidecarRuntime};
use serde_json::Value;
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

type ReadParts =
    fn(&[&Path], usize, Option<&Path>, Option<usize>) -> Result<DocumentRead, ParseError>;
type ConstrainedCall = fn(&Endpoint, &str, &Value, &AtomicBool) -> Result<Value, ModelCallError>;
type CheckReading = fn(&Reading, Kind, &Segment, &BTreeSet<usize>, &[Segment]) -> Checked;
type SpawnSidecar = fn(&Path, &Path, &Path, SidecarRuntime) -> io::Result<Sidecar>;

#[test]
fn mini_can_read_ordered_parts_with_a_physical_page_limit() {
    let _: ReadParts = read_document_parts_limited;
}

#[test]
fn mini_can_make_a_cancellable_constrained_call() {
    let _: fn(u16) -> Endpoint = Endpoint::local;
    let _: ConstrainedCall = call_constrained;
}

#[test]
fn mini_can_check_a_reading_against_shown_passages() {
    let _: CheckReading = check;
}

#[test]
fn mini_can_start_and_wait_for_a_local_sidecar() {
    let _: fn(&Path) -> PathBuf = binary_in;
    let _: fn() -> SidecarRuntime = SidecarRuntime::default;
    let _: SpawnSidecar = Sidecar::spawn;
    let _: fn(&mut Sidecar, Duration) -> io::Result<()> = Sidecar::wait_until_ready;
    let _: fn(&Sidecar) -> u16 = Sidecar::port;
}
