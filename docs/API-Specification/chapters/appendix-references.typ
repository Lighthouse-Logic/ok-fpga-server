// chapters/appendix-references.typ — Appendix B: references.
// ref: ADR-034 §8–§9 (Opal Kelly sources); fp6-api-surface.md.
#import "../lib.typ": *

= References <appendix-references>

== Opal Kelly <ref-opalkelly>

+ Opal Kelly, _FrontPanel SDK documentation_.
  #link("https://docs.opalkelly.com/fpsdk/")
+ Opal Kelly, _FrontPanel HDL: Endpoints_ (WireIn, WireOut, TriggerIn,
  TriggerOut, PipeIn, PipeOut, block-throttled pipes, register bridge).
  #link("https://docs.opalkelly.com/fpsdk/frontpanel-hdl/")
+ Opal Kelly, _FrontPanel API reference_, class
  `OpalKelly::FPGADataPortClassic` (`UpdateWireOuts`, `GetWireOutValue`,
  `UpdateTriggerOuts`, `IsTriggered`, `GetTriggerOutVector`,
  `ActivateTriggerIn`).
  #link("https://library.opalkelly.com/library/FrontPanelAPI/classOpalKelly_1_1FPGADataPortClassic.html")
+ FrontPanel SDK 6.1.0, C header `okFrontPanel.h`: declarations of
  `okFPGADataPortClassic_GetWireOutValue`, `_UpdateWireOuts`,
  `_UpdateTriggerOuts`, `_IsTriggered` and `_GetTriggerOutVector`; the C++
  wrapper class `FPGADataPortClassic` in the same header.
+ Opal Kelly, _SYZYGY Standard and SmartVIO DNA Specification_.
  #link("https://syzygyfpga.io/")

== Standards <ref-standards>

+ RFC 2119, _Key words for use in RFCs to Indicate Requirement Levels_.
  #link("https://www.rfc-editor.org/rfc/rfc2119")
+ RFC 6762, _Multicast DNS (mDNS)_, and RFC 6763, _DNS-Based Service Discovery (DNS-SD)_.
  #link("https://www.rfc-editor.org/rfc/rfc6762"), #link("https://www.rfc-editor.org/rfc/rfc6763")
+ RFC 7807, _Problem Details for HTTP APIs_.
  #link("https://www.rfc-editor.org/rfc/rfc7807")
+ OASIS, _MQTT Version 5.0 Standard_.
  #link("https://docs.oasis-open.org/mqtt/mqtt/v5.0/mqtt-v5.0.html")
+ OpenAPI Specification 3.x. #link("https://spec.openapis.org/oas/latest.html")
+ Protocol Buffers and gRPC status codes.
  #link("https://grpc.io/docs/guides/status-codes/")
+ W3C/WHATWG, _Server-Sent Events_.
  #link("https://html.spec.whatwg.org/multipage/server-sent-events.html")

== Project files <ref-project>

+ `api/openapi.yaml` — REST interface definition.
+ `proto/okfpga/v1/okfpga.proto` — gRPC interface definition.
+ `config.toml` — server configuration.
