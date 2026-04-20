# Roadmap: Raw MQTT Ingest → LoRaWAN Integration → Bidirectional Device Communication

## Where You Are

One-way IoT pipeline: EMQX MQTT gateways receive raw binary payloads, CEL expressions transform them, ClickHouse stores the results. Gateways are generic MQTT connections with a type enum. End device definitions carry ordered lists of payload contracts with match expressions. No LoRaWAN-specific support. No downlink capability.

## Where You're Going

A LoRaWAN-native IoT platform that connects to network servers like The Things Network, ingests all device messages (not just uplinks) into a raw event store, transforms uplink payloads through a simplified single-contract pipeline, and sends downlink commands back to devices.

---

## Phase 1: Simplify the Existing Model

Strip out abstractions that don't serve the LoRaWAN direction. Gateways become LoRaWAN MQTT connections (no type enum). End device definitions get a single transform expression and schema (no match expressions, no contract list).

- [ ] #196 — Remove gateway type abstraction — drop `gateway_type` enum, `EmqxGatewayConfig`, and the `GatewayRunner` factory pattern. A gateway is a LoRaWAN network server MQTT connection with a broker URL and credentials
- [ ] #197 — Simplify end device definitions to single contract — replace the ordered `contracts: Vec<PayloadContract>` with a single `transform_expression` and `json_schema` on the definition itself. Remove match expressions
- [ ] #198 — Update gateway orchestrator for LoRaWAN MQTT — remove the pluggable runner abstraction, assume all gateways connect to a LoRaWAN network server MQTT broker

> **Why first:** The current abstractions (gateway types, multi-contract matching) add complexity without serving the LoRaWAN use case. Simplifying now means Phase 2 builds on a clean foundation instead of working around dead code.

---

## Phase 2: TTN MQTT Integration + Raw Event Storage

Connect to The Things Network's MQTT broker, subscribe to all device topics, and store every message in a new `end_device_events` ClickHouse table. Uplink messages additionally feed into the existing CEL → `processed_envelopes` pipeline.

### TTN MQTT Connection

- [ ] #199 — TTN MQTT topic parser — parse TTN's topic structure (`v3/{app_id}@{tenant_id}/devices/{device_id}/{message_type}`) and extract device identity + message type
- [ ] #200 — TTN gateway runner — connect to TTN MQTT broker using application API key, subscribe to `v3/+/devices/+/#` for all device messages. Design the LoRaWAN gateway trait so future providers (ChirpStack) can implement the same interface

### Raw Event Storage

- [ ] #201 — `end_device_events` ClickHouse table — new table with columns: `received_at DateTime`, `end_device_id String`, `message_type String`, `payload JSON`. Partitioned by month, ordered by `(end_device_id, received_at)`
- [ ] #202 — Event ingestion worker — consumes all LoRaWAN messages from NATS, writes to `end_device_events`. Every message type (join, up, down/ack, down/nack, down/sent, location, service/data) gets stored

### Uplink Pipeline Bridge

- [ ] #203 — TTN uplink to CEL pipeline — when message_type is `up`, extract the `frm_payload` (raw bytes) from the TTN JSON, wrap it as a `RawEnvelope`, and publish to the existing `raw_envelopes` NATS stream. The existing analytics worker handles CEL transformation → `processed_envelopes`

> **Why second:** This is the core integration — connecting to TTN and getting data flowing. The raw event table captures everything the network server tells us about each device, while the uplink bridge ensures the existing transformation pipeline keeps working.

---

## Phase 3: Downlink Infrastructure

Build the reverse path — platform to device. Research TTN's downlink mechanics first, then implement domain model, API, and delivery.

- [ ] #204 — Research TTN downlink API — investigate confirmed vs unconfirmed downlinks, FPort selection, Class A receive windows, priority levels, payload encoding, and the MQTT publish topic format (`v3/{app_id}@{tenant_id}/devices/{device_id}/down/push`)
- [ ] #205 — Downlink domain model — `DownlinkCommand` entity with lifecycle states, persistence in PostgreSQL. Contract shape informed by research
- [ ] #206 — Downlink gRPC API — submit commands, query status, view history
- [ ] #207 — Downlink delivery via MQTT — publish downlink commands to the TTN MQTT broker. Track acknowledgment via `down/ack` and `down/nack` events from the event stream

> **Why third:** Downlinks require the uplink path to be working (you need device identity, event tracking, and a proven MQTT connection). The research item is deliberately first in this phase — the contract shape and scheduling model depend on understanding TTN's constraints.

---

## Backlog

Items that are valuable but not blocking the LoRaWAN integration. Pick these up opportunistically.

- [ ] #65 — Consolidate service initialization into `init_process` crate
- [ ] #48 — CDC high availability
- [ ] #82 — Restrict wildcard CORS to localhost origins
- [ ] #159 — Expose Ponix as an MCP server
- [ ] #134 — MCP client in agent runtime
- [ ] #135 — Workspace MCP registry
- [ ] #136 — MCP server management API

---

## Open Questions

- **Downlink contract shape** — needs research into TTN's downlink API before committing. How do we define the encoding from structured JSON to raw bytes? Is it the inverse of the uplink transform expression, or a separate definition?
- **LoRaWAN class scheduling** — Class A devices can only receive downlinks in a narrow window after an uplink. Does the platform need to queue and time downlinks, or does TTN handle that?
- **TTN application registration** — creating applications and registering devices in TTN is explicitly out of scope. An external process handles this. But how do we map TTN app IDs to ponix organizations/gateways?
- **Multiple TTN applications per gateway** — is a gateway 1:1 with a TTN application, or can one gateway aggregate multiple applications?
- **Provider abstraction depth** — how much should the LoRaWAN gateway trait abstract? TTN and ChirpStack have different MQTT topic structures, auth models, and downlink APIs. Design for the common denominator or allow provider-specific extensions?
