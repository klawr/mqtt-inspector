/*
 * Copyright (c) 2024-2026 Kai Lawrence
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

use super::config;
use super::jsonrpc;
use super::mqtt;
use super::websocket;

use rumqttc::v5::mqttbytes::{self, v5::Packet};
use rumqttc::v5::{ConnectionError, Event, StateError};
use std::collections::HashMap;

const RECONNECT_BACKOFF_MS: u64 = 1000;
const DISCONNECT_NOTIFY_GRACE_MS: u64 = 1500;

/// Constant-time string comparison to prevent timing attacks.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn broker_exists(mqtt_map: &mqtt::BrokerMap, hostname: &str) -> bool {
    mqtt_map.lock().unwrap().contains_key(hostname)
}

fn truncate_payload(payload: bytes::Bytes, max_len: usize) -> (bytes::Bytes, usize) {
    let original_len = payload.len();
    if original_len <= max_len {
        return (payload, original_len);
    }
    (
        bytes::Bytes::copy_from_slice(&payload[..max_len]),
        original_len,
    )
}

/// Evict oldest messages while preserving each topic's newest message.
///
/// This may stop before reaching `max_broker_bytes` if every topic has only
/// one message left, because those newest-per-topic messages are protected.
fn evict_while_preserving_topic_latest(
    broker: &mut mqtt::MqttBroker,
) -> Vec<(String, usize, usize)> {
    let mut eviction_counts: HashMap<String, usize> = HashMap::new();

    while broker.total_bytes > mqtt::max_broker_bytes() {
        let queue_len = broker.eviction_order.len();
        if queue_len == 0 {
            break;
        }

        let mut evicted_one = false;

        for _ in 0..queue_len {
            let Some((topic_key, payload_len)) = broker.eviction_order.pop_front() else {
                break;
            };

            let topic_len = broker
                .topics
                .get(&topic_key)
                .map(std::collections::VecDeque::len);
            match topic_len {
                Some(len) if len > 1 => {
                    if let Some(topic_vec) = broker.topics.get_mut(&topic_key) {
                        topic_vec.pop_front();
                        broker.total_bytes = broker.total_bytes.saturating_sub(payload_len);
                        broker.total_messages = broker.total_messages.saturating_sub(1);
                        *eviction_counts.entry(topic_key.clone()).or_insert(0) += 1;
                        if topic_vec.is_empty() {
                            broker.topics.remove(&topic_key);
                        }
                        evicted_one = true;
                        break;
                    }
                }
                Some(_) => {
                    // Keep newest/only message for this topic.
                    broker.eviction_order.push_back((topic_key, payload_len));
                }
                None => {
                    // Stale eviction entry: topic was already removed.
                }
            }
        }

        if !evicted_one {
            break;
        }
    }

    eviction_counts
        .into_iter()
        .map(|(topic_key, count)| {
            let new_count = broker.topics.get(&topic_key).map(|v| v.len()).unwrap_or(0);
            (topic_key, count, new_count)
        })
        .collect()
}

/// Why a connection loop returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopExit {
    /// Broker removed, DISCONNECT received, or (fallback mode) the connection dropped.
    Ended,
    /// The broker rejected MQTT 5 before ever accepting a connection.
    ProtocolRejected,
}

/// How the connection loop deals with protocol negotiation in auto mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopMode {
    /// A fixed protocol; never exit for protocol reasons.
    Fixed,
    /// Auto mode on MQTT 5: exit with `ProtocolRejected` if the broker keeps
    /// refusing the MQTT 5 handshake.
    TryV5,
    /// Auto mode after falling back to 3.1.1: once connected, exit when the
    /// connection drops, so the next attempt tries MQTT 5 again. This undoes a
    /// fallback that was caused by a transient failure rather than an old broker.
    FallbackV311,
}

/// Consecutive handshake rejections before auto mode falls back to 3.1.1.
/// More than one, so a broker restarting mid-handshake doesn't trigger it.
const PROTOCOL_REJECTIONS_BEFORE_FALLBACK: u32 = 2;

/// A connection event, independent of the MQTT version.
enum ConnEvent {
    Publish {
        topic: String,
        payload: bytes::Bytes,
        retain: bool,
        properties: Option<std::sync::Arc<mqtt::MessageProperties>>,
    },
    ConnAck(String),
    Disconnect(String),
    /// PingResp, SubAck, outgoing packets, ...
    Other,
    OversizedPayload,
    StateError(String),
    /// `protocol_rejection`: the failure looks like the broker refusing the
    /// MQTT version (connection dropped or unparseable CONNACK during handshake).
    Error {
        message: String,
        protocol_rejection: bool,
    },
}

fn is_protocol_rejection_io(err: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    // 3.1.1-only brokers either close the socket on a v5 CONNECT (mosquitto
    // < 1.6: "connection closed by peer") or answer with a 3.1.1 CONNACK that
    // the v5 parser rejects as invalid data.
    matches!(
        err.kind(),
        ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::UnexpectedEof
            | ErrorKind::InvalidData
    )
}

fn v5_event(n: Result<Event, ConnectionError>) -> ConnEvent {
    match n {
        Ok(Event::Incoming(Packet::Publish(p))) => ConnEvent::Publish {
            // rumqttc already resolved topic aliases into `p.topic`.
            topic: String::from_utf8_lossy(&p.topic).into_owned(),
            payload: p.payload,
            retain: p.retain,
            properties: mqtt::MessageProperties::from_publish(p.properties),
        },
        Ok(Event::Incoming(Packet::ConnAck(a))) => ConnEvent::ConnAck(format!("{:?}", a.code)),
        Ok(Event::Incoming(Packet::Disconnect(d))) => ConnEvent::Disconnect(format!(
            "{:?} {:?}",
            d.reason_code,
            d.properties.and_then(|p| p.reason_string)
        )),
        Ok(_) => ConnEvent::Other,
        Err(ConnectionError::MqttState(StateError::Deserialization(
            mqttbytes::Error::PayloadSizeLimitExceeded { .. },
        ))) => ConnEvent::OversizedPayload,
        Err(ConnectionError::MqttState(err)) => ConnEvent::StateError(err.to_string()),
        Err(err) => {
            let protocol_rejection = match &err {
                ConnectionError::Io(io) => is_protocol_rejection_io(io),
                ConnectionError::ConnectionRefused(code) => {
                    *code == mqttbytes::v5::ConnectReturnCode::UnsupportedProtocolVersion
                }
                _ => false,
            };
            ConnEvent::Error {
                message: err.to_string(),
                protocol_rejection,
            }
        }
    }
}

fn v311_event(n: Result<rumqttc::Event, rumqttc::ConnectionError>) -> ConnEvent {
    use rumqttc::{ConnectionError, Event, Packet, StateError};
    match n {
        Ok(Event::Incoming(Packet::Publish(p))) => ConnEvent::Publish {
            topic: p.topic,
            payload: p.payload,
            retain: p.retain,
            properties: None,
        },
        Ok(Event::Incoming(Packet::ConnAck(a))) => ConnEvent::ConnAck(format!("{:?}", a.code)),
        Ok(Event::Incoming(Packet::Disconnect)) => ConnEvent::Disconnect(String::new()),
        Ok(_) => ConnEvent::Other,
        Err(ConnectionError::MqttState(StateError::Deserialization(
            rumqttc::mqttbytes::Error::PayloadSizeLimitExceeded(_),
        ))) => ConnEvent::OversizedPayload,
        Err(ConnectionError::MqttState(err)) => ConnEvent::StateError(err.to_string()),
        Err(err) => ConnEvent::Error {
            message: err.to_string(),
            // Every MQTT 5 broker also accepts 3.1.1, so this is never the cause.
            protocol_rejection: false,
        },
    }
}

/// State of one connection loop, shared by both MQTT versions.
struct ConnectionLoop<'a> {
    hostname: String,
    protocol: config::MqttProtocol,
    mode: LoopMode,
    peer_map: &'a websocket::PeerMap,
    mqtt_map: &'a mqtt::BrokerMap,
    notification_buf: &'a websocket::NotificationBuf,
    disconnect_candidate_since: Option<std::time::Instant>,
    disconnect_notified: bool,
    connected_once: bool,
    consecutive_rejections: u32,
}

impl ConnectionLoop<'_> {
    /// Handles one event; `Some` stops the loop.
    fn handle(&mut self, event: ConnEvent) -> Option<LoopExit> {
        if !broker_exists(self.mqtt_map, &self.hostname) {
            println!(
                "Broker {} no longer exists in map. Stopping connection loop.",
                self.hostname
            );
            return Some(LoopExit::Ended);
        }

        match event {
            ConnEvent::Publish {
                topic,
                payload,
                retain,
                properties,
            } => {
                self.disconnect_candidate_since = None;
                return self.handle_publish(topic, payload, retain, properties);
            }
            ConnEvent::ConnAck(code) => {
                self.disconnect_candidate_since = None;
                self.disconnect_notified = false;
                self.connected_once = true;
                self.consecutive_rejections = 0;
                let client = {
                    let mut mqtt_lock = self.mqtt_map.lock().unwrap();
                    if let Some(broker) = mqtt_lock.get_mut(&self.hostname) {
                        broker.connected = true;
                        broker.protocol = Some(self.protocol);
                        Some(broker.client.clone())
                    } else {
                        None
                    }
                };
                if let Some(mut client) = client {
                    if let Err(err) = client.subscribe_all() {
                        println!(
                            "Failed to re-subscribe after ConnAck for {}: {err}",
                            self.hostname
                        );
                    }
                }
                websocket::send_broker_status_to_peers(
                    self.peer_map,
                    &self.hostname,
                    true,
                    Some(self.protocol),
                );
                println!(
                    "Connection event: {code} for {:?} (MQTT {})",
                    self.hostname,
                    self.protocol.label()
                );
            }
            ConnEvent::Disconnect(reason) => {
                println!("Disconnect event for {:?}: {reason}", self.hostname);
                return Some(LoopExit::Ended);
            }
            ConnEvent::Other | ConnEvent::OversizedPayload => {}
            ConnEvent::StateError(err) => {
                println!(
                    "MqttState error for {:?}: {err}. Will retry.",
                    self.hostname
                );
                return self.handle_connection_error();
            }
            ConnEvent::Error {
                message,
                protocol_rejection,
            } => {
                println!(
                    "Connection error for {:?}: {message}. Will retry.",
                    self.hostname
                );
                if self.mode == LoopMode::TryV5 && !self.connected_once {
                    if protocol_rejection {
                        self.consecutive_rejections += 1;
                        if self.consecutive_rejections >= PROTOCOL_REJECTIONS_BEFORE_FALLBACK {
                            return Some(LoopExit::ProtocolRejected);
                        }
                    } else {
                        self.consecutive_rejections = 0;
                    }
                }
                return self.handle_connection_error();
            }
        }
        None
    }

    /// Marks the broker disconnected, notifies peers after a grace period and
    /// backs off before the client reconnects.
    fn handle_connection_error(&mut self) -> Option<LoopExit> {
        {
            let mut mqtt_lock = self.mqtt_map.lock().unwrap();
            if let Some(broker) = mqtt_lock.get_mut(&self.hostname) {
                broker.connected = false;
            }
        }
        if self.mode == LoopMode::FallbackV311 && self.connected_once {
            return Some(LoopExit::Ended);
        }
        let now = std::time::Instant::now();
        let candidate_since = self.disconnect_candidate_since.get_or_insert(now);
        if !self.disconnect_notified
            && now.duration_since(*candidate_since).as_millis()
                >= DISCONNECT_NOTIFY_GRACE_MS as u128
        {
            websocket::send_broker_status_to_peers(self.peer_map, &self.hostname, false, None);
            self.disconnect_notified = true;
        }
        std::thread::sleep(std::time::Duration::from_millis(RECONNECT_BACKOFF_MS));
        None
    }

    fn handle_publish(
        &mut self,
        topic: String,
        payload: bytes::Bytes,
        retain: bool,
        properties: Option<std::sync::Arc<mqtt::MessageProperties>>,
    ) -> Option<LoopExit> {
        let (payload, original_payload_len) = truncate_payload(payload, mqtt::max_message_size());
        let timestamp = chrono::Utc::now().to_rfc3339();
        let (total_bytes, new_sample, topic_message_count, evictions, _rate_history_len) = {
            let mut mqtt_lock = self.mqtt_map.lock().unwrap();
            let broker = match mqtt_lock.get_mut(&self.hostname) {
                Some(b) => b,
                None => {
                    println!("Broker {} not found in map. Exiting loop.", self.hostname);
                    return Some(LoopExit::Ended);
                }
            };
            broker.connected = true;
            let msg_bytes = payload.len();
            let new_msg = mqtt::MqttMessage {
                timestamp: timestamp.clone(),
                payload: payload.clone(),
                original_payload_size: original_payload_len,
                retain,
                properties: properties.clone(),
            };
            let topic_name = topic.clone();
            if let Some(topic_vec) = broker.topics.get_mut(&topic_name) {
                topic_vec.push_back(new_msg);
            } else {
                let mut vd = std::collections::VecDeque::new();
                vd.push_back(new_msg);
                broker.topics.insert(topic_name.clone(), vd);
            }
            broker.total_bytes += msg_bytes;
            broker.total_messages += 1;
            broker.eviction_order.push_back((topic_name, msg_bytes));

            let evictions = evict_while_preserving_topic_latest(broker);

            let topic_message_count = broker.topics.get(&topic).map(|v| v.len()).unwrap_or(0);

            // Rate history sampling: accumulate bytes and record every 10s
            broker.rate_bytes_accumulator += msg_bytes;
            let now_ms = chrono::Utc::now().timestamp_millis();
            let elapsed_ms = now_ms - broker.rate_last_sample_ms;
            let new_sample = if elapsed_ms >= 10_000 {
                let elapsed_secs = elapsed_ms as f64 / 1000.0;
                let bytes_per_second = broker.rate_bytes_accumulator as f64 / elapsed_secs;
                let sample = mqtt::RateHistoryEntry {
                    timestamp: now_ms,
                    bytes_per_second,
                    total_bytes: broker.total_bytes,
                };
                broker.rate_history.push(sample.clone());
                broker.rate_bytes_accumulator = 0;
                broker.rate_last_sample_ms = now_ms;
                // Prune entries older than 7 days
                let cutoff = now_ms - 7 * 24 * 60 * 60 * 1000;
                broker.rate_history.retain(|e| e.timestamp >= cutoff);
                Some(sample)
            } else {
                None
            };

            let rate_history_len = broker.rate_history.len();

            (
                broker.total_bytes,
                new_sample,
                topic_message_count,
                evictions,
                rate_history_len,
            )
        }; // mqtt_lock dropped here
        if let Some(ref sample) = new_sample {
            websocket::send_rate_sample_to_peers(self.peer_map, &self.hostname, sample);
        }
        // Buffer eviction notifications (will be flushed in batch)
        if !evictions.is_empty() {
            websocket::buffer_evictions(self.notification_buf, &self.hostname, &evictions);
        }
        // Buffer lightweight meta (will be flushed in batch)
        websocket::buffer_message_meta(
            self.notification_buf,
            websocket::PendingMeta {
                source: self.hostname.clone(),
                topic: topic.clone(),
                timestamp: timestamp.clone(),
                payload_size: original_payload_len,
                total_bytes,
                topic_message_count,
                retain,
            },
        );
        // Send full payload ONLY to peers watching this topic
        let message = websocket::SubscribedPeerMessage {
            source: &self.hostname,
            topic: &topic,
            payload: &payload,
            original_payload_size: original_payload_len,
            total_bytes: Some(total_bytes),
            timestamp: &timestamp,
            retain,
            properties: properties.as_deref(),
        };
        websocket::send_message_to_subscribed_peers(self.peer_map, &message);
        None
    }

    /// Marks the broker disconnected once the loop has ended.
    fn finish(&self) {
        {
            let mut mqtt_lock = self.mqtt_map.lock().unwrap();
            if let Some(broker) = mqtt_lock.get_mut(&self.hostname) {
                broker.connected = false;
            }
        }
        if !self.disconnect_notified {
            websocket::send_broker_status_to_peers(self.peer_map, &self.hostname, false, None);
        }
    }
}

fn loop_forever(
    connection: mqtt::MqttConnection,
    mode: LoopMode,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    notification_buf: &websocket::NotificationBuf,
) -> LoopExit {
    let ((ip, port), protocol) = match &connection {
        mqtt::MqttConnection::V5(c) => (
            c.eventloop.options.broker_address(),
            config::MqttProtocol::V5,
        ),
        mqtt::MqttConnection::V311(c) => (
            c.eventloop.mqtt_options.broker_address(),
            config::MqttProtocol::V311,
        ),
    };
    let mut state = ConnectionLoop {
        hostname: format!("{ip}:{port}"),
        protocol,
        mode,
        peer_map,
        mqtt_map,
        notification_buf,
        disconnect_candidate_since: None,
        disconnect_notified: false,
        connected_once: false,
        consecutive_rejections: 0,
    };

    let exit = match connection {
        mqtt::MqttConnection::V5(mut c) => c.iter().find_map(|n| state.handle(v5_event(n))),
        mqtt::MqttConnection::V311(mut c) => c.iter().find_map(|n| state.handle(v311_event(n))),
    }
    // The iterator only ends when the client was dropped/disconnected.
    .unwrap_or(LoopExit::Ended);

    println!(
        "Connection loop for {:?} ended ({exit:?}). Marking broker as disconnected.",
        state.hostname
    );
    state.finish();
    exit
}

pub fn connect_to_known_brokers(
    broker_path: &str,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    notification_buf: &websocket::NotificationBuf,
) {
    let config_dir = std::path::Path::new(broker_path)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    config::get_known_brokers(broker_path)
        .iter()
        .for_each(|broker_config| {
            let resolved = broker_config.with_resolved_cert_paths(config_dir);
            connect_to_broker(&resolved, peer_map, mqtt_map, notification_buf);
        });
}

/// Processes one JSON-RPC request from a connected peer.
///
/// `access` is the access of the connection the request arrived on. It is
/// passed in rather than looked up in `peer_map`, because a peer can be
/// removed from the map (e.g. evicted as a slow consumer) while its socket is
/// still delivering requests; a lookup would then have no access to enforce.
pub fn deserialize_json_rpc_and_process(
    json_rpc: &str,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    config_path: &str,
    addr: Option<std::net::SocketAddr>,
    notification_buf: &websocket::NotificationBuf,
    access: &websocket::PeerAccess,
) {
    let message = match jsonrpc::deserialize_json_rpc(json_rpc) {
        Ok(msg) => msg,
        Err(err) => {
            println!("Error deserializing JSON-RPC: {err:?}");
            return;
        }
    };
    println!(
        "Got method \"{}\" with params {}",
        message.method, message.params
    );
    match message.method {
        // Everything that changes the shared configuration goes through
        // `process_config_change`; this arm is the single admin gate for all of it.
        "connect" | "remove" | "save_command" | "remove_command" | "save_pipeline"
        | "remove_pipeline" => {
            if !access.can_edit() {
                println!(
                    "Denied \"{}\" for {}: admin role required",
                    message.method,
                    access
                        .identity()
                        .map(|i| i.display_name())
                        .unwrap_or_default()
                );
                return;
            }
            process_config_change(
                message,
                peer_map,
                mqtt_map,
                config_path,
                addr,
                notification_buf,
                access,
            );
        }
        "publish" => {
            let host = match message.params.get("host").and_then(|v| v.as_str()) {
                Some(h) => h.trim_matches('"').to_string(),
                None => {
                    println!("Missing or invalid 'host' param for publish");
                    return;
                }
            };
            // Check authentication before allowing publish
            if let Some(peer_addr) = addr {
                let peers = peer_map.lock().unwrap();
                if let Some(peer) = peers.get(&peer_addr) {
                    if !peer.authenticated_brokers.contains(&host) {
                        println!(
                            "Peer {peer_addr} not authenticated for broker {host}, publish denied"
                        );
                        return;
                    }
                } else {
                    return;
                }
            } else {
                return;
            }
            let topic = match message.params.get("topic").and_then(|v| v.as_str()) {
                Some(t) => t,
                None => {
                    println!("Missing or invalid 'topic' param for publish");
                    return;
                }
            };
            let payload = match message.params.get("payload").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => {
                    println!("Missing or invalid 'payload' param for publish");
                    return;
                }
            };
            let retain = message
                .params
                .get("retain")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // Optional MQTT 5 user properties: [[key, value], ...].
            let user_properties = message
                .params
                .get("user_properties")
                .and_then(|v| v.as_array())
                .map(|pairs| {
                    pairs
                        .iter()
                        .filter_map(|pair| match pair.as_array()?.as_slice() {
                            [key, value] => {
                                Some((key.as_str()?.to_string(), value.as_str()?.to_string()))
                            }
                            _ => None,
                        })
                        .filter(|(key, _)| !key.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            mqtt::publish_message(&host, topic, payload, retain, user_properties, mqtt_map);
        }
        "subscribe_topic" => {
            if let Some(peer_addr) = addr {
                if let (Some(broker), Some(topic)) = (
                    message.params["broker"].as_str(),
                    message.params["topic"].as_str(),
                ) {
                    let since_timestamp = message.params["since_timestamp"].as_str();
                    websocket::handle_subscribe_topic(
                        peer_map,
                        mqtt_map,
                        peer_addr,
                        broker,
                        topic,
                        since_timestamp,
                    );
                }
            }
        }
        "unsubscribe_topic" => {
            if let Some(peer_addr) = addr {
                if let (Some(broker), Some(topic)) = (
                    message.params["broker"].as_str(),
                    message.params["topic"].as_str(),
                ) {
                    websocket::handle_unsubscribe_topic(peer_map, peer_addr, broker, topic);
                }
            }
        }
        "authenticate_broker" => {
            let hostname = match message.params.get("hostname").and_then(|v| v.as_str()) {
                Some(h) => h.trim_matches('"').to_string(),
                None => {
                    println!("Missing 'hostname' param for authenticate_broker");
                    return;
                }
            };
            if access.is_claims() {
                println!("Ignoring authenticate_broker in claims mode; access comes from roles");
                return;
            }
            let supplied_password = message
                .params
                .get("password")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Look up the broker's configured password
            let brokers_path = std::format!("{config_path}/brokers.json");
            let broker_configs = config::get_known_brokers(&brokers_path);
            let broker_cfg = broker_configs.iter().find(|b| b.key() == hostname);

            let success = match broker_cfg {
                Some(cfg) => match cfg.ui_password() {
                    Some(p) => constant_time_eq(supplied_password, p),
                    None => true, // No password required
                },
                None => false, // Broker not found in config
            };

            if success {
                if let Some(peer_addr) = addr {
                    {
                        let mut peers = peer_map.lock().unwrap();
                        if let Some(peer) = peers.get_mut(&peer_addr) {
                            peer.authenticated_brokers.insert(hostname.clone());
                        }
                    }
                    // Send topic summaries for this broker now that the peer is authenticated
                    websocket::send_broker_topic_summaries(
                        peer_map, mqtt_map, peer_addr, &hostname,
                    );
                }
            }

            // Send result back to the requesting peer
            if let Some(peer_addr) = addr {
                let result = jsonrpc::JsonRpcNotification {
                    jsonrpc: "2.0",
                    method: "broker_auth_result",
                    params: serde_json::json!({
                        "broker": hostname,
                        "success": success,
                    }),
                };
                if let Ok(serialized) = serde_json::to_string(&result) {
                    websocket::send_to_specific_peer(peer_map, peer_addr, &serialized);
                }
            }
        }
        _ => {}
    }
}

/// Handles the methods that change the shared configuration (brokers, commands,
/// pipelines). Only called after the admin check in `deserialize_json_rpc_and_process`.
fn process_config_change(
    message: jsonrpc::JsonRpcNotification<'_>,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    config_path: &str,
    addr: Option<std::net::SocketAddr>,
    notification_buf: &websocket::NotificationBuf,
    access: &websocket::PeerAccess,
) {
    match message.method {
        "connect" => {
            let hostname = match message.params.get("hostname").and_then(|v| v.as_str()) {
                Some(h) => h.trim_matches('"').to_string(),
                None => {
                    println!("Missing or invalid 'hostname' param for connect");
                    return;
                }
            };
            let use_tls = message
                .params
                .get("use_tls")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let username = message
                .params
                .get("username")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let optional_string = |key: &str| {
                message
                    .params
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            };
            let password = message
                .params
                .get("password")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let broker_config = config::BrokerConfig {
                host: hostname,
                use_tls,
                username,
                password,
                ca_cert: optional_string("ca_cert"),
                client_cert: optional_string("client_cert"),
                client_key: optional_string("client_key"),
                client_key_password: message
                    .params
                    .get("client_key_password")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                access_password: message
                    .params
                    .get("access_password")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                allowed_roles: message
                    .params
                    .get("allowed_roles")
                    .and_then(|v| v.as_array())
                    .map(|roles| {
                        roles
                            .iter()
                            .filter_map(|r| r.as_str())
                            .map(|r| r.trim().to_string())
                            .filter(|r| !r.is_empty())
                            .collect()
                    })
                    .unwrap_or_default(),
                protocol: optional_string("protocol").and_then(|p| config::MqttProtocol::parse(&p)),
            };
            // Reject before persisting, so a typo never ends up in brokers.json.
            if let Err(err) = broker_config.address() {
                println!("Ignoring connect request: {err}");
                return;
            }
            let has_password = broker_config.ui_password().is_some();
            // Persist the paths as entered (relative to the config dir), but
            // connect using the resolved ones.
            let resolved_config =
                broker_config.with_resolved_cert_paths(std::path::Path::new(config_path));
            connect_to_broker(&resolved_config, peer_map, mqtt_map, notification_buf);
            let broker_path = std::format!("{}/brokers.json", config_path);
            config::add_to_brokers(&broker_path, &broker_config);
            let broker_key = broker_config.key().to_string();
            // Grant access to every peer that may use the new broker (open
            // brokers in password mode, matching roles in claims mode) before
            // broadcasting, so the list each peer gets is already filtered.
            websocket::auto_authenticate_all_peers_for_broker(
                peer_map,
                &broker_key,
                has_password,
                &broker_config.allowed_roles,
            );
            websocket::broadcast_brokers(peer_map, mqtt_map);
            // Password mode: the peer that added a password-protected broker
            // knows the password, so authenticate it right away.
            if has_password && !access.is_claims() {
                if let Some(peer_addr) = addr {
                    let mut peers = peer_map.lock().unwrap();
                    if let Some(peer) = peers.get_mut(&peer_addr) {
                        peer.authenticated_brokers.insert(broker_key.clone());
                    }
                    drop(peers);
                    // Notify the frontend so it marks the broker as authenticated
                    let result = jsonrpc::JsonRpcNotification {
                        jsonrpc: "2.0",
                        method: "broker_auth_result",
                        params: serde_json::json!({ "broker": broker_key, "success": true }),
                    };
                    if let Ok(serialized) = serde_json::to_string(&result) {
                        websocket::send_to_specific_peer(peer_map, peer_addr, &serialized);
                    }
                }
            }
        }
        "remove" => {
            let hostname = match message.params.get("hostname").and_then(|v| v.as_str()) {
                Some(h) => h.trim_matches('"').to_string(),
                None => {
                    println!("Missing or invalid 'hostname' param for remove");
                    return;
                }
            };
            remove_broker(&hostname, peer_map, mqtt_map);
            let broker_path = std::format!("{}/brokers.json", config_path);
            config::remove_from_brokers(&broker_path, &hostname);
            websocket::broadcast_brokers(peer_map, mqtt_map)
        }
        "save_command" => {
            let command_path: String = std::format!("{config_path}/commands");
            config::add_to_commands(&command_path, message.params);
            websocket::broadcast_commands(peer_map, config_path);
        }
        "remove_command" => {
            let command_path: String = std::format!("{config_path}/commands");
            config::remove_from_commands(&command_path, message.params);
            websocket::broadcast_commands(peer_map, config_path);
        }
        "save_pipeline" => {
            let pipelines_path = std::format!("{config_path}/pipelines");
            config::add_to_pipelines(&pipelines_path, message.params);
            websocket::broadcast_pipelines(peer_map, config_path);
        }
        "remove_pipeline" => {
            let pipelines_path = std::format!("{config_path}/pipelines");
            config::remove_from_pipelines(&pipelines_path, message.params);
            websocket::broadcast_pipelines(peer_map, config_path);
        }
        other => println!("Unhandled config method \"{other}\""),
    }
}

fn connect_to_broker(
    broker_config: &config::BrokerConfig,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    notification_buf: &websocket::NotificationBuf,
) {
    let config_clone = broker_config.clone();
    let mqtt_map_clone = mqtt_map.clone();
    let peer_map_clone = peer_map.clone();
    let buf_clone = notification_buf.clone();

    std::thread::spawn(move || {
        connect_to_mqtt_client_and_loop_forever(
            &config_clone,
            &mqtt_map_clone,
            &peer_map_clone,
            &buf_clone,
        );
    });
}

fn connect_to_mqtt_client_and_loop_forever(
    broker_config: &config::BrokerConfig,
    mqtt_map: &mqtt::BrokerMap,
    peer_map: &websocket::PeerMap,
    notification_buf: &websocket::NotificationBuf,
) {
    let mqtt_host = broker_config.key();
    let mut mqtt_lock = mqtt_map.lock().unwrap();

    if mqtt_lock.contains_key(mqtt_host) {
        println!("MQTT-Client for {mqtt_host} already exists.");
    } else {
        println!("MQTT-Client for {mqtt_host} does not exist. Creating new client.");
        // Unset protocol = auto: start with MQTT 5, fall back to 3.1.1 if rejected.
        let auto = broker_config.protocol.is_none();
        let mut protocol = broker_config.protocol.unwrap_or(config::MqttProtocol::V5);
        let (client, mut connection) = match mqtt::connect_to_mqtt_host(broker_config, protocol) {
            Ok(created) => created,
            Err(err) => {
                println!("Not connecting to {mqtt_host}: {err}");
                return;
            }
        };
        let requires_auth = broker_config.ui_password().is_some();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let broker = mqtt::MqttBroker {
            client,
            broker: mqtt_host.to_string(),
            connected: false,
            topics: HashMap::new(),
            total_bytes: 0,
            total_messages: 0,
            eviction_order: std::collections::VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: now_ms,
            requires_auth,
            allowed_roles: broker_config.allowed_roles.clone(),
            protocol: None,
        };

        mqtt_lock.insert(mqtt_host.to_string(), broker);
        drop(mqtt_lock);

        loop {
            let mode = match (auto, protocol) {
                (false, _) => LoopMode::Fixed,
                (true, config::MqttProtocol::V5) => LoopMode::TryV5,
                (true, config::MqttProtocol::V311) => LoopMode::FallbackV311,
            };
            let exit = loop_forever(connection, mode, peer_map, mqtt_map, notification_buf);

            if !broker_exists(mqtt_map, mqtt_host) {
                println!("Broker {mqtt_host} was removed. Stopping reconnect loop.");
                break;
            }

            match exit {
                LoopExit::ProtocolRejected => {
                    // The loop already backed off after the rejection.
                    println!("{mqtt_host} rejected MQTT 5. Falling back to MQTT 3.1.1.");
                    protocol = config::MqttProtocol::V311;
                }
                LoopExit::Ended => {
                    if auto {
                        protocol = config::MqttProtocol::V5;
                    }
                    println!(
                        "Connection loop ended for {mqtt_host}. Recreating MQTT client and reconnecting."
                    );
                    std::thread::sleep(std::time::Duration::from_millis(RECONNECT_BACKOFF_MS));
                }
            }

            // The address was valid for the first client, so this can't fail.
            let Ok((new_client, new_connection)) =
                mqtt::connect_to_mqtt_host(broker_config, protocol)
            else {
                break;
            };
            {
                let mut mqtt_lock = mqtt_map.lock().unwrap();
                let Some(broker) = mqtt_lock.get_mut(mqtt_host) else {
                    break;
                };
                broker.client = new_client;
                broker.connected = false;
            }
            connection = new_connection;
        }
    }
}

fn remove_broker(mqtt_host: &str, peer_map: &websocket::PeerMap, mqtt_map: &mqtt::BrokerMap) {
    let mut mqtt_lock = mqtt_map.lock().unwrap();

    if let Some(broker) = mqtt_lock.get(mqtt_host) {
        println!("Removing MQTT-Client for {mqtt_host}");

        if let Err(err) = broker.client.clone().disconnect() {
            println!("Error disconnecting MQTT client: {err:?}");
        }

        mqtt_lock.remove(mqtt_host);
        drop(mqtt_lock);
        peer_map
            .lock()
            .unwrap()
            .iter_mut()
            .for_each(|(_addr, peer)| {
                if peer.selected_broker.as_deref() == Some(mqtt_host) {
                    peer.selected_broker = None;
                    peer.subscribed_topics.clear();
                }
                peer.authenticated_brokers.remove(mqtt_host);

                let message = jsonrpc::JsonRpcNotification {
                    jsonrpc: "2.0",
                    method: "broker_removal",
                    params: serde_json::json!(mqtt_host),
                };

                if let Ok(serialized) = serde_json::to_string(&message) {
                    match peer
                        .tx
                        .try_send(warp::filters::ws::Message::text(serialized))
                    {
                        Ok(_) => {}
                        Err(err) if err.is_disconnected() => {
                            println!("Error sending message: {err:?}")
                        }
                        Err(_) => {}
                    }
                } else {
                    println!("Failed to serialize brokers.");
                }
            });
    } else {
        println!("No MQTT-Client for {mqtt_host} found.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_channel::mpsc::channel;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Mutex;
    use websocket::PEER_CHANNEL_CAPACITY;

    fn make_peer_map() -> websocket::PeerMap {
        websocket::PeerMap::new(Mutex::new(HashMap::new()))
    }

    fn make_mqtt_map() -> mqtt::BrokerMap {
        mqtt::BrokerMap::new(Mutex::new(HashMap::new()))
    }

    fn make_notification_buf() -> websocket::NotificationBuf {
        websocket::NotificationBuf::new(Mutex::new(websocket::NotificationBuffer::default()))
    }

    // --- MQTT version fallback ---

    fn io_error(kind: std::io::ErrorKind) -> ConnectionError {
        ConnectionError::Io(std::io::Error::new(kind, "test"))
    }

    fn is_rejection(event: ConnEvent) -> bool {
        matches!(
            event,
            ConnEvent::Error {
                protocol_rejection: true,
                ..
            }
        )
    }

    #[test]
    fn test_v5_protocol_rejection_classification() {
        use std::io::ErrorKind;
        // Old mosquitto closes the socket; spec-compliant 3.1.1 brokers send a
        // CONNACK the v5 parser can't read (InvalidData).
        assert!(is_rejection(v5_event(Err(io_error(
            ErrorKind::ConnectionAborted
        )))));
        assert!(is_rejection(v5_event(Err(io_error(
            ErrorKind::InvalidData
        )))));
        assert!(is_rejection(v5_event(Err(
            ConnectionError::ConnectionRefused(
                mqttbytes::v5::ConnectReturnCode::UnsupportedProtocolVersion
            )
        ))));
        // Broker down or bad credentials are not protocol problems.
        assert!(!is_rejection(v5_event(Err(io_error(
            ErrorKind::ConnectionRefused
        )))));
        assert!(!is_rejection(v5_event(Err(
            ConnectionError::ConnectionRefused(mqttbytes::v5::ConnectReturnCode::NotAuthorized)
        ))));
        // 3.1.1 is the fallback; it never triggers another one.
        let v311_err = rumqttc::ConnectionError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            "test",
        ));
        assert!(!is_rejection(v311_event(Err(v311_err))));
    }

    fn connection_loop<'a>(
        mode: LoopMode,
        peer_map: &'a websocket::PeerMap,
        mqtt_map: &'a mqtt::BrokerMap,
        notification_buf: &'a websocket::NotificationBuf,
    ) -> ConnectionLoop<'a> {
        let host = "127.0.0.1:18850";
        let (client, _connection) = mqtt::connect_to_mqtt_host(
            &config::BrokerConfig::from_host(host),
            config::MqttProtocol::V5,
        )
        .unwrap();
        mqtt_map.lock().unwrap().insert(
            host.to_string(),
            mqtt::MqttBroker {
                client,
                broker: host.to_string(),
                connected: false,
                topics: HashMap::new(),
                total_bytes: 0,
                total_messages: 0,
                eviction_order: std::collections::VecDeque::new(),
                rate_history: Vec::new(),
                rate_bytes_accumulator: 0,
                rate_last_sample_ms: 0,
                requires_auth: false,
                allowed_roles: Vec::new(),
                protocol: None,
            },
        );
        ConnectionLoop {
            hostname: host.to_string(),
            protocol: config::MqttProtocol::V5,
            mode,
            peer_map,
            mqtt_map,
            notification_buf,
            disconnect_candidate_since: None,
            disconnect_notified: false,
            connected_once: false,
            consecutive_rejections: 0,
        }
    }

    fn rejection() -> ConnEvent {
        ConnEvent::Error {
            message: "connection closed by peer".to_string(),
            protocol_rejection: true,
        }
    }

    #[test]
    fn test_fallback_after_repeated_rejection_only_before_connect() {
        let (pm, mm, nb) = (make_peer_map(), make_mqtt_map(), make_notification_buf());

        // Auto mode: the second consecutive rejection triggers the fallback.
        let mut state = connection_loop(LoopMode::TryV5, &pm, &mm, &nb);
        assert_eq!(state.handle(rejection()), None);
        assert_eq!(state.handle(rejection()), Some(LoopExit::ProtocolRejected));

        // A fixed protocol never falls back.
        let mut state = connection_loop(LoopMode::Fixed, &pm, &mm, &nb);
        state.consecutive_rejections = PROTOCOL_REJECTIONS_BEFORE_FALLBACK;
        assert_eq!(state.handle(rejection()), None);

        // Once MQTT 5 connected, drops are ordinary disconnects.
        let mut state = connection_loop(LoopMode::TryV5, &pm, &mm, &nb);
        assert_eq!(
            state.handle(ConnEvent::ConnAck("Success".to_string())),
            None
        );
        assert_eq!(
            mm.lock().unwrap()["127.0.0.1:18850"].protocol,
            Some(config::MqttProtocol::V5)
        );
        state.consecutive_rejections = PROTOCOL_REJECTIONS_BEFORE_FALLBACK;
        assert_eq!(state.handle(rejection()), None);
    }

    #[test]
    fn test_fallback_connection_exits_on_drop_to_retry_v5() {
        let (pm, mm, nb) = (make_peer_map(), make_mqtt_map(), make_notification_buf());
        let mut state = connection_loop(LoopMode::FallbackV311, &pm, &mm, &nb);
        state.protocol = config::MqttProtocol::V311;
        assert_eq!(
            state.handle(ConnEvent::ConnAck("Success".to_string())),
            None
        );
        let drop = ConnEvent::Error {
            message: "connection reset".to_string(),
            protocol_rejection: false,
        };
        assert_eq!(state.handle(drop), Some(LoopExit::Ended));
    }

    fn make_addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
    }

    fn insert_peer(
        peer_map: &websocket::PeerMap,
        port: u16,
    ) -> (
        SocketAddr,
        futures_channel::mpsc::Receiver<warp::filters::ws::Message>,
    ) {
        let addr = make_addr(port);
        let (tx, rx) = channel(PEER_CHANNEL_CAPACITY);
        peer_map
            .lock()
            .unwrap()
            .insert(addr, websocket::PeerConnection::new(tx));
        (addr, rx)
    }

    // --- deserialize_json_rpc_and_process ---

    #[test]
    fn test_process_invalid_json() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        // Should not panic
        deserialize_json_rpc_and_process(
            "not json",
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
    }

    #[test]
    fn test_process_unknown_method() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let json = r#"{"jsonrpc":"2.0","method":"unknown_method","params":{}}"#;
        // Should not panic
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
    }

    #[test]
    fn test_process_publish() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let json = r#"{"jsonrpc":"2.0","method":"publish","params":{"host":"nonexistent:1883","topic":"test","payload":"hello"}}"#;
        // Should not panic (broker doesn't exist, but it's handled gracefully)
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
    }

    #[test]
    fn test_process_connect_spawns_thread() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&config_path).ok();
        let json =
            r#"{"jsonrpc":"2.0","method":"connect","params":{"hostname":"127.0.0.1:19999"}}"#;

        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );

        // Give the spawned thread a moment to take the lock
        std::thread::sleep(std::time::Duration::from_millis(100));

        // The broker should appear in the mqtt_map
        let map = mqtt_map.lock().unwrap();
        assert!(map.contains_key("127.0.0.1:19999"));
        drop(map);

        // Cleanup
        std::fs::remove_dir_all(&config_path).ok();
    }

    /// Regression: an invalid port used to panic while the broker map was
    /// locked, poisoning it and breaking every later request.
    #[test]
    fn test_process_connect_invalid_address_is_rejected() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&config_path).ok();

        for hostname in ["127.0.0.1:18841x", "127.0.0.1", "127.0.0.1:70000"] {
            let json = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "connect",
                "params": { "hostname": hostname }
            })
            .to_string();
            deserialize_json_rpc_and_process(
                &json,
                &peer_map,
                &mqtt_map,
                &config_path,
                None,
                &make_notification_buf(),
                &websocket::PeerAccess::Password,
            );
        }
        // A bad entry already in brokers.json must not crash either.
        connect_to_broker(
            &config::BrokerConfig::from_host("bad-port:abc"),
            &peer_map,
            &mqtt_map,
            &make_notification_buf(),
        );
        std::thread::sleep(std::time::Duration::from_millis(200));

        assert!(!mqtt_map.is_poisoned(), "broker map must not be poisoned");
        assert!(mqtt_map.lock().unwrap().is_empty());
        assert!(
            !std::path::Path::new(&config_path)
                .join("brokers.json")
                .exists(),
            "invalid brokers must not be persisted"
        );
        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_process_connect_duplicate_broker() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&config_path).ok();

        let json =
            r#"{"jsonrpc":"2.0","method":"connect","params":{"hostname":"127.0.0.1:19998"}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Connect again — should not duplicate
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
        std::thread::sleep(std::time::Duration::from_millis(100));

        assert_eq!(mqtt_map.lock().unwrap().len(), 1);

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_process_remove_nonexistent_broker() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&config_path).ok();

        let json =
            r#"{"jsonrpc":"2.0","method":"remove","params":{"hostname":"nonexistent:1883"}}"#;
        // Should not panic
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_process_save_and_remove_command() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        let commands_path = format!("{}/commands", config_path);
        std::fs::create_dir_all(&commands_path).ok();

        // Save a command
        let save_json = r#"{"jsonrpc":"2.0","method":"save_command","params":{"name":"test_cmd","topic":"t","payload":"p"}}"#;
        deserialize_json_rpc_and_process(
            save_json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );

        let cmd_file = format!("{}/test_cmd.json", commands_path);
        assert!(std::path::Path::new(&cmd_file).exists());

        // Remove the command
        let remove_json =
            r#"{"jsonrpc":"2.0","method":"remove_command","params":{"name":"test_cmd"}}"#;
        deserialize_json_rpc_and_process(
            remove_json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );

        assert!(!std::path::Path::new(&cmd_file).exists());

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_process_save_and_remove_pipeline() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        let pipelines_path = format!("{}/pipelines", config_path);
        std::fs::create_dir_all(&pipelines_path).ok();

        let save_json = r#"{"jsonrpc":"2.0","method":"save_pipeline","params":{"name":"test_pipe","pipeline":[{"topic":"t1"}]}}"#;
        deserialize_json_rpc_and_process(
            save_json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );

        let pipe_file = format!("{}/test_pipe.json", pipelines_path);
        assert!(std::path::Path::new(&pipe_file).exists());

        let remove_json =
            r#"{"jsonrpc":"2.0","method":"remove_pipeline","params":{"name":"test_pipe"}}"#;
        deserialize_json_rpc_and_process(
            remove_json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );

        assert!(!std::path::Path::new(&pipe_file).exists());

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_connect_broadcasts_brokers_to_peers() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&config_path).ok();

        let json =
            r#"{"jsonrpc":"2.0","method":"connect","params":{"hostname":"127.0.0.1:19997"}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );

        // Should have received a broadcast_brokers message, even if a status
        // notification arrived first.
        let mut methods = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            let parsed: serde_json::Value = serde_json::from_str(msg.to_str().unwrap()).unwrap();
            if let Some(method) = parsed["method"].as_str() {
                methods.push(method.to_string());
            }
        }
        assert!(methods.contains(&"mqtt_brokers".to_string()));

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_remove_broker_sends_removal_notification() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&config_path).ok();

        // First connect
        let connect_json =
            r#"{"jsonrpc":"2.0","method":"connect","params":{"hostname":"127.0.0.1:19996"}}"#;
        deserialize_json_rpc_and_process(
            connect_json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Drain the connect broadcast
        while rx.try_recv().is_ok() {}

        // Now remove
        let remove_json =
            r#"{"jsonrpc":"2.0","method":"remove","params":{"hostname":"127.0.0.1:19996"}}"#;
        deserialize_json_rpc_and_process(
            remove_json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );

        // Should receive broker_removal and broadcast_brokers messages
        let mut methods = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let Ok(text) = msg.to_str() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
                    if let Some(method) = parsed["method"].as_str() {
                        methods.push(method.to_string());
                    }
                }
            }
        }
        assert!(methods.contains(&"broker_removal".to_string()));

        assert!(!mqtt_map.lock().unwrap().contains_key("127.0.0.1:19996"));

        std::fs::remove_dir_all(&config_path).ok();
    }

    // --- connect_to_known_brokers ---

    #[test]
    fn test_connect_to_known_brokers_empty_file() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        // Nonexistent file should result in 0 brokers being connected
        connect_to_known_brokers(
            "/nonexistent/brokers.json",
            &peer_map,
            &mqtt_map,
            &make_notification_buf(),
        );
        // Give threads a moment
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(mqtt_map.lock().unwrap().len(), 0);
    }

    // --- Concurrency stress test ---

    #[test]
    fn test_concurrent_process_commands_no_deadlock() {
        use std::sync::Arc;
        use std::thread;

        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        let commands_path = format!("{}/commands", config_path);
        std::fs::create_dir_all(&commands_path).ok();

        // Insert some peers to receive broadcasts
        let mut _receivers = Vec::new();
        for port in 9001..9004 {
            let (_addr, rx) = insert_peer(&peer_map, port);
            _receivers.push(rx);
        }

        let pm = Arc::clone(&peer_map);
        let mm = Arc::clone(&mqtt_map);
        let cp = config_path.clone();

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let pm = Arc::clone(&pm);
                let mm = Arc::clone(&mm);
                let cp = cp.clone();
                thread::spawn(move || {
                    let json = format!(
                        r#"{{"jsonrpc":"2.0","method":"save_command","params":{{"name":"cmd_{}","topic":"t","payload":"p"}}}}"#,
                        i
                    );
                    deserialize_json_rpc_and_process(&json, &pm, &mm, &cp, None, &make_notification_buf(), &websocket::PeerAccess::Password);
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        std::fs::remove_dir_all(&config_path).ok();
    }

    // --- Missing/invalid params edge cases ---

    #[test]
    fn test_process_connect_missing_hostname() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        // "hostname" key is absent
        let json = r#"{"jsonrpc":"2.0","method":"connect","params":{}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
        // Should not panic, no broker added
        assert_eq!(mqtt_map.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_process_connect_hostname_not_string() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let json = r#"{"jsonrpc":"2.0","method":"connect","params":{"hostname": 42}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
        assert_eq!(mqtt_map.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_process_remove_missing_hostname() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let json = r#"{"jsonrpc":"2.0","method":"remove","params":{}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
        // No panic
    }

    #[test]
    fn test_process_publish_missing_host() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let json = r#"{"jsonrpc":"2.0","method":"publish","params":{"topic":"t","payload":"p"}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
    }

    #[test]
    fn test_process_publish_missing_topic() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let json =
            r#"{"jsonrpc":"2.0","method":"publish","params":{"host":"h:1883","payload":"p"}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
    }

    #[test]
    fn test_process_publish_missing_payload() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let json = r#"{"jsonrpc":"2.0","method":"publish","params":{"host":"h:1883","topic":"t"}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
    }

    #[test]
    fn test_process_remove_command_missing_name() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        let commands_path = format!("{}/commands", config_path);
        std::fs::create_dir_all(&commands_path).ok();

        let json = r#"{"jsonrpc":"2.0","method":"remove_command","params":{}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
        // Should not panic

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_process_remove_pipeline_missing_name() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        let pipelines_path = format!("{}/pipelines", config_path);
        std::fs::create_dir_all(&pipelines_path).ok();

        let json = r#"{"jsonrpc":"2.0","method":"remove_pipeline","params":{}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            &config_path,
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_process_subscribe_topic_updates_peer() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let (addr, mut rx) = insert_peer(&peer_map, 9001);

        let json = r#"{"jsonrpc":"2.0","method":"subscribe_topic","params":{"broker":"b:1883","topic":"t/1"}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            Some(addr),
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );

        // Should receive topic_sync_complete (no clear anymore)
        let msg1 = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(msg1.to_str().unwrap()).unwrap();
        assert_eq!(parsed["method"], "topic_sync_complete");
        assert_eq!(parsed["params"]["topic"], "t/1");

        // Peer should have the topic subscribed
        let peers = peer_map.lock().unwrap();
        let peer = peers.get(&addr).unwrap();
        assert_eq!(peer.selected_broker.as_deref(), Some("b:1883"));
        assert!(peer.subscribed_topics.contains("t/1"));
    }

    #[test]
    fn test_process_unsubscribe_topic() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let (addr, _rx) = insert_peer(&peer_map, 9001);

        for method in ["subscribe_topic", "unsubscribe_topic"] {
            let json = format!(
                r#"{{"jsonrpc":"2.0","method":"{method}","params":{{"broker":"b:1883","topic":"t/1"}}}}"#
            );
            deserialize_json_rpc_and_process(
                &json,
                &peer_map,
                &mqtt_map,
                "/tmp",
                Some(addr),
                &make_notification_buf(),
                &websocket::PeerAccess::Password,
            );
        }

        let peers = peer_map.lock().unwrap();
        let peer = peers.get(&addr).unwrap();
        assert!(!peer.subscribed_topics.contains("t/1"));
    }

    #[test]
    fn test_process_subscribe_topic_without_addr_is_noop() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();

        // addr = None → should skip subscribe_topic entirely
        let json = r#"{"jsonrpc":"2.0","method":"subscribe_topic","params":{"broker":"b:1883","topic":"t/1"}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
        // No panic, no state change
    }

    #[test]
    fn test_eviction_keeps_newest_message_per_topic() {
        let cfg = config::BrokerConfig::from_host("127.0.0.1:18839");
        let (client, _conn) = mqtt::connect_to_mqtt_host(&cfg, config::MqttProtocol::V5).unwrap();

        let mut broker = mqtt::MqttBroker {
            client,
            broker: "127.0.0.1:18839".to_string(),
            connected: true,
            topics: HashMap::new(),
            total_bytes: mqtt::max_broker_bytes() + 3,
            total_messages: 3,
            eviction_order: std::collections::VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
            requires_auth: false,
            allowed_roles: Vec::new(),
            protocol: None,
        };

        let mut topic_a = std::collections::VecDeque::new();
        topic_a.push_back(mqtt::MqttMessage {
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            payload: bytes::Bytes::from_static(b"a-old"),
            original_payload_size: 5,
            retain: false,
            properties: None,
        });
        topic_a.push_back(mqtt::MqttMessage {
            timestamp: "2026-01-01T00:00:01Z".to_string(),
            payload: bytes::Bytes::from_static(b"a-new"),
            original_payload_size: 5,
            retain: false,
            properties: None,
        });

        let mut topic_b = std::collections::VecDeque::new();
        topic_b.push_back(mqtt::MqttMessage {
            timestamp: "2026-01-01T00:00:02Z".to_string(),
            payload: bytes::Bytes::from_static(b"b-only"),
            original_payload_size: 6,
            retain: false,
            properties: None,
        });

        broker.topics.insert("t/a".to_string(), topic_a);
        broker.topics.insert("t/b".to_string(), topic_b);
        broker.eviction_order.push_back(("t/a".to_string(), 1));
        broker.eviction_order.push_back(("t/b".to_string(), 1));
        broker.eviction_order.push_back(("t/a".to_string(), 1));

        let evictions = evict_while_preserving_topic_latest(&mut broker);

        assert_eq!(broker.topics["t/a"].len(), 1);
        assert_eq!(
            broker.topics["t/a"].back().unwrap().timestamp,
            "2026-01-01T00:00:01Z"
        );
        assert_eq!(broker.topics["t/b"].len(), 1);
        assert_eq!(broker.total_messages, 2);
        assert!(evictions
            .iter()
            .any(|(topic, count, new_count)| topic == "t/a" && *count == 1 && *new_count == 1));
    }

    // ─── Integration: reconnect after broker restart ─────────────────

    /// Helper: find a free TCP port for mosquitto.
    fn find_free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    /// Helper: start mosquitto on `port`, returning the Child handle.
    fn start_mosquitto(port: u16) -> std::process::Child {
        std::process::Command::new("mosquitto")
            .args(["-p", &port.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("mosquitto must be installed to run integration tests")
    }

    fn mosquitto_available() -> bool {
        std::process::Command::new("mosquitto")
            .arg("-h")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
    }

    /// Helper: block until a TCP connection to `port` succeeds, or panic
    /// after `timeout`.
    fn wait_for_port(port: u16, timeout: std::time::Duration) {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("mosquitto did not become ready on port {port}");
    }

    /// Verify that after a broker restart the backend automatically
    /// reconnects **and** re-subscribes so messages keep flowing.
    #[test]
    fn test_resubscribe_after_broker_restart() {
        if !mosquitto_available() {
            eprintln!("Skipping integration test: mosquitto not available");
            return;
        }

        let port = find_free_port();
        let hostname = format!("127.0.0.1:{port}");

        // 1. Start mosquitto
        let mut mosquitto = start_mosquitto(port);
        wait_for_port(port, std::time::Duration::from_secs(5));

        // 2. Connect our backend
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let notification_buf = make_notification_buf();
        let broker_config = config::BrokerConfig {
            host: hostname.clone(),
            use_tls: false,
            username: None,
            password: None,
            ca_cert: None,
            client_cert: None,
            client_key: None,
            client_key_password: None,
            access_password: None,
            allowed_roles: Vec::new(),
            protocol: None,
        };

        connect_to_broker(&broker_config, &peer_map, &mqtt_map, &notification_buf);

        // Wait for the backend to connect and subscribe
        let connected = (|| {
            for _ in 0..50 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let lock = mqtt_map.lock().unwrap();
                if let Some(b) = lock.get(&hostname) {
                    if b.connected {
                        return true;
                    }
                }
            }
            false
        })();
        assert!(connected, "backend should connect to mosquitto");

        // 3. Publish a message and verify the backend receives it
        {
            let (mut pub_client, mut pub_conn) = rumqttc::Client::new(
                {
                    let mut opts = rumqttc::MqttOptions::new("test-pub", "127.0.0.1", port);
                    opts.set_keep_alive(std::time::Duration::from_secs(5));
                    opts
                },
                10,
            );
            // Drive the connection briefly to establish it
            std::thread::spawn(move || {
                for notification in pub_conn.iter() {
                    if notification.is_err() {
                        break;
                    }
                }
            });
            std::thread::sleep(std::time::Duration::from_millis(500));
            pub_client
                .publish("test/before", rumqttc::QoS::AtLeastOnce, false, b"hello1")
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(500));

            let has_before = mqtt_map
                .lock()
                .unwrap()
                .get(&hostname)
                .map(|b| b.topics.contains_key("test/before"))
                .unwrap_or(false);
            assert!(has_before, "backend should have received test/before");
            let _ = pub_client.disconnect();
        }

        // 4. Kill mosquitto (simulates a broker crash / restart)
        mosquitto.kill().ok();
        mosquitto.wait().ok();

        // Wait until the backend notices the disconnect
        let disconnected = (|| {
            for _ in 0..100 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let lock = mqtt_map.lock().unwrap();
                if let Some(b) = lock.get(&hostname) {
                    if !b.connected {
                        return true;
                    }
                }
            }
            false
        })();
        assert!(disconnected, "backend should detect broker disconnect");

        // 5. Restart mosquitto on the same port
        let mut mosquitto2 = start_mosquitto(port);
        wait_for_port(port, std::time::Duration::from_secs(5));

        // Wait for the backend to reconnect
        let reconnected = (|| {
            for _ in 0..100 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let lock = mqtt_map.lock().unwrap();
                if let Some(b) = lock.get(&hostname) {
                    if b.connected {
                        return true;
                    }
                }
            }
            false
        })();
        assert!(reconnected, "backend should reconnect after broker restart");

        // 6. Publish another message and verify the backend receives it
        //    (this is the key assertion: if we did NOT re-subscribe on
        //    ConnAck, this message would never arrive)
        {
            let (mut pub_client, mut pub_conn) = rumqttc::Client::new(
                {
                    let mut opts = rumqttc::MqttOptions::new("test-pub2", "127.0.0.1", port);
                    opts.set_keep_alive(std::time::Duration::from_secs(5));
                    opts
                },
                10,
            );
            std::thread::spawn(move || {
                for notification in pub_conn.iter() {
                    if notification.is_err() {
                        break;
                    }
                }
            });
            std::thread::sleep(std::time::Duration::from_millis(500));
            pub_client
                .publish("test/after", rumqttc::QoS::AtLeastOnce, false, b"hello2")
                .unwrap();
            // Give the backend time to receive via the re-established subscription
            let received_after = (|| {
                for _ in 0..50 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let lock = mqtt_map.lock().unwrap();
                    if let Some(b) = lock.get(&hostname) {
                        if b.topics.contains_key("test/after") {
                            return true;
                        }
                    }
                }
                false
            })();
            assert!(
                received_after,
                "backend should receive messages after broker restart (re-subscribe on ConnAck)"
            );
            let _ = pub_client.disconnect();
        }

        // Cleanup
        mosquitto2.kill().ok();
        mosquitto2.wait().ok();
    }

    /// MQTT 5 user properties (and other publish properties) must be stored
    /// with the message so the UI can show them.
    #[test]
    fn test_mqtt5_user_properties_are_stored() {
        if !mosquitto_available() {
            eprintln!("Skipping integration test: mosquitto not available");
            return;
        }

        let port = find_free_port();
        let hostname = format!("127.0.0.1:{port}");
        let mut mosquitto = start_mosquitto(port);
        wait_for_port(port, std::time::Duration::from_secs(5));

        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let notification_buf = make_notification_buf();
        let broker_config = config::BrokerConfig::from_host(&hostname);
        connect_to_broker(&broker_config, &peer_map, &mqtt_map, &notification_buf);

        let connected = (0..50).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            mqtt_map
                .lock()
                .unwrap()
                .get(&hostname)
                .is_some_and(|b| b.connected)
        });
        assert!(connected, "backend should connect to mosquitto");

        let (pub_client, mut pub_conn) = rumqttc::v5::Client::new(
            rumqttc::v5::MqttOptions::new("test-pub-props", "127.0.0.1", port),
            10,
        );
        std::thread::spawn(move || {
            for notification in pub_conn.iter() {
                if notification.is_err() {
                    break;
                }
            }
        });
        let props = rumqttc::v5::mqttbytes::v5::PublishProperties {
            user_properties: vec![
                ("trace-id".to_string(), "abc123".to_string()),
                ("tag".to_string(), "one".to_string()),
                ("tag".to_string(), "two".to_string()),
            ],
            content_type: Some("application/json".to_string()),
            correlation_data: Some(bytes::Bytes::from_static(&[0xde, 0xad, 0xbe, 0xef])),
            ..Default::default()
        };
        pub_client
            .publish_with_properties(
                "test/props",
                rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
                false,
                "{}",
                props,
            )
            .unwrap();

        mqtt::publish_message(
            &hostname,
            "test/sent",
            "x",
            false,
            vec![("origin".to_string(), "inspector".to_string())],
            &mqtt_map,
        );
        let sent = (0..50).find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let lock = mqtt_map.lock().unwrap();
            lock.get(&hostname)
                .and_then(|b| b.topics.get("test/sent"))
                .and_then(|msgs| msgs.back())
                .map(|m| m.properties.clone())
        });
        let sent_properties = sent
            .expect("backend should receive its own publish")
            .expect("own publish should carry user properties");
        assert_eq!(
            sent_properties.user_properties,
            vec![("origin".to_string(), "inspector".to_string())]
        );

        let stored = (0..50).find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let lock = mqtt_map.lock().unwrap();
            lock.get(&hostname)
                .and_then(|b| b.topics.get("test/props"))
                .and_then(|msgs| msgs.back())
                .map(|m| m.properties.clone())
        });
        let _ = pub_client.disconnect();
        mosquitto.kill().ok();
        mosquitto.wait().ok();

        let properties = stored
            .expect("backend should have received test/props")
            .expect("message should carry properties");
        assert_eq!(
            properties.user_properties,
            vec![
                ("trace-id".to_string(), "abc123".to_string()),
                ("tag".to_string(), "one".to_string()),
                ("tag".to_string(), "two".to_string()),
            ]
        );
        assert_eq!(properties.content_type.as_deref(), Some("application/json"));
        assert_eq!(properties.correlation_data.as_deref(), Some("0xdeadbeef"));
    }

    /// Starts mosquitto with a TLS listener that requires a client certificate
    /// signed by the test CA.
    fn start_mosquitto_mtls(
        port: u16,
        pki: &crate::server::test_certs::TestPki,
    ) -> std::process::Child {
        let conf = pki.dir.join("mosquitto.conf");
        std::fs::write(
            &conf,
            format!(
                "listener {port} 127.0.0.1\n\
                 allow_anonymous true\n\
                 cafile {}\n\
                 certfile {}\n\
                 keyfile {}\n\
                 require_certificate true\n",
                pki.path("ca.crt"),
                pki.path("server.crt"),
                pki.path("server.key"),
            ),
        )
        .unwrap();
        std::process::Command::new("mosquitto")
            .args(["-c", &conf.to_string_lossy()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("mosquitto must be installed to run integration tests")
    }

    fn wait_until_connected(mqtt_map: &mqtt::BrokerMap, hostname: &str, attempts: usize) -> bool {
        for _ in 0..attempts {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if mqtt_map
                .lock()
                .unwrap()
                .get(hostname)
                .is_some_and(|b| b.connected)
            {
                return true;
            }
        }
        false
    }

    #[test]
    fn test_integration_mutual_tls_with_client_certificate() {
        if !mosquitto_available() || !crate::server::test_certs::openssl_available() {
            eprintln!("Skipping integration test: mosquitto or openssl not available");
            return;
        }
        let pki = crate::server::test_certs::generate();
        let port = find_free_port();
        let hostname = format!("127.0.0.1:{port}");
        let mut mosquitto = start_mosquitto_mtls(port, &pki);
        wait_for_port(port, std::time::Duration::from_secs(5));

        // Paths are given relative to the "config dir" (the PKI dir here) to
        // exercise the same resolution brokers.json uses.
        let broker_config = config::BrokerConfig {
            host: hostname.clone(),
            use_tls: false, // implied by the certificate settings
            username: None,
            password: None,
            ca_cert: Some("ca.crt".to_string()),
            client_cert: Some("client.crt".to_string()),
            client_key: Some("client.key".to_string()),
            client_key_password: None,
            access_password: None,
            allowed_roles: Vec::new(),
            protocol: None,
        }
        .with_resolved_cert_paths(&pki.dir);

        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let notification_buf = make_notification_buf();
        connect_to_broker(&broker_config, &peer_map, &mqtt_map, &notification_buf);

        let connected = wait_until_connected(&mqtt_map, &hostname, 50);
        remove_broker(&hostname, &peer_map, &mqtt_map);
        mosquitto.kill().ok();
        mosquitto.wait().ok();
        assert!(
            connected,
            "backend should connect using its client certificate"
        );
    }

    #[test]
    fn test_integration_mutual_tls_rejected_without_client_certificate() {
        if !mosquitto_available() || !crate::server::test_certs::openssl_available() {
            eprintln!("Skipping integration test: mosquitto or openssl not available");
            return;
        }
        let pki = crate::server::test_certs::generate();
        let port = find_free_port();
        let hostname = format!("127.0.0.1:{port}");
        let mut mosquitto = start_mosquitto_mtls(port, &pki);
        wait_for_port(port, std::time::Duration::from_secs(5));

        // Trusts the server, but presents no client certificate.
        let broker_config = config::BrokerConfig {
            host: hostname.clone(),
            use_tls: true,
            username: None,
            password: None,
            ca_cert: Some(pki.path("ca.crt")),
            client_cert: None,
            client_key: None,
            client_key_password: None,
            access_password: None,
            allowed_roles: Vec::new(),
            protocol: None,
        };

        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let notification_buf = make_notification_buf();
        connect_to_broker(&broker_config, &peer_map, &mqtt_map, &notification_buf);

        let connected = wait_until_connected(&mqtt_map, &hostname, 20);
        remove_broker(&hostname, &peer_map, &mqtt_map);
        mosquitto.kill().ok();
        mosquitto.wait().ok();
        assert!(
            !connected,
            "broker must reject clients without a certificate"
        );
    }

    #[test]
    fn test_integration_mutual_tls_with_encrypted_client_key() {
        if !mosquitto_available() || !crate::server::test_certs::openssl_available() {
            eprintln!("Skipping integration test: mosquitto or openssl not available");
            return;
        }
        let pki = crate::server::test_certs::generate();
        let port = find_free_port();
        let hostname = format!("127.0.0.1:{port}");
        let mut mosquitto = start_mosquitto_mtls(port, &pki);
        wait_for_port(port, std::time::Duration::from_secs(5));

        let broker_config = config::BrokerConfig {
            host: hostname.clone(),
            use_tls: true,
            username: None,
            password: None,
            ca_cert: Some(pki.path("ca.crt")),
            client_cert: Some(pki.path("client_rsa.crt")),
            client_key: Some(pki.path("client_rsa_aes.key")),
            client_key_password: Some(crate::server::test_certs::KEY_PASSWORD.to_string()),
            access_password: None,
            allowed_roles: Vec::new(),
            protocol: None,
        };

        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let notification_buf = make_notification_buf();
        connect_to_broker(&broker_config, &peer_map, &mqtt_map, &notification_buf);

        let connected = wait_until_connected(&mqtt_map, &hostname, 50);
        remove_broker(&hostname, &peer_map, &mqtt_map);
        mosquitto.kill().ok();
        mosquitto.wait().ok();
        assert!(
            connected,
            "backend should connect using its encrypted client key"
        );
    }

    // --- access_password gates authenticate_broker ---

    fn authenticate(config_path: &str, peer_map: &websocket::PeerMap, addr: SocketAddr, pw: &str) {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "authenticate_broker",
            "params": { "hostname": "secured:8883", "password": pw }
        });
        deserialize_json_rpc_and_process(
            &msg.to_string(),
            peer_map,
            &make_mqtt_map(),
            config_path,
            Some(addr),
            &make_notification_buf(),
            &websocket::PeerAccess::Password,
        );
    }

    fn is_authenticated(peer_map: &websocket::PeerMap, addr: SocketAddr) -> bool {
        peer_map.lock().unwrap()[&addr]
            .authenticated_brokers
            .contains("secured:8883")
    }

    #[test]
    fn test_authenticate_broker_with_access_password() {
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&config_path).unwrap();
        // Certificate-only broker (no MQTT credentials) protected by access_password;
        // the MQTT password of another broker must not interfere.
        std::fs::write(
            format!("{config_path}/brokers.json"),
            r#"[{"host":"secured:8883","client_cert":"c.crt","client_key":"c.key","access_password":"view-pw"}]"#,
        )
        .unwrap();
        let peer_map = make_peer_map();
        let (addr, _rx) = insert_peer(&peer_map, 40100);

        authenticate(&config_path, &peer_map, addr, "wrong");
        assert!(!is_authenticated(&peer_map, addr));

        authenticate(&config_path, &peer_map, addr, "view-pw");
        assert!(is_authenticated(&peer_map, addr));

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_authenticate_broker_access_password_overrides_mqtt_password() {
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&config_path).unwrap();
        std::fs::write(
            format!("{config_path}/brokers.json"),
            r#"[{"host":"secured:8883","username":"u","password":"mqtt-pw","access_password":"view-pw"}]"#,
        )
        .unwrap();
        let peer_map = make_peer_map();
        let (addr, _rx) = insert_peer(&peer_map, 40101);

        authenticate(&config_path, &peer_map, addr, "mqtt-pw");
        assert!(!is_authenticated(&peer_map, addr));

        authenticate(&config_path, &peer_map, addr, "view-pw");
        assert!(is_authenticated(&peer_map, addr));

        std::fs::remove_dir_all(&config_path).ok();
    }

    // --- claims mode: admin-only configuration changes ---

    fn claims_access(roles: &[&str], is_admin: bool) -> websocket::PeerAccess {
        websocket::PeerAccess::Claims(crate::server::oidc::Identity {
            subject: "sub".to_string(),
            username: Some("bob".to_string()),
            roles: roles.iter().map(|r| r.to_string()).collect(),
            expires_at: i64::MAX,
            is_admin,
        })
    }

    /// Sets the peer's access in the map (as `handle_socket` does) and returns
    /// it, so tests pass the same access along with each request.
    fn set_identity(
        peer_map: &websocket::PeerMap,
        addr: SocketAddr,
        roles: &[&str],
        is_admin: bool,
    ) -> websocket::PeerAccess {
        let access = claims_access(roles, is_admin);
        peer_map.lock().unwrap().get_mut(&addr).unwrap().access = access.clone();
        access
    }

    fn config_change_calls() -> [String; 6] {
        [
            rpc(
                "save_command",
                serde_json::json!({"name": "c", "topic": "t", "payload": "p"}),
            ),
            rpc(
                "save_pipeline",
                serde_json::json!({"name": "p", "pipeline": [{"topic": "t"}]}),
            ),
            rpc("remove", serde_json::json!({"hostname": "keep:1883"})),
            rpc(
                "connect",
                serde_json::json!({"hostname": "127.0.0.1:19995"}),
            ),
            rpc("remove_command", serde_json::json!({"name": "kept"})),
            rpc("remove_pipeline", serde_json::json!({"name": "kept"})),
        ]
    }

    /// Config dir with one broker, one command and one pipeline that a denied
    /// peer must not be able to remove.
    fn config_with_kept_items() -> String {
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(format!("{config_path}/commands")).unwrap();
        std::fs::create_dir_all(format!("{config_path}/pipelines")).unwrap();
        std::fs::write(
            format!("{config_path}/brokers.json"),
            r#"[{"host":"keep:1883"}]"#,
        )
        .unwrap();
        std::fs::write(
            format!("{config_path}/commands/kept.json"),
            r#"{"name":"kept","topic":"t","payload":"p"}"#,
        )
        .unwrap();
        std::fs::write(
            format!("{config_path}/pipelines/kept.json"),
            r#"{"name":"kept","pipeline":[]}"#,
        )
        .unwrap();
        config_path
    }

    fn assert_config_unchanged(config_path: &str, mqtt_map: &mqtt::BrokerMap) {
        assert!(!std::path::Path::new(&format!("{config_path}/commands/c.json")).exists());
        assert!(!std::path::Path::new(&format!("{config_path}/pipelines/p.json")).exists());
        let brokers = config::get_known_brokers(&format!("{config_path}/brokers.json"));
        assert_eq!(brokers.len(), 1);
        assert_eq!(brokers[0].host, "keep:1883");
        assert!(mqtt_map.lock().unwrap().is_empty());
        assert!(std::path::Path::new(&format!("{config_path}/commands/kept.json")).exists());
        assert!(std::path::Path::new(&format!("{config_path}/pipelines/kept.json")).exists());
    }

    fn rpc(method: &str, params: serde_json::Value) -> String {
        serde_json::json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string()
    }

    #[test]
    fn test_claims_mode_non_admin_cannot_change_config() {
        let config_path = config_with_kept_items();
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let buf = make_notification_buf();
        let (addr, _rx) = insert_peer(&peer_map, 40200);
        let viewer = set_identity(&peer_map, addr, &["viewer"], false);

        let calls = config_change_calls();
        for call in &calls {
            deserialize_json_rpc_and_process(
                call,
                &peer_map,
                &mqtt_map,
                &config_path,
                Some(addr),
                &buf,
                &viewer,
            );
        }
        assert_config_unchanged(&config_path, &mqtt_map);

        // Admin may.
        let admin = set_identity(&peer_map, addr, &["admin"], true);
        deserialize_json_rpc_and_process(
            &calls[0],
            &peer_map,
            &mqtt_map,
            &config_path,
            Some(addr),
            &buf,
            &admin,
        );
        assert!(std::path::Path::new(&format!("{config_path}/commands/c.json")).exists());

        std::fs::remove_dir_all(&config_path).ok();
    }

    /// Regression: a peer evicted from `peer_map` (e.g. as a slow consumer that
    /// stopped reading) can still deliver requests over its open socket. Its
    /// access must still be enforced, not fall back to password mode.
    #[test]
    fn test_claims_mode_evicted_peer_stays_denied() {
        let config_path = config_with_kept_items();
        let peer_map = make_peer_map(); // peer intentionally not in the map
        let mqtt_map = make_mqtt_map();
        let buf = make_notification_buf();
        let viewer = claims_access(&["viewer"], false);

        for call in &config_change_calls() {
            deserialize_json_rpc_and_process(
                call,
                &peer_map,
                &mqtt_map,
                &config_path,
                Some(make_addr(40205)),
                &buf,
                &viewer,
            );
        }
        assert_config_unchanged(&config_path, &mqtt_map);

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_claims_mode_ignores_broker_password_login() {
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&config_path).unwrap();
        std::fs::write(
            format!("{config_path}/brokers.json"),
            r#"[{"host":"secured:8883","access_password":"view-pw","allowed_roles":["plant-a"]}]"#,
        )
        .unwrap();
        let peer_map = make_peer_map();
        let (addr, _rx) = insert_peer(&peer_map, 40201);
        let viewer = set_identity(&peer_map, addr, &["viewer"], false);

        let call = rpc(
            "authenticate_broker",
            serde_json::json!({"hostname": "secured:8883", "password": "view-pw"}),
        );
        deserialize_json_rpc_and_process(
            &call,
            &peer_map,
            &make_mqtt_map(),
            &config_path,
            Some(addr),
            &make_notification_buf(),
            &viewer,
        );
        assert!(!peer_map.lock().unwrap()[&addr]
            .authenticated_brokers
            .contains("secured:8883"));

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_claims_mode_admin_adds_broker_with_allowed_roles() {
        let config_path = format!("/tmp/mqtt_test_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&config_path).unwrap();
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let (admin, _ra) = insert_peer(&peer_map, 40202);
        let (plant, _rp) = insert_peer(&peer_map, 40203);
        let (other, _ro) = insert_peer(&peer_map, 40204);
        let admin_access = set_identity(&peer_map, admin, &["admin"], true);
        set_identity(&peer_map, plant, &["plant-a"], false);
        set_identity(&peer_map, other, &["viewer"], false);

        let call = rpc(
            "connect",
            serde_json::json!({"hostname": "127.0.0.1:19996", "allowed_roles": ["plant-a", " "]}),
        );
        deserialize_json_rpc_and_process(
            &call,
            &peer_map,
            &mqtt_map,
            &config_path,
            Some(admin),
            &make_notification_buf(),
            &admin_access,
        );

        let brokers = config::get_known_brokers(&format!("{config_path}/brokers.json"));
        assert_eq!(brokers[0].allowed_roles, vec!["plant-a"]);
        let peers = peer_map.lock().unwrap();
        assert!(peers[&plant]
            .authenticated_brokers
            .contains("127.0.0.1:19996"));
        assert!(!peers[&other]
            .authenticated_brokers
            .contains("127.0.0.1:19996"));
        assert!(!peers[&admin]
            .authenticated_brokers
            .contains("127.0.0.1:19996"));
        drop(peers);

        remove_broker("127.0.0.1:19996", &peer_map, &mqtt_map);
        std::fs::remove_dir_all(&config_path).ok();
    }
}
