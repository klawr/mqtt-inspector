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

fn loop_forever(
    mut connection: rumqttc::Connection,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
) {
    let (ip, port) = connection.eventloop.mqtt_options.broker_address();
    let hostname = format!("{}:{}", ip, port);

    for notification in connection.iter() {
        let mut mqtt_lock = mqtt_map.lock().unwrap();
        // Check if broker is still in map:
        if !mqtt_lock.contains_key(&hostname) {
            println!("Broker {} not found in map. Exiting loop.", hostname);
            break;
        }
        match notification {
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(p))) => {
                let payload = if p.payload.len() > mqtt::max_message_size() {
                    bytes::Bytes::from(std::format!(
                        "Payload size limit exceeded: {}.\nThe message is probably fine, but is is too large to be displayed.",
                        p.payload.len()
                    ))
                } else {
                    p.payload
                };
                mqtt_lock.entry(hostname.clone()).and_modify(|broker| {
                    broker.connected = true;
                    let msg_bytes = payload.len();
                    let new_msg = mqtt::MqttMessage {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        payload: payload.to_vec(),
                    };
                    if let Some(topic_vec) = broker.topics.get_mut(&p.topic) {
                        topic_vec.push(new_msg);
                    } else {
                        broker.topics.insert(p.topic.clone(), vec![new_msg]);
                    }
                    broker.total_bytes += msg_bytes;

                    // Evict oldest messages across all topics until under the byte cap
                    while broker.total_bytes > mqtt::max_broker_bytes() {
                        // Find the topic with the oldest first message
                        let oldest_topic = broker
                            .topics
                            .iter()
                            .filter(|(_, msgs)| !msgs.is_empty())
                            .min_by_key(|(_, msgs)| msgs[0].timestamp.clone())
                            .map(|(k, _)| k.clone());

                        match oldest_topic {
                            Some(key) => {
                                if let Some(topic_vec) = broker.topics.get_mut(&key) {
                                    let removed = topic_vec.remove(0);
                                    broker.total_bytes =
                                        broker.total_bytes.saturating_sub(removed.payload.len());
                                    if topic_vec.is_empty() {
                                        broker.topics.remove(&key);
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                });
                drop(mqtt_lock);
                websocket::send_message_to_peers(peer_map, &hostname, &p.topic, &payload);
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(a))) => {
                mqtt_lock.entry(hostname.clone()).and_modify(|broker| {
                    broker.connected = true;
                });
                drop(mqtt_lock);
                // Small update for the peers already connected
                websocket::send_broker_status_to_peers(peer_map, &hostname, true);
                println!("Connection event: {:?} for {:?}", a.code, hostname);
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::Disconnect)) => {
                println!("Disconnect event for {:?}", hostname);
            }
            Ok(_) => {
                // maybe handle other events
            }
            Err(rumqttc::ConnectionError::MqttState(rumqttc::StateError::Deserialization(
                rumqttc::mqttbytes::Error::PayloadSizeLimitExceeded(p),
            ))) => {
                drop(mqtt_lock);
                let payload =
                    bytes::Bytes::from(std::format!("Payload size limit exceeded: {}.", p));
                println!("Payload size limit exceeded: {}", p);
                websocket::send_message_to_peers(peer_map, &hostname, "$ERROR", &payload)
            }
            Err(rumqttc::ConnectionError::MqttState(err)) => {
                println!(
                    "Got ConnectionAborted event for {:?}. Error: {}. Stopping loop",
                    hostname, err
                );
                break;
            }
            Err(_err) => {
                // Update the connection status of the broker
                mqtt_lock.entry(hostname.clone()).and_modify(|broker| {
                    broker.connected = false;
                });
                drop(mqtt_lock);
                // Small update for the peers already connected
                websocket::send_broker_status_to_peers(peer_map, &hostname, false);
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        }
    }
}

pub fn connect_to_known_brokers(
    broker_path: &str,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
) {
    config::get_known_brokers(broker_path)
        .iter()
        .for_each(|broker| {
            connect_to_broker(broker, peer_map, mqtt_map);
        });
}

pub fn deserialize_json_rpc_and_process(
    json_rpc: &str,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    config_path: &str,
) {
    let result = jsonrpc::deserialize_json_rpc(json_rpc);
    if result.is_err() {
        println!("Error deserializing JSON-RPC: {:?}", result.err());
        return;
    }
    let message = result.unwrap();
    println!(
        "Got method \"{}\" with params {}",
        message.method,
        message.params.clone()
    );
    match message.method {
        "connect" => {
            let hostname = message.params["hostname"]
                .to_string()
                .trim_matches('"')
                .to_string();
            connect_to_broker(&hostname, peer_map, mqtt_map);
            let broker_path = std::format!("{}/brokers.json", &config_path);
            config::add_to_brokers(&broker_path, &hostname);
            websocket::broadcast_brokers(peer_map, mqtt_map)
        }
        "remove" => {
            let hostname = message.params["hostname"]
                .to_string()
                .trim_matches('"')
                .to_string();
            remove_broker(&hostname, peer_map, mqtt_map);
            let broker_path = std::format!("{}/brokers.json", &config_path);
            config::remove_from_brokers(&broker_path, &hostname);
            websocket::broadcast_brokers(peer_map, mqtt_map)
        }
        "publish" => {
            let host = message.params["host"]
                .to_string()
                .trim_matches('"')
                .to_string();
            let topic = message.params["topic"].as_str().unwrap();
            let payload = message.params["payload"].as_str().unwrap();
            mqtt::publish_message(&host, topic, payload, mqtt_map);
        }
        "save_command" => {
            let command_path: String = std::format!("{}/commands", config_path);
            config::add_to_commands(&command_path, message.params);
            websocket::broadcast_commands(peer_map, config_path);
        }
        "remove_command" => {
            let command_path: String = std::format!("{}/commands", config_path);
            config::remove_from_commands(&command_path, message.params);
            websocket::broadcast_commands(peer_map, config_path);
        }
        "save_pipeline" => {
            let pipelines_path = std::format!("{}/pipelines", config_path);
            config::add_to_pipelines(&pipelines_path, message.params);
            websocket::broadcast_pipelines(peer_map, config_path);
        }
        "remove_pipeline" => {
            let pipelines_path = std::format!("{}/pipelines", config_path);
            config::remove_from_pipelines(&pipelines_path, message.params);
            websocket::broadcast_pipelines(peer_map, config_path);
        }
        _ => {
            // Implement other methods
        }
    }
    // Implement deserialization and processing of JSON-RPC message
}

fn connect_to_broker(mqtt_host: &str, peer_map: &websocket::PeerMap, mqtt_map: &mqtt::BrokerMap) {
    let mqtt_host_clone = mqtt_host.trim_matches('"').to_string();
    let mqtt_map_clone = mqtt_map.clone();
    let peer_map_clone = peer_map.clone();

    std::thread::spawn(move || {
        connect_to_mqtt_client_and_loop_forever(&mqtt_host_clone, &mqtt_map_clone, &peer_map_clone);
    });
}

fn connect_to_mqtt_client_and_loop_forever(
    mqtt_host: &String,
    mqtt_map: &mqtt::BrokerMap,
    peer_map: &websocket::PeerMap,
) {
    let mut mqtt_lock = mqtt_map.lock().unwrap();
    let mqtt_client = mqtt_lock.iter().find(|entry| entry.0 == mqtt_host);

    if mqtt_client.is_some() {
        println!("MQTT-Client for {} already exists.", mqtt_host);
    } else {
        println!(
            "MQTT-Client for {} does not exist. Creating new client.",
            mqtt_host
        );
        let (client, connection) = mqtt::connect_to_mqtt_host(mqtt_host);
        let broker = mqtt::MqttBroker {
            client,
            broker: mqtt_host.to_string(),
            connected: false,
            topics: HashMap::new(),
            total_bytes: 0,
        };

        mqtt_lock.insert(mqtt_host.to_string(), broker);
        drop(mqtt_lock);

        loop_forever(connection, peer_map, mqtt_map);
    }
}

fn remove_broker(mqtt_host: &str, peer_map: &websocket::PeerMap, mqtt_map: &mqtt::BrokerMap) {
    let mut mqtt_lock = mqtt_map.lock().unwrap();
    let mqtt_client = mqtt_lock.iter().find(|entry| entry.0 == mqtt_host);

    if let Some((_key, broker)) = mqtt_client {
        println!("Removing MQTT-Client for {}", mqtt_host);

        if let Err(err) = broker.client.clone().disconnect() {
            println!("Error disconnecting MQTT client: {:?}", err);
        }

        mqtt_lock.remove(mqtt_host);
        drop(mqtt_lock);
        // TODO put this into websocket
        peer_map.lock().unwrap().iter().for_each(|(_addr, tx)| {
            let message = jsonrpc::JsonRpcNotification {
                jsonrpc: "2.0",
                method: "broker_removal",
                params: serde_json::json!(mqtt_host),
            };

            if let Ok(serialized) = serde_json::to_string(&message) {
                match tx.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                    Ok(_) => { /* Implement Logging */ }
                    Err(err) => println!("Error sending message: {:?}", err),
                }
            } else {
                println!("Failed to serialize brokers.");
            }
        });
    } else {
        println!("No MQTT-Client for {} found.", mqtt_host);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_channel::mpsc::unbounded;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Mutex;

    fn make_peer_map() -> websocket::PeerMap {
        websocket::PeerMap::new(Mutex::new(HashMap::new()))
    }

    fn make_mqtt_map() -> mqtt::BrokerMap {
        mqtt::BrokerMap::new(Mutex::new(HashMap::new()))
    }

    fn make_addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
    }

    fn insert_peer(
        peer_map: &websocket::PeerMap,
        port: u16,
    ) -> (
        SocketAddr,
        futures_channel::mpsc::UnboundedReceiver<warp::filters::ws::Message>,
    ) {
        let addr = make_addr(port);
        let (tx, rx) = unbounded();
        peer_map.lock().unwrap().insert(addr, tx);
        (addr, rx)
    }

    // --- deserialize_json_rpc_and_process ---

    #[test]
    fn test_process_invalid_json() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        // Should not panic
        deserialize_json_rpc_and_process("not json", &peer_map, &mqtt_map, "/tmp");
    }

    #[test]
    fn test_process_unknown_method() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let json = r#"{"jsonrpc":"2.0","method":"unknown_method","params":{}}"#;
        // Should not panic
        deserialize_json_rpc_and_process(json, &peer_map, &mqtt_map, "/tmp");
    }

    #[test]
    fn test_process_publish() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let json = r#"{"jsonrpc":"2.0","method":"publish","params":{"host":"nonexistent:1883","topic":"test","payload":"hello"}}"#;
        // Should not panic (broker doesn't exist, but it's handled gracefully)
        deserialize_json_rpc_and_process(json, &peer_map, &mqtt_map, "/tmp");
    }

    #[test]
    fn test_process_connect_spawns_thread() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!(
            "/tmp/mqtt_test_{}",
            uuid::Uuid::new_v4()
        );
        std::fs::create_dir_all(&config_path).ok();
        let json = r#"{"jsonrpc":"2.0","method":"connect","params":{"hostname":"127.0.0.1:19999"}}"#;

        deserialize_json_rpc_and_process(json, &peer_map, &mqtt_map, &config_path);

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
        let config_path = format!(
            "/tmp/mqtt_test_{}",
            uuid::Uuid::new_v4()
        );
        std::fs::create_dir_all(&config_path).ok();

        let json = r#"{"jsonrpc":"2.0","method":"connect","params":{"hostname":"127.0.0.1:19998"}}"#;
        deserialize_json_rpc_and_process(json, &peer_map, &mqtt_map, &config_path);
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Connect again — should not duplicate
        deserialize_json_rpc_and_process(json, &peer_map, &mqtt_map, &config_path);
        std::thread::sleep(std::time::Duration::from_millis(100));

        assert_eq!(mqtt_map.lock().unwrap().len(), 1);

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_process_remove_nonexistent_broker() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!(
            "/tmp/mqtt_test_{}",
            uuid::Uuid::new_v4()
        );
        std::fs::create_dir_all(&config_path).ok();

        let json = r#"{"jsonrpc":"2.0","method":"remove","params":{"hostname":"nonexistent:1883"}}"#;
        // Should not panic
        deserialize_json_rpc_and_process(json, &peer_map, &mqtt_map, &config_path);

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_process_save_and_remove_command() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!(
            "/tmp/mqtt_test_{}",
            uuid::Uuid::new_v4()
        );
        let commands_path = format!("{}/commands", config_path);
        std::fs::create_dir_all(&commands_path).ok();

        // Save a command
        let save_json = r#"{"jsonrpc":"2.0","method":"save_command","params":{"name":"test_cmd","topic":"t","payload":"p"}}"#;
        deserialize_json_rpc_and_process(save_json, &peer_map, &mqtt_map, &config_path);

        let cmd_file = format!("{}/test_cmd.json", commands_path);
        assert!(std::path::Path::new(&cmd_file).exists());

        // Remove the command
        let remove_json = r#"{"jsonrpc":"2.0","method":"remove_command","params":{"name":"test_cmd"}}"#;
        deserialize_json_rpc_and_process(remove_json, &peer_map, &mqtt_map, &config_path);

        assert!(!std::path::Path::new(&cmd_file).exists());

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_process_save_and_remove_pipeline() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let config_path = format!(
            "/tmp/mqtt_test_{}",
            uuid::Uuid::new_v4()
        );
        let pipelines_path = format!("{}/pipelines", config_path);
        std::fs::create_dir_all(&pipelines_path).ok();

        let save_json = r#"{"jsonrpc":"2.0","method":"save_pipeline","params":{"name":"test_pipe","pipeline":[{"topic":"t1"}]}}"#;
        deserialize_json_rpc_and_process(save_json, &peer_map, &mqtt_map, &config_path);

        let pipe_file = format!("{}/test_pipe.json", pipelines_path);
        assert!(std::path::Path::new(&pipe_file).exists());

        let remove_json = r#"{"jsonrpc":"2.0","method":"remove_pipeline","params":{"name":"test_pipe"}}"#;
        deserialize_json_rpc_and_process(remove_json, &peer_map, &mqtt_map, &config_path);

        assert!(!std::path::Path::new(&pipe_file).exists());

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_connect_broadcasts_brokers_to_peers() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);
        let config_path = format!(
            "/tmp/mqtt_test_{}",
            uuid::Uuid::new_v4()
        );
        std::fs::create_dir_all(&config_path).ok();

        let json = r#"{"jsonrpc":"2.0","method":"connect","params":{"hostname":"127.0.0.1:19997"}}"#;
        deserialize_json_rpc_and_process(json, &peer_map, &mqtt_map, &config_path);

        // Should have received a broadcast_brokers message
        let msg = rx.try_next().unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(msg.to_str().unwrap()).unwrap();
        assert_eq!(parsed["method"], "mqtt_brokers");

        std::fs::remove_dir_all(&config_path).ok();
    }

    #[test]
    fn test_remove_broker_sends_removal_notification() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);
        let config_path = format!(
            "/tmp/mqtt_test_{}",
            uuid::Uuid::new_v4()
        );
        std::fs::create_dir_all(&config_path).ok();

        // First connect
        let connect_json = r#"{"jsonrpc":"2.0","method":"connect","params":{"hostname":"127.0.0.1:19996"}}"#;
        deserialize_json_rpc_and_process(connect_json, &peer_map, &mqtt_map, &config_path);
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Drain the connect broadcast
        while rx.try_next().is_ok() {}

        // Now remove
        let remove_json = r#"{"jsonrpc":"2.0","method":"remove","params":{"hostname":"127.0.0.1:19996"}}"#;
        deserialize_json_rpc_and_process(remove_json, &peer_map, &mqtt_map, &config_path);

        // Should receive broker_removal and broadcast_brokers messages
        let mut methods = Vec::new();
        while let Ok(Some(msg)) = rx.try_next() {
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
        connect_to_known_brokers("/nonexistent/brokers.json", &peer_map, &mqtt_map);
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
        let config_path = format!(
            "/tmp/mqtt_test_{}",
            uuid::Uuid::new_v4()
        );
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
                    deserialize_json_rpc_and_process(&json, &pm, &mm, &cp);
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        std::fs::remove_dir_all(&config_path).ok();
    }
}
