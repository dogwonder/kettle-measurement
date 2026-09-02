//! #596: the backend a sidecar answered on is provenance, not a note.
//!
//! Two runtimes on the same build and weights disagreed on 53 of 852
//! passages; two runs on one runtime were byte-identical. So a device
//! string carries two facts: which backend (the line a comparison may
//! not cross) and which card (unmeasured, said out loud). This is the
//! reading of the first.

use runner::eval::SidecarInfo;

fn on(device: Option<&str>) -> SidecarInfo {
    SidecarInfo {
        version: "10145 (ad256ded3)".to_owned(),
        file: "llama-server".to_owned(),
        device: device.map(str::to_owned),
    }
}

#[test]
fn the_backend_is_the_device_name_without_its_index_or_card() {
    assert_eq!(on(Some("MTL0 (Apple M1 Pro)")).backend(), Some("MTL"));
    assert_eq!(
        on(Some("CUDA0 (NVIDIA GeForce RTX 5090)")).backend(),
        Some("CUDA")
    );
    assert_eq!(on(Some("CPU")).backend(), Some("CPU"));
}

#[test]
fn several_cards_on_one_backend_are_one_backend() {
    assert_eq!(
        on(Some("CUDA0 (NVIDIA A100), CUDA1 (NVIDIA A100)")).backend(),
        Some("CUDA")
    );
}

#[test]
fn an_unrecorded_device_has_no_backend_to_claim() {
    // `None` means not recorded, never "no accelerator" — a CPU run
    // says "CPU" — so there is nothing here to compare against.
    assert_eq!(on(None).backend(), None);
}
