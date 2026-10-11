// lib.typ — shared helpers for the ok-fpga-server API Specification.
//
// Conventions for authors (internal; never shown in the PDF):
// - Every normative paragraph, list or table carries a Typst comment
//   `// ref: ADR-0NN §x; Dnn` pointing at the internal design record
//   (docs/architecture/adr/, docs/resource-management-recommendations.md).
//   The visible text NEVER mentions ADRs, decision ids or phase names.
// - Every heading has a label (`<res-declare>`) so the OpenAPI / proto
//   files can later cite sections by number ("see spec §3.2.1").
// - Normative keywords MUST / MUST NOT / SHOULD / SHOULD NOT / MAY follow
//   RFC 2119.

#let accent = rgb("#1f4e79")
#let note-fill = rgb("#eef4fb")
#let caution-fill = rgb("#fdf1e7")
#let example-fill = rgb("#f3f7ef")

/// A boxed note with a coloured left rule.
#let _callout(label, fill, stroke-color, body) = block(
  width: 100%,
  fill: fill,
  inset: (x: 10pt, y: 8pt),
  radius: 3pt,
  stroke: (left: 3pt + stroke-color),
  breakable: true,
)[#text(weight: "bold", fill: stroke-color)[#label] #body]

/// Background information or a helpful explanation.
#let note(body) = _callout("Note.", note-fill, accent, body)

/// A trap that developers fall into; read carefully.
#let caution(body) = _callout("Caution.", caution-fill, rgb("#b34700"), body)

/// A worked example with a title.
#let example(title, body) = block(
  width: 100%,
  fill: example-fill,
  inset: (x: 10pt, y: 8pt),
  radius: 3pt,
  stroke: 0.5pt + rgb("#7a9a5a"),
  breakable: true,
)[#text(weight: "bold", fill: rgb("#3d5a1e"))[Example: #title] \ #body]

/// Link to the Opal Kelly FrontPanel API documentation.
#let ok-api = link(
  "https://library.opalkelly.com/library/FrontPanelAPI/classOpalKelly_1_1FPGADataPortClassic.html",
)[FrontPanel API reference, `FPGADataPortClassic`]
