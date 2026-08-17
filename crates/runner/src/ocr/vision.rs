//! The Apple Vision binding (#399), and nothing else.
//!
//! Deliberately thin. Everything worth reviewing — when a reading is
//! too poor to trust, what a person is told — is in the parent module,
//! pure and tested in CI. This file turns a file path into
//! [`super::Reading`] and has no opinions.
//!
//! Vision was chosen over Tesseract because the corpus is photographs
//! of paper rather than scans of digital files: it is materially better
//! on a page held at an angle under room light, it needs no vendored
//! binary, and it reports a confidence per observation, which is the
//! signal #399's second half needs. It runs on-device; nothing is
//! uploaded. macOS-only is the accepted cost (#71 keeps the portable
//! path).

use super::{Correction, Line, OcrError, Placed, Reading};
use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_foundation::{NSDictionary, NSString, NSURL};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation,
    VNRequestTextRecognitionLevel,
};
use std::path::Path;

/// Read every line of text Vision finds in one picture.
pub fn recognise(path: &Path, correction: Correction) -> Result<Reading, OcrError> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));

    let request: Retained<VNRecognizeTextRequest> = VNRecognizeTextRequest::new();
    // Accurate, not fast: a wrong digit in a date is the failure this
    // pack cannot have, and a letter is one page — there is no
    // throughput to trade against.
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    // Vision's language correction fixes exactly the class of error the
    // first real letter showed (`aim` read as `alm`), using the
    // system's own dictionaries, on-device.
    request.setUsesLanguageCorrection(correction == Correction::Applied);

    let requests = objc2_foundation::NSArray::from_retained_slice(&[Retained::into_super(
        Retained::into_super(request.clone()),
    )]);

    let handler = unsafe {
        VNImageRequestHandler::initWithURL_options(
            VNImageRequestHandler::alloc(),
            &url,
            &NSDictionary::new(),
        )
    };

    handler
        .performRequests_error(&requests)
        .map_err(|error| OcrError::Unreadable(error.localizedDescription().to_string()))?;

    let Some(results) = request.results() else {
        return Ok(Reading { lines: vec![] });
    };

    // Position travels with the text, and the parent module decides
    // what to do with it: reading order and one-line-per-line are
    // judgements, testable on every machine, and this file has no
    // opinions (see `Reading::from_placed`).
    let mut placed = Vec::new();
    for observation in results.iter() {
        let observation: &VNRecognizedTextObservation = &observation;
        // One candidate: Vision ranks them, and a second-choice reading
        // of a letter is a guess by another name. If the best it has is
        // poor, the parent module's floor is what should decide, not a
        // fallback that hides how poor it was.
        let candidates = observation.topCandidates(1);
        let Some(best) = candidates.iter().next() else {
            continue;
        };
        // Safe in practice: a text observation always carries a box.
        let box_ = unsafe { observation.boundingBox() };
        placed.push(Placed {
            top: box_.origin.y + box_.size.height,
            left: box_.origin.x,
            right: box_.origin.x + box_.size.width,
            line: Line {
                text: best.string().to_string(),
                confidence: best.confidence(),
                top: box_.origin.y + box_.size.height,
                left: box_.origin.x,
            },
        });
    }

    Ok(Reading::from_placed(placed))
}
