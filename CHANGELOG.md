# Changelog

All notable changes to this project will be documented in this file.
## [1.7.0] - 2026-07-18

### Added
- Replace selected topic with tabs
- Fix alignment and make tabs persistent
## [1.6.7] - 2026-06-15

### Build
- Bump ghcr.io/devcontainers/features/node
## [1.6.6] - 2026-06-15

### Fixed
- Formatting
- Ci release
## [1.6.5] - 2026-04-29

### Added
- Copy functionality for topic and message

### Fixed
- Formatting
## [1.6.4] - 2026-04-28

### Added
- Implement getTopicSwitchResetState function and update message selection logic

### Fixed
- Formatting
## [1.6.3] - 2026-04-01

### Added
- Keep at least one message per broker
- Handle large messages properly
- Add broker to title bar
- Make top bar responsive
- Handle blocking event by big message
- Implement reconnect delay in loop_forever and adjust incoming packet size constraints

### Fixed
- Formatting
- Update loop_forever to ignore payload size limit errors and continue processing
- Formatting
## [1.6.2] - 2026-03-31

### Added
- Implement downsampling for rate history and optimize rate tracking logic
- Enhance websocket message handling and optimize rate history processing
- Enhance publish message component and improve rate history display
- Update README
- Close #33
- Set dynamic page title for mqtt-inspector
## [1.6.1] - 2026-03-31

### Added
- Enhance MQTT connection handling and improve websocket message efficiency
- Implement topic selector component and refactor topic handling in various components
- Formatting
## [1.6.0] - 2026-03-28

### Added
- Refactor broker connection handling to support TLS and credentials
- Add broker authentication support and UI
- Add retain flag to MQTT messages and update related components
- Implement broker authentication and retain message feature
- Cleanup
## [1.5.0] - 2026-03-28

### Added
- Add MQTT frame building and handling tests
- Enhance stress test with topic management and client roles
- Improve performance on many messages per topic
- Add total messages tracking and improve message handling across components
- Cleanup

### Feat
- Lazyload messages
- Remove dead code
- Enhance MQTT message handling and improve command/pipeline management

### Fix
- Batch messages to keep stability in frontend

### Fixed
- Failing test
## [1.4.2] - 2026-03-22

### Added
- Enhance WebSocket message handling with binary frame parsing and batching
- Randomize every message

### Fixed
- Locking when we hit max_byte_size
- Formatting
- Show the line since when the messages are still available in throughput
- History calculation
## [1.4.1] - 2026-03-21

### Added
- Add setup steps to system test
- Add rate history chart component for visualizing throughput and storage data

### Fixed
- Formatting
## [1.4.0] - 2026-03-21

### Feat
- Refactor WebSocket message handling and state management
- Add system tests. Try fix locks and memory issues. Improve UI

### Fixed
- Formatting
- Refactor string formatting for improved readability and consistency
- Formatting
## [1.3.2] - 2025-09-18

### Feat
- Add prev, next and first buttons for messages
- Lock compare image
## [1.3.1] - 2025-09-17

### Feat
- Improve layout
- Store selected broker and ui in url

### Fix
- Remove warnings about missing child elements
- Formatting
## [1.3.0] - 2025-09-16

### Feat
- Fix layout
- Implement comparing messages
- Style diffs green and red
- Update layout to be more responsive

### Fix
- Formatting
## [1.2.0] - 2025-09-15

### Feat
- Update dev-container
- Move publish to tab
- Update message selection

### Fix
- Editable text
- Update to artifact@v4
- Formatting
## [1.1.1] - 2024-09-22

### Added
- Update publish message component alignment
- Update component scrolling behavior

### Fix
- #24
- Formatting

### Fixed
- Stop publishing reconnects
- Update Monaco on selectedCommand changed
- #28
- Formatting
- Update message when changing topic
## [1.1.0] - 2024-04-10

### Fixed
- Update monaco.svelte Copyright
- Use proper workflows for pull requests
- Push master workflow
## [1.0.0] - 2024-04-04

### Added
- Initial commit
- Implement serving web page
- Add deployment
- Add tini as a cheat because I can't ctrl+c
- Implement ctrl+c handling
- Use only one port for GET and websocket
- Cleanup main.rs by moving stuff to server.rs
- Implement saving commands
- Implement pipelines
- Select brokers instantly incl. broadcast
- Add deployment instructions
- Implement broker saving
- Implement editing pipelines
- Add GitHub Logo
- Use delta_t for pipeline timing
- Implement delta_t for history in messages, too
- Sanitize payload before sending to backend
- Implement button to reconnect to websocket
- MInor cleanup 🧼
- Update tree text
- Add license notive to files
- Implement status message if MQTT Broker is connected.
- Replace client by MqttBroker struct
- Send brokerstatus to new peers
- Replace SocketAddr with String
- Cap number of messages per topic at 100
- Load up to 100 messages per topic from backend
- Use CodeSnippet for selected topic for copy button
- Implementing removing brokers
- Add trim for convenience when adding new brokers
- Save commands in directory instead of a single file
- Implement removing commands and pipelines
- Add clean all rows button to pipelines
- Add guard before clearing all pipeline rows
- Implement saveguards when overriding pipelines or commands
- Move dialogs into dialogs directory 🧼
- Fix potential block, rename mqtt::Map to mqtt::BrokerMap
- Move submodules into server module
- Move logic from server to websocket module
- Move logic from server to broker_peer_bridge
- Move logic from server to broker_peer_bridge
- Refactor to use borrowing where I can make it work
- Implement tests for config.rs
- Add test job for rust in CI
- Add some tests for most svelte function
- Add svelte testing to CI
- Reverse test and build CI order
- Add code quality checks to CI. Also cleanup CI
- Format frontend
- Format backend
- Add prettyPrint for json
- Add searchbar to topic_tree
- Add search to pipeline nextStepText
- Add vscode build tasks
- Use monaco editor for messages
- Add tagged deploy for releases

### Fix
- Add new messages to all brokers

### Fixed
- Handle big payload sizes
- Update CI pipeline
- Add linter exception for monaco

