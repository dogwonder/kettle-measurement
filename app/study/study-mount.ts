// #431's harness entry. Named `study-mount.ts` rather than `main.ts`:
// the product has one of those and the demo refuses a filename shared
// with `src/`, because a name collision is how a copy starts.
//
// The stylesheets are the product's, imported rather than duplicated.
import { mount } from "svelte";
import "../src/tokens.css";
import "../src/app.css";
import "../src/styles/govuk-frontend.scss";
import StudyApp from "./StudyApp.svelte";

// `?material=statements` runs the statement track; anything else is
// the letter track, the default since 27 August 2026.
const query = new URLSearchParams(window.location.search);
const material = query.get("material") === "statements" ? "statements" : "letters";
// `?participant=a01` is how the id the script issued reaches the screen.
// Without it the box suggested `p01` and that is what got typed, so the
// author's first sitting came out claiming to be one of the twenty.
const participant = query.get("participant");

export default mount(StudyApp, {
  target: document.getElementById("study")!,
  props: { material, participant },
});
