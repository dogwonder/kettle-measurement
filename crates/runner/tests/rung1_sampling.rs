//! #568 rung 1: the parts of the instrument that decide the denominator.
//!
//! The measurement this backs is a rate — 1 invented claim in 470
//! judgeable ones, 0.21%, Wilson [0.04%, 1.20%] — and it closed #568
//! and sent #474 to option 3. A rate is only as good as its
//! denominator, and every function here is upstream of that number:
//! which passages are asked about, how they are grouped into calls, and
//! how an answer is joined back to the passage it is about.
//!
//! Each of these can fail *quietly*. A batcher that dropped a passage
//! shrinks the denominator; a join that missed would drop claims from
//! the numerator and the denominator together; a prompt that omitted a
//! passage would produce fewer answers than were counted. None of them
//! would raise anything at run time, and the run costs 86 minutes of
//! GPU, so the failure would be found — if at all — by hand-reading the
//! output afterwards.
//!
//! The example itself needs weights and a sidecar and is not testable
//! here. Its pure half is, and is where the arithmetic lives.

#[path = "../examples/rung1/mod.rs"]
mod rung1;

use rung1::{
    batches, chunks, closed_prompt, closed_schema, passage_id, prose_schema, worth_asking,
};

/// Passages shaped like the ones a filed set of accounts produces.
fn passages(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|i| {
            (
                i.to_string(),
                format!(
                    "Total income for the year was £{},{:03}.",
                    i + 1,
                    i * 7 % 1000
                ),
            )
        })
        .collect()
}

#[test]
fn every_passage_reaches_exactly_one_batch_in_the_order_it_was_read() {
    let all = passages(40);
    let grouped = batches(all.clone(), 300);

    assert!(grouped.len() > 1, "the budget has to actually split these");

    let flat: Vec<(String, String)> = grouped.iter().flatten().cloned().collect();
    // Not "the same set": order is what the id join and the hand-read
    // both rely on, and a batcher that reordered would still pass a
    // set comparison.
    assert_eq!(
        flat, all,
        "batching dropped, duplicated or reordered a passage"
    );
}

#[test]
fn a_passage_bigger_than_the_budget_travels_whole_rather_than_being_cut() {
    // The alternative is asking the model about text the document does
    // not contain, which is the one thing this run exists to count.
    let long = "The trustees consider the reserves policy ".repeat(200);
    let all = vec![
        ("1".to_owned(), "Total income was £381,290.".to_owned()),
        ("2".to_owned(), long.clone()),
        ("3".to_owned(), "Total expenditure was £318,734.".to_owned()),
    ];

    let grouped = batches(all, 100);
    let oversized: Vec<&(String, String)> = grouped
        .iter()
        .flatten()
        .filter(|(id, _)| id == "2")
        .collect();

    assert_eq!(oversized.len(), 1, "the long passage was split or dropped");
    assert_eq!(oversized[0].1, long, "the long passage was truncated");
}

#[test]
fn chunking_prose_keeps_every_line_once_and_in_order() {
    let text = (0..200)
        .map(|i| format!("line {i} of the statement of financial activities"))
        .collect::<Vec<_>>()
        .join("\n");

    let pieces = chunks(&text, 400);
    assert!(pieces.len() > 1, "the budget has to actually split this");

    let rejoined: Vec<&str> = pieces.iter().flat_map(|piece| piece.lines()).collect();
    let original: Vec<&str> = text.lines().collect();
    assert_eq!(
        rejoined, original,
        "chunking lost, duplicated or moved a line"
    );
}

#[test]
fn the_passage_id_is_the_number_however_the_model_echoed_it() {
    // The model echoes the id as it saw it printed, brackets and all,
    // and judging joins on the number. This was a real defect: a join
    // on the echoed string drops every claim whose id came back
    // decorated, and drops it from numerator and denominator together,
    // so the rate stays plausible while the count silently falls.
    for echoed in ["306", "[306]", "passage 306", " 306 ", "[306]:"] {
        assert_eq!(passage_id(echoed), "306", "{echoed} did not join");
    }
    assert_eq!(passage_id("none"), "", "a number-free id is not a passage");
}

#[test]
fn a_passage_is_asked_about_when_it_carries_a_figure_or_qualifies_one() {
    // The sampling rule, pre-registered before the corpus was fetched.
    // It is the denominator's definition, so it belongs in a test
    // rather than in a sentence on the issue.
    assert!(worth_asking("Total income for the year was £381,290."));
    // No digit, but it is exactly the kind of sentence that decides how
    // a figure should be read — which is the error class this run found.
    assert!(worth_asking("The trustees have adopted a reserves policy."));
    assert!(worth_asking(
        "The accounts are prepared on a going concern basis."
    ));
    // A heading carries no fact to ask about.
    assert!(!worth_asking("Notes"));
    assert!(!worth_asking("31 March"), "two words is not a statement");
    assert!(!worth_asking(
        "The charity continued its work throughout the year."
    ));
}

#[test]
fn the_closed_prompt_asks_about_every_passage_in_its_batch() {
    // Fewer passages in the prompt than in the batch means fewer
    // answers than the denominator counted, and nothing at run time
    // would say so.
    let batch = passages(6);
    let prompt = closed_prompt(&batch);
    for (id, text) in &batch {
        assert!(
            prompt.contains(&format!("[{id}]")),
            "passage {id} not asked"
        );
        assert!(prompt.contains(text), "passage {id}'s text not sent");
    }
}

#[test]
fn both_schemas_are_grammar_safe_without_a_sidecar() {
    // The example asserts this too, but only after spawning a sidecar
    // and loading the weights — ten minutes into a run that costs
    // eighty-six. Here it costs nothing.
    runner::exec::assert_grammar_safe(&closed_schema()).expect("closed schema");
    runner::exec::assert_grammar_safe(&prose_schema()).expect("prose schema");
}
