"""Adapt public UKHSA templates with fictitious placeholders; no Kettle generator.

Run with Python, reportlab and pypdfium2. Text is authored by UKHSA;
selection, factual annotation and the plain PDF layout are authored here.
No model answers are inputs. Sources are the downloaded originals beside this file.
"""
from html import escape
import hashlib
import importlib.metadata
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
import zipfile

from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.pagesizes import A4
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer
import pypdfium2 as pdfium

ROOT = Path(__file__).resolve().parent
NS = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
SOURCES = [
    ("ukhsa-12-months", 15, 31, "text",
     "https://assets.publishing.service.gov.uk/media/6978aad6d6ab92f1d3a4d685/Invitation_letter_to_children_aged_12_months_template.docx"),
    ("ukhsa-18-months", 15, 29, "pdf",
     "https://assets.publishing.service.gov.uk/media/694ac8f93022cdf03a0eb92e/Vaccination_invitation_letter_children_aged_18_months_old_template_final.docx"),
    ("ukhsa-preschool", 16, 32, "image",
     "https://assets.publishing.service.gov.uk/media/694ac91f1a2e540ccd8a551f/Pre-school_invitation_letter_children_aged_3_years_four_months_old_template.docx"),
]


def sha(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def paragraphs(path, first, last):
    with zipfile.ZipFile(path) as z:
        root = ET.fromstring(z.read("word/document.xml"))
    original = ["".join(t.text or "" for t in p.findall(".//w:t", NS)).strip()
                for p in root.findall(".//w:p", NS)]
    replacements = {"«Insert child’s name»": "Robin", "«Insert child’s name »": "Robin",
                    "«Insert child’s first name»": "Robin", "[insert number]": "01632 960123",
                    "[GP/Practice Nurse/Practice Manager name]": "Rowan Vale",
                    "[Position/title]": "Practice Manager"}
    body = []
    for text in original[first:last + 1]:
        for old, new in replacements.items():
            text = text.replace(old, new)
        if text:
            body.append(" ".join(text.split()))
    return ["Brookmere Practice", "8 September 2026", "Dear Alex", *body,
            "As a reminder, you can use this section to record the date and time of your child’s vaccination appointment:",
            "on: _____/_____/_____ at _________am/pm"]


def generate(out):
    out.mkdir(parents=True, exist_ok=False)
    cases, facts, bindings = [], [], {}
    for ident, first, last, arm, locator in SOURCES:
        source = ROOT / "sources" / (ident + ".docx")
        text = paragraphs(source, first, last)
        # This blank reminder is a form to fill after booking, not an
        # appointment already made. Optional advice depends on facts/preferences
        # the letter cannot settle. The actual ask is to phone to arrange one.
        asks = [{"id": ident + "-book", "kind": "response", "status": "obligation",
                 "passage": 3, "deadline_at": 3, "deadline_words": "",
                 "deadline_read": {"count": 0, "unit": "none", "qualifier": "none", "counts_from": "none"},
                 "fields": {"party": "practice", "deadline": None, "amount": None}}]
        for i, passage in enumerate(text):
            if i == 3:
                continue
            asks.append({"id": f"{ident}-negative-{i}", "kind": "response", "status": "no-obligation",
                         "passage": i, "fields": {}, "reason": "Background, optional information/conditional advice, signature or a blank reminder; no additional current ask."})
        case = {"id": ident, "document_kind": "letter", "passages": text,
                "spans": [{"fact": "practice", "passage": 0, "text": text[0]}], "asks": asks}
        cases.append(case)
        (out / (ident + ".txt")).write_text("\n\n".join(text) + "\n")
        pdf = out / (ident + ".pdf")
        style = ParagraphStyle("body", fontName="Helvetica", fontSize=11, leading=15, spaceAfter=10)
        SimpleDocTemplate(str(pdf), pagesize=A4, leftMargin=48, rightMargin=48,
                          topMargin=48, bottomMargin=48, invariant=1).build(
            [Paragraph(escape(p), style) for p in text])
        pages = pdfium.PdfDocument(pdf)
        if len(pages) > 3:
            raise ValueError("challenge exceeds the shipped letter page scope")
        images = []
        for i, page in enumerate(pages):
            name = f"{ident}-page-{i+1}.png"
            page.render(scale=2).to_pil().save(out / name)
            images.append(name)
        # The image is a clean raster scan, not a claimed camera photograph.
        files = {"text": [ident + ".txt"], "pdf": [ident + ".pdf"], "image": images}[arm]
        bindings[ident] = {"inputs": {"letter": files}}
    corpus = {"schema_version": 1, "slice": "external-ukhsa-01",
              "selection": {"id": "external-ukhsa-invitations-2026-09-08", "purpose": "challenge",
                            "exposure": "unexposed", "fields": ["kind", "party", "deadline", "amount"]},
              "facts": [{"id": "practice", "field": "party", "status": "stated",
                         "value": {"kind": "text", "text": "Brookmere Practice"}}],
              "cases": cases, "relations": [], "provenance": {
                  "synthetic": True, "independent": True,
                  "authoring": {"author": "UK Health Security Agency (letter wording); Codex (placeholder adaptation, annotations and rendering)",
                                "relationship": "separate-author",
                                "source_families": [{"id": "ukhsa-childhood-vaccination-invitations", "relationship": "external-source",
                                    "locator": "UKHSA published invitation templates, 24 December 2025 / 27 January 2026; original DOCX files pinned in generation.json"}]},
                  "limitations": "One external wording family, three related templates. Kettle's developer authors the annotations and plain layout; not independent adjudication, a new camera layout family or proof of generalisation. Blank reminders normalised to one wording; no clinical recommendations evaluated."}}
    write = lambda name, value: (out / name).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")
    write("corpus.json", corpus)
    write("bindings.json", bindings)
    write("generation.json", {"schema": "kettle/external-template-generation@1",
        "generator": sha(Path(__file__)), "sources": [{"file": ident + ".docx", "url": url,
            "digest": sha(ROOT / "sources" / (ident + ".docx"))} for ident, _, _, _, url in SOURCES],
        "licence": "UKHSA Crown copyright, Open Government Licence v3.0; no logos reproduced",
        "dependencies": {p: importlib.metadata.version(p) for p in ("reportlab", "pypdfium2")},
        "files": {p.name: sha(p) for p in sorted(out.iterdir()) if p.is_file()}})


if __name__ == "__main__":
    generate(Path(sys.argv[1]))
