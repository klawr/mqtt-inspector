# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/klawr/mqtt-inspector/compare/v0.1.0...v0.1.1) - 2026-06-12

### Added

- implement reconnect delay in loop_forever and adjust incoming packet size constraints
- handle blocking event by big message
- add broker to title bar
- handle large messages properly
- Keep at least one message per broker
- enhance websocket message handling and optimize rate history processing
- enhance MQTT connection handling and improve websocket message efficiency
- cleanup
- implement broker authentication and retain message feature
- add retain flag to MQTT messages and update related components
- add broker authentication support and UI
- Refactor broker connection handling to support TLS and credentials
- Cleanup
- Add total messages tracking and improve message handling across components
- Improve performance on many messages per topic
- add MQTT frame building and handling tests
- Enhance WebSocket message handling with binary frame parsing and batching
- add rate history chart component for visualizing throughput and storage data
- *(frontend)* Use monaco editor for messages
- Format backend
- Format frontend
- Implement tests for config.rs
- Refactor to use borrowing where I can make it work
- Move logic from server to broker_peer_bridge
- Move logic from server to broker_peer_bridge
- Move logic from server to websocket module
- Move submodules into server module
- Fix potential block, rename mqtt::Map to mqtt::BrokerMap
- Implement removing commands and pipelines
- Save commands in directory instead of a single file
- Implementing removing brokers
- Load up to 100 messages per topic from backend
- Replace SocketAddr with String
- Send brokerstatus to new peers
- Replace client by MqttBroker struct
- Implement status message if MQTT Broker is connected.
- Add license notive to files
- Implement broker saving
- Add deployment instructions
- Select brokers instantly incl. broadcast
- Implement pipelines
- Implement saving commands
- Cleanup main.rs by moving stuff to server.rs
- Use only one port for GET and websocket
- Implement ctrl+c handling
- Add deployment
- Implement serving web page
- Initial commit

### Fixed

- formatting
- Formatting
- update loop_forever to ignore payload size limit errors and continue processing
- Formatting
- Formatting
- Formatting
- Formatting
- Refactor string formatting for improved readability and consistency
- Formatting
- *(backend)* Stop publishing reconnects
- Handle big payload sizes

### Other

- add semantic versioning
- remove oversized incoming message test case
- Enhance MQTT message handling and improve command/pipeline management
- Remove dead code
- Batch messages to keep stability in frontend
- Lazyload messages
- Add system tests. Try fix locks and memory issues. Improve UI
- Refactor WebSocket message handling and state management
- Editable text
