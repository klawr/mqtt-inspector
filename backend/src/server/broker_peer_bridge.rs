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

use std::collections::HashMap;

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

fn loop_forever(
    mut connection: rumqttc::Connection,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    notification_buf: &websocket::NotificationBuf,
) {
    let (ip, port) = connection.eventloop.mqtt_options.broker_address();
    let hostname = format!("{ip}:{port}");

    for notification in connection.iter() {
        match notification {
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(p))) => {
                let retain = p.retain;
                let payload = if p.payload.len() > mqtt::max_message_size() {
                    bytes::Bytes::from(std::format!(
                        "Payload size limit exceeded: {}.\nThe message is probably fine, but is is too large to be displayed.",
                        p.payload.len()
                    ))
                } else {
                    p.payload
                };
                let timestamp = chrono::Utc::now().to_rfc3339();
                let (total_bytes, new_sample, topic_message_count, evictions, rate_history_len) = {
                    let mut mqtt_lock = mqtt_map.lock().unwrap();
                    let broker = match mqtt_lock.get_mut(&hostname) {
                        Some(b) => b,
                        None => {
                            println!("Broker {hostname} not found in map. Exiting loop.");
                            break;
                        }
                    };
                    broker.connected = true;
                    let msg_bytes = payload.len();
                    let new_msg = mqtt::MqttMessage {
                        timestamp: timestamp.clone(),
                        payload: payload.clone(),
                        retain,
                    };
                    let topic_name = p.topic.clone();
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

                    // Evict oldest messages using the O(1) eviction queue
                    // Track which topics had messages evicted (lazy allocation)
                    let needs_eviction = broker.total_bytes > mqtt::max_broker_bytes();
                    let mut eviction_counts: HashMap<String, usize> = if needs_eviction {
                        HashMap::new()
                    } else {
                        HashMap::with_capacity(0)
                    };
                    while broker.total_bytes > mqtt::max_broker_bytes() {
                        match broker.eviction_order.pop_front() {
                            Some((topic_key, payload_len)) => {
                                if let Some(topic_vec) = broker.topics.get_mut(&topic_key) {
                                    if !topic_vec.is_empty() {
                                        topic_vec.pop_front();
                                        broker.total_bytes =
                                            broker.total_bytes.saturating_sub(payload_len);
                                        broker.total_messages =
                                            broker.total_messages.saturating_sub(1);
                                        *eviction_counts.entry(topic_key.clone()).or_insert(0) += 1;
                                        if topic_vec.is_empty() {
                                            broker.topics.remove(&topic_key);
                                        }
                                    }
                                }
                            }
                            None => break,
                        }
                    }

                    // Build eviction notification data: (topic, evicted_count, new_topic_count)
                    let evictions: Vec<(String, usize, usize)> = eviction_counts
                        .into_iter()
                        .map(|(topic_key, count)| {
                            let new_count =
                                broker.topics.get(&topic_key).map(|v| v.len()).unwrap_or(0);
                            (topic_key, count, new_count)
                        })
                        .collect();

                    let topic_message_count =
                        broker.topics.get(&p.topic).map(|v| v.len()).unwrap_or(0);

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
                    println!(
                        "Rate sample for {hostname}: {:.1} B/s, {} total bytes, history entry #{rate_history_len}",
                        sample.bytes_per_second,
                        sample.total_bytes,
                    );
                    websocket::send_rate_sample_to_peers(peer_map, &hostname, sample);
                }
                // Buffer eviction notifications (will be flushed in batch)
                if !evictions.is_empty() {
                    websocket::buffer_evictions(notification_buf, &hostname, &evictions);
                }
                // Buffer lightweight meta (will be flushed in batch)
                websocket::buffer_message_meta(
                    notification_buf,
                    &hostname,
                    &p.topic,
                    &timestamp,
                    payload.len(),
                    total_bytes,
                    topic_message_count,
                    retain,
                );
                // Send full payload ONLY to peers watching this topic
                websocket::send_message_to_subscribed_peers(
                    peer_map,
                    &hostname,
                    &p.topic,
                    &payload,
                    total_bytes,
                    &timestamp,
                    retain,
                );
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(a))) => {
                {
                    let mut mqtt_lock = mqtt_map.lock().unwrap();
                    if let Some(broker) = mqtt_lock.get_mut(&hostname) {
                        broker.connected = true;
                    }
                }
                // Small update for the peers already connected
                websocket::send_broker_status_to_peers(peer_map, &hostname, true);
                println!("Connection event: {:?} for {:?}", a.code, hostname);
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::Disconnect)) => {
                println!("Disconnect event for {hostname:?}");
            }
            Ok(_) => {
                // PingReq, PingResp, SubAck, etc. — no lock needed
            }
            Err(rumqttc::ConnectionError::MqttState(rumqttc::StateError::Deserialization(
                rumqttc::mqttbytes::Error::PayloadSizeLimitExceeded(p),
            ))) => {
                let payload = bytes::Bytes::from(std::format!("Payload size limit exceeded: {p}."));
                println!("Payload size limit exceeded: {p}");
                let timestamp = chrono::Utc::now().to_rfc3339();
                websocket::buffer_message_meta(
                    notification_buf,
                    &hostname,
                    "$ERROR",
                    &timestamp,
                    payload.len(),
                    0,
                    0,
                    false,
                );
                websocket::send_message_to_subscribed_peers(
                    peer_map, &hostname, "$ERROR", &payload, 0, &timestamp, false,
                )
            }
            Err(rumqttc::ConnectionError::MqttState(err)) => {
                {
                    let mut mqtt_lock = mqtt_map.lock().unwrap();
                    if let Some(broker) = mqtt_lock.get_mut(&hostname) {
                        broker.connected = false;
                    }
                }
                println!("MqttState error for {hostname:?}: {err}. Will retry.");
                websocket::send_broker_status_to_peers(peer_map, &hostname, false);
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
            Err(_) => {
                {
                    let mut mqtt_lock = mqtt_map.lock().unwrap();
                    if let Some(broker) = mqtt_lock.get_mut(&hostname) {
                        broker.connected = false;
                    }
                }
                // Small update for the peers already connected
                websocket::send_broker_status_to_peers(peer_map, &hostname, false);
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        }
    }
    // Connection iterator ended — broker disconnected or was removed
    println!("Connection loop for {hostname:?} ended. Marking broker as disconnected.");
    {
        let mut mqtt_lock = mqtt_map.lock().unwrap();
        if let Some(broker) = mqtt_lock.get_mut(&hostname) {
            broker.connected = false;
        }
    }
    websocket::send_broker_status_to_peers(peer_map, &hostname, false);
}

pub fn connect_to_known_brokers(
    broker_path: &str,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    notification_buf: &websocket::NotificationBuf,
) {
    config::get_known_brokers(broker_path)
        .iter()
        .for_each(|broker_config| {
            connect_to_broker(broker_config, peer_map, mqtt_map, notification_buf);
        });
}

pub fn deserialize_json_rpc_and_process(
    json_rpc: &str,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    config_path: &str,
    addr: Option<std::net::SocketAddr>,
    notification_buf: &websocket::NotificationBuf,
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
            };
            let has_password = broker_config
                .password
                .as_ref()
                .is_some_and(|p| !p.is_empty());
            connect_to_broker(&broker_config, peer_map, mqtt_map, notification_buf);
            let broker_path = std::format!("{}/brokers.json", &config_path);
            config::add_to_brokers(&broker_path, &broker_config);
            websocket::broadcast_brokers(peer_map, mqtt_map);
            // Auto-authenticate the peer that added a password-protected broker,
            // and auto-authenticate ALL peers for non-password brokers.
            let broker_key = broker_config.key().to_string();
            if has_password {
                if let Some(peer_addr) = addr {
                    let mut peers = peer_map.lock().unwrap();
                    if let Some(peer) = peers.get_mut(&peer_addr) {
                        peer.authenticated_brokers.insert(broker_key);
                    }
                }
            } else {
                websocket::auto_authenticate_all_peers_for_broker(peer_map, &broker_key);
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
            let broker_path = std::format!("{}/brokers.json", &config_path);
            config::remove_from_brokers(&broker_path, &hostname);
            websocket::broadcast_brokers(peer_map, mqtt_map)
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
            mqtt::publish_message(&host, topic, payload, retain, mqtt_map);
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
        "select_topic" => {
            if let Some(peer_addr) = addr {
                let broker = message.params["broker"].as_str();
                let topic = message.params["topic"].as_str();
                websocket::handle_select_topic(peer_map, mqtt_map, peer_addr, broker, topic);
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
                Some(cfg) => match &cfg.password {
                    Some(p) if !p.is_empty() => constant_time_eq(supplied_password, p),
                    _ => true, // No password required
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
        let (client, connection) = mqtt::connect_to_mqtt_host(broker_config);
        let requires_auth = broker_config
            .password
            .as_ref()
            .is_some_and(|p| !p.is_empty());
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
        };

        mqtt_lock.insert(mqtt_host.to_string(), broker);
        drop(mqtt_lock);

        loop_forever(connection, peer_map, mqtt_map, notification_buf);
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
                    deserialize_json_rpc_and_process(&json, &pm, &mm, &cp, None, &make_notification_buf());
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
        );

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_process_select_topic_updates_peer() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let (addr, mut rx) = insert_peer(&peer_map, 9001);

        let json = r#"{"jsonrpc":"2.0","method":"select_topic","params":{"broker":"b:1883","topic":"t/1"}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            Some(addr),
            &make_notification_buf(),
        );

        // Should receive topic_messages_clear + topic_sync_complete
        let msg1 = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(msg1.to_str().unwrap()).unwrap();
        assert_eq!(parsed["method"], "topic_messages_clear");

        // Peer should have selection set
        let peers = peer_map.lock().unwrap();
        let peer = peers.get(&addr).unwrap();
        assert_eq!(peer.selected_broker.as_deref(), Some("b:1883"));
        assert_eq!(peer.selected_topic.as_deref(), Some("t/1"));
    }

    #[test]
    fn test_process_select_topic_without_addr_is_noop() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();

        // addr = None → should skip select_topic entirely
        let json = r#"{"jsonrpc":"2.0","method":"select_topic","params":{"broker":"b:1883","topic":"t/1"}}"#;
        deserialize_json_rpc_and_process(
            json,
            &peer_map,
            &mqtt_map,
            "/tmp",
            None,
            &make_notification_buf(),
        );
        // No panic, no state change
    }
}
