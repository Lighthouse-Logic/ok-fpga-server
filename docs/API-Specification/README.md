# ok-fpga-server API Specification (Typst)

Developer-facing specification of the REST (`api/openapi.yaml`) and gRPC
(`proto/okfpga/v1/okfpga.proto`) interfaces.

- PDF: [ok-fpga-server-api-specification.pdf](ok-fpga-server-api-specification.pdf)
- Entry point: [main.typ](main.typ); helpers: [lib.typ](lib.typ); chapters in [chapters/](chapters/)

## Build

```bash
cd docs/API-Specification
typst compile main.typ ok-fpga-server-api-specification.pdf
```

Requires Typst ≥ 0.13 (tested with 0.15.1).

## Author rules

- Visible text never mentions ADRs, decision ids (D-nn), phase names or the
  roadmap. Put those in `// ref:` comments next to the text they justify.
- Every heading has a label (`<res-declare>`); section numbers are stable
  anchors that the OpenAPI / proto files will cite later. Append new
  sub-sections; do not renumber.
- Use MUST / SHOULD / MAY (RFC 2119), give a *Rationale* for non-obvious
  rules, and cite Opal Kelly with `#footnote[...]`.
- Update Appendix A (implementation status) whenever the server changes.
- Typst traps: a line starting with `/ ` is a term list; a `.` directly
  after `#link(...)[...]`, `#footnote[...]` or `#ok-api` is parsed as field
  access (write `\.`).
