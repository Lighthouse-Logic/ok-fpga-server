// main.typ — ok-fpga-server API Specification (entry point).
//
// Build (from docs/API-Specification/):
//   typst compile main.typ ok-fpga-server-api-specification.pdf
//
// Internal traceability lives in `// ref:` comments only (ADR numbers,
// decision ids D1–D78, roadmap phases). The rendered PDF must never show
// them. Section numbers are stable anchors: the OpenAPI and proto files
// will later cite them (roadmap 🔵 Later item).

#import "lib.typ": *

#let doc-version = "1.0.0"
#let spec-version = "0.3.10"
#let spec-date = "2026-11-06"

#set document(
  title: "ok-fpga-server API Specification",
  author: "ok-fpga-server project",
)
#set page(
  paper: "us-letter",
  margin: (x: 1in, y: 1in),
  numbering: "1",
  header: context {
    if counter(page).get().first() > 1 [
      #set text(size: 8pt, fill: gray)
      ok-fpga-server API Specification #h(1fr) Doc v#doc-version (Server v#spec-version)
    ]
  },
)
#set text(font: ("Libertinus Serif", "New Computer Modern"), size: 10.5pt)
#set par(justify: true)
#show raw: set text(font: ("DejaVu Sans Mono", "Liberation Mono"), size: 8.5pt)
#show raw.where(block: true): set text(size: 7.5pt)
#show raw.where(block: true): block.with(
  fill: luma(245),
  inset: 8pt,
  radius: 3pt,
  width: 100%,
)
#show link: set text(fill: accent)
#set heading(numbering: "1.1.1")
#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  set text(size: 17pt, fill: accent)
  block(below: 1em, it)
}
#show heading.where(level: 2): set text(size: 13pt, fill: accent)
#set table(stroke: 0.5pt + luma(180), inset: 5pt)
#show table: set par(justify: false)
#show table: set text(size: 9pt)
#show table.cell.where(y: 0): set text(weight: "bold")

// ---------------------------------------------------------------- title page
#page(numbering: none, header: none)[
  #v(2in)
  #align(center)[
    #text(size: 26pt, weight: "bold", fill: accent)[ok-fpga-server]
    #v(4pt)
    #text(size: 18pt)[REST and gRPC API Specification]
    #v(1.5em)
    #text(size: 12pt)[Developer reference for the OpenAPI and Protocol Buffers interfaces]
    #v(3em)
    #text(size: 11pt)[Document Version #doc-version #h(1em) | #h(1em) Target Server #spec-version #h(1em) | #h(1em) #spec-date]
    #v(1em)
    #text(size: 10pt, fill: gray)[
      Complete specification of the Server Overview, Resource Model, Access
      Enforcement and Worked Examples.
    ]
  ]
]

#outline(title: [Contents], depth: 3, indent: auto)

// ---------------------------------------------------------------- body
#include "chapters/introduction.typ"
#include "chapters/overview.typ"
#include "chapters/resource-model.typ"
#include "chapters/enforcement.typ"
#include "chapters/example-dual-port.typ"

// ---------------------------------------------------------------- appendices
#counter(heading).update(0)
#set heading(numbering: "A.1.1", supplement: [Appendix])
#include "chapters/appendix-status.typ"
#include "chapters/appendix-references.typ"
