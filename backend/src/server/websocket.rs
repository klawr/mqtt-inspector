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

use super::config::{CommandMessage, PipelineMessage};
use super::jsonrpc;
use super::mqtt;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use futures_channel::mpsc::UnboundedSender;

pub type PeerMap = Arc<Mutex<HashMap<SocketAddr, UnboundedSender<warp::filters::ws::Message>>>>;

pub fn send_message_to_peers(
    peer_map: &PeerMap,
    source: &str,
    topic: &str,
    payload: &bytes::Bytes,
) {
    let mut to_remove = Vec::new();
    {
        let peers = peer_map.lock().unwrap();
        for (addr, tx) in peers.iter() {
            let message = jsonrpc::JsonRpcNotification {
                jsonrpc: "2.0",
                method: "mqtt_message",
                params: serde_json::json!({
                    "source": source,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "topic": topic,
                    "payload": payload.to_vec(), // Convert Bytes to Vec<u8>
                }),
            };

            if let Ok(serialized) = serde_json::to_string(&message) {
                match tx.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                    Ok(_) => { /* Implement Logging */ }
                    Err(err) => {
                        if tx.is_closed() {
                            println!("Peer {} is closed. Removing from peer map.", addr);
                            to_remove.push(*addr);
                        }
                        println!("Error sending message to {}: {:?}", addr, err);
                    }
                }
            }
        }
    }

    if !to_remove.is_empty() {
        let mut peers = peer_map.lock().unwrap();
        for addr in to_remove {
            peers.remove(&addr);
        }
    }
}

pub fn send_broker_status_to_peers(peer_map: &PeerMap, source: &str, status: bool) {
    let mut to_remove = Vec::new();
    {
        let peers = peer_map.lock().unwrap();
        for (addr, tx) in peers.iter() {
            let message = jsonrpc::JsonRpcNotification {
                jsonrpc: "2.0",
                method: "mqtt_connection_status",
                params: serde_json::json!({
                    "source": source,
                    "connected": status,
                }),
            };

            if let Ok(serialized) = serde_json::to_string(&message) {
                match tx.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                    Ok(_) => { /* Implement Logging */ }
                    Err(err) => {
                        if tx.is_closed() {
                            println!("Peer {} is closed. Removing from peer map.", addr);
                            to_remove.push(*addr);
                        }
                        println!("Error sending message to {}: {:?}", addr, err);
                    }
                }
            }
        }
    }

    if !to_remove.is_empty() {
        let mut peers = peer_map.lock().unwrap();
        for addr in to_remove {
            peers.remove(&addr);
        }
    }
}

pub fn send_configs(sender: &UnboundedSender<warp::filters::ws::Message>, config_path: &str) {
    send_commands(sender, &format!("{}/commands", config_path));
    send_pipelines(sender, &format!("{}/pipelines", config_path));
}

pub fn send_commands(
    sender: &UnboundedSender<warp::filters::ws::Message>,
    commands_path: &String,
) {
    if let Ok(commands) = std::fs::read_dir(commands_path) {
        let commands: Vec<CommandMessage> = commands
            .filter_map(|dir_entry| {
                if let Ok(file) = dir_entry {
                    if let Ok(file_content) = std::fs::read_to_string(file.path()) {
                        return serde_json::from_str(&file_content).ok();
                    }
                }
                None
            })
            .collect();

        let jsonrpc = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "commands",
            params: serde_json::json!(&commands),
        };

        if let Ok(serialized) = serde_json::to_string(&jsonrpc) {
            match sender.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                Ok(_) => { /* Implement Logging */ }
                Err(err) => println!("Error sending message: {:?}", err),
            }
        } else {
            eprintln!("Failed to serialize commands jsonjpc")
        }
    } else {
        eprintln!("Failed to read commands file from {}", commands_path);
    }
}

pub fn send_pipelines(
    sender: &UnboundedSender<warp::filters::ws::Message>,
    pipelines_path: &String,
) {
    if let Ok(pipelines) = std::fs::read_dir(pipelines_path) {
        let pipelines: Vec<PipelineMessage> = pipelines
            .filter_map(|dir_entry| {
                if let Ok(file) = dir_entry {
                    if let Ok(file_content) = std::fs::read_to_string(file.path()) {
                        return serde_json::from_str(&file_content).ok();
                    }
                }
                None
            })
            .collect();

        let jsonrpc = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "pipelines",
            params: serde_json::json!(&pipelines),
        };

        if let Ok(serialized) = serde_json::to_string(&jsonrpc) {
            match sender.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                Ok(_) => { /* Implement Logging */ }
                Err(err) => println!("Error sending message: {:?}", err),
            }
        } else {
            eprintln!("Failed to serialize commands jsonjpc")
        }
    }
}

pub fn broadcast_brokers(peer_map: &PeerMap, mqtt_map: &mqtt::BrokerMap) {
    let serialized = {
        let binding = mqtt_map.lock().unwrap();
        let brokers: Vec<&mqtt::MqttBroker> = binding.iter().map(|broker| broker.1).collect();
        let message = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "mqtt_brokers",
            params: serde_json::json!(brokers),
        };
        serde_json::to_string(&message)
    };

    match serialized {
        Ok(serialized) => {
            let peers = peer_map.lock().unwrap();
            for (_addr, tx) in peers.iter() {
                match tx.unbounded_send(warp::filters::ws::Message::text(serialized.clone())) {
                    Ok(_) => { /* Implement Logging */ }
                    Err(err) => println!("Error sending message: {:?}", err),
                }
            }
        }
        Err(_) => println!("Failed to serialize brokers."),
    }
}

pub fn send_brokers(tx: &UnboundedSender<warp::filters::ws::Message>, mqtt_map: &mqtt::BrokerMap) {
    // First, send settings so the frontend knows the configured limits
    send_settings(tx);

    let binding = mqtt_map.lock().unwrap();

    // Phase 1: Send broker metadata (without full topic data) so UI renders immediately
    let broker_summaries: Vec<serde_json::Value> = binding
        .values()
        .map(|broker| {
            serde_json::json!({
                "broker": broker.broker,
                "connected": broker.connected,
                "topics": {},
                "total_bytes": broker.total_bytes,
            })
        })
        .collect();

    let meta_msg = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "mqtt_brokers",
        params: serde_json::json!(broker_summaries),
    };
    if let Ok(serialized) = serde_json::to_string(&meta_msg) {
        let _ = tx.unbounded_send(warp::filters::ws::Message::text(serialized));
    }

    // Phase 2: Stream individual messages newest-first per broker
    // Collect all (broker, topic, message) tuples and sort by timestamp descending
    let mut all_messages: Vec<(&str, &str, &mqtt::MqttMessage)> = Vec::new();
    for broker in binding.values() {
        for (topic, messages) in &broker.topics {
            for msg in messages {
                all_messages.push((&broker.broker, topic, msg));
            }
        }
    }
    all_messages.sort_by(|a, b| b.2.timestamp.cmp(&a.2.timestamp));

    for (source, topic, msg) in &all_messages {
        let notification = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "mqtt_message",
            params: serde_json::json!({
                "source": source,
                "timestamp": msg.timestamp,
                "topic": topic,
                "payload": msg.payload,
            }),
        };
        if let Ok(serialized) = serde_json::to_string(&notification) {
            if tx.unbounded_send(warp::filters::ws::Message::text(serialized)).is_err() {
                break; // peer disconnected
            }
        }
    }

    // Phase 3: Send a "sync_complete" so the frontend knows history is done
    let done = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "sync_complete",
        params: serde_json::json!({}),
    };
    if let Ok(serialized) = serde_json::to_string(&done) {
        let _ = tx.unbounded_send(warp::filters::ws::Message::text(serialized));
    }
}

pub fn send_settings(tx: &UnboundedSender<warp::filters::ws::Message>) {
    let message = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "settings",
        params: serde_json::json!({
            "max_broker_bytes": mqtt::max_broker_bytes(),
            "max_message_size": mqtt::max_message_size(),
        }),
    };
    if let Ok(serialized) = serde_json::to_string(&message) {
        let _ = tx.unbounded_send(warp::filters::ws::Message::text(serialized));
    }
}

pub fn broadcast_pipelines(peer_map: &PeerMap, config_path: &str) {
    peer_map
        .lock()
        .unwrap()
        .iter()
        .for_each(|(_addr, tx)| send_pipelines(tx, &format!("{}/pipelines", config_path)));
}

pub fn broadcast_commands(peer_map: &PeerMap, config_path: &str) {
    peer_map
        .lock()
        .unwrap()
        .iter()
        .for_each(|(_addr, tx)| send_commands(tx, &format!("{}/commands", config_path)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_channel::mpsc::unbounded;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn make_addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
    }

    fn make_peer_map() -> PeerMap {
        PeerMap::new(Mutex::new(HashMap::new()))
    }

    fn make_mqtt_map() -> mqtt::BrokerMap {
        mqtt::BrokerMap::new(Mutex::new(HashMap::new()))
    }

    fn insert_peer(
        peer_map: &PeerMap,
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

    // --- send_message_to_peers ---

    #[test]
    fn test_send_message_to_peers_with_no_peers() {
        let peer_map = make_peer_map();
        let payload = bytes::Bytes::from("hello");
        // Should not panic with an empty peer map
        send_message_to_peers(&peer_map, "broker:1883", "test/topic", &payload);
        assert_eq!(peer_map.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_send_message_to_peers_delivers_to_single_peer() {
        let peer_map = make_peer_map();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);
        let payload = bytes::Bytes::from("hello");

        send_message_to_peers(&peer_map, "broker:1883", "test/topic", &payload);

        let msg = rx.try_next().unwrap().unwrap();
        let text = msg.to_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["method"], "mqtt_message");
        assert_eq!(parsed["params"]["source"], "broker:1883");
        assert_eq!(parsed["params"]["topic"], "test/topic");
    }

    #[test]
    fn test_send_message_to_peers_delivers_to_multiple_peers() {
        let peer_map = make_peer_map();
        let (_addr1, mut rx1) = insert_peer(&peer_map, 9001);
        let (_addr2, mut rx2) = insert_peer(&peer_map, 9002);
        let (_addr3, mut rx3) = insert_peer(&peer_map, 9003);

        let payload = bytes::Bytes::from("data");
        send_message_to_peers(&peer_map, "host:1883", "topic", &payload);

        for rx in [&mut rx1, &mut rx2, &mut rx3] {
            let msg = rx.try_next().unwrap().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(msg.to_str().unwrap()).unwrap();
            assert_eq!(parsed["method"], "mqtt_message");
        }
    }

    #[test]
    fn test_send_message_to_peers_removes_closed_peers() {
        let peer_map = make_peer_map();
        let (addr_closed, rx_closed) = insert_peer(&peer_map, 9001);
        let (_addr_open, mut rx_open) = insert_peer(&peer_map, 9002);

        // Drop the receiver to close the channel
        drop(rx_closed);

        let payload = bytes::Bytes::from("test");
        send_message_to_peers(&peer_map, "broker:1883", "topic", &payload);

        // Closed peer should be removed
        let peers = peer_map.lock().unwrap();
        assert!(!peers.contains_key(&addr_closed));
        assert_eq!(peers.len(), 1);
        drop(peers);

        // Open peer should still get the message
        let msg = rx_open.try_next().unwrap().unwrap();
        assert!(msg.to_str().unwrap().contains("mqtt_message"));
    }

    #[test]
    fn test_send_message_to_peers_payload_content() {
        let peer_map = make_peer_map();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);
        let payload = bytes::Bytes::from(vec![0x48, 0x65, 0x6c, 0x6c, 0x6f]); // "Hello"

        send_message_to_peers(&peer_map, "src", "t", &payload);

        let msg = rx.try_next().unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(msg.to_str().unwrap()).unwrap();
        let payload_arr = parsed["params"]["payload"].as_array().unwrap();
        assert_eq!(payload_arr, &[0x48, 0x65, 0x6c, 0x6c, 0x6f]);
    }

    // --- send_broker_status_to_peers ---

    #[test]
    fn test_send_broker_status_to_peers_connected() {
        let peer_map = make_peer_map();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);

        send_broker_status_to_peers(&peer_map, "broker:1883", true);

        let msg = rx.try_next().unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(msg.to_str().unwrap()).unwrap();
        assert_eq!(parsed["method"], "mqtt_connection_status");
        assert_eq!(parsed["params"]["source"], "broker:1883");
        assert_eq!(parsed["params"]["connected"], true);
    }

    #[test]
    fn test_send_broker_status_to_peers_disconnected() {
        let peer_map = make_peer_map();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);

        send_broker_status_to_peers(&peer_map, "broker:1883", false);

        let msg = rx.try_next().unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(msg.to_str().unwrap()).unwrap();
        assert_eq!(parsed["params"]["connected"], false);
    }

    #[test]
    fn test_send_broker_status_removes_closed_peers() {
        let peer_map = make_peer_map();
        let (addr, rx) = insert_peer(&peer_map, 9001);
        drop(rx);

        send_broker_status_to_peers(&peer_map, "broker:1883", true);

        assert!(!peer_map.lock().unwrap().contains_key(&addr));
    }

    #[test]
    fn test_send_broker_status_empty_peer_map() {
        let peer_map = make_peer_map();
        // Should not panic
        send_broker_status_to_peers(&peer_map, "broker:1883", true);
    }

    // --- broadcast_brokers ---

    #[test]
    fn test_broadcast_brokers_empty_maps() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        // Should not panic
        broadcast_brokers(&peer_map, &mqtt_map);
    }

    #[test]
    fn test_broadcast_brokers_sends_to_all_peers() {
        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();
        let (_addr1, mut rx1) = insert_peer(&peer_map, 9001);
        let (_addr2, mut rx2) = insert_peer(&peer_map, 9002);

        broadcast_brokers(&peer_map, &mqtt_map);

        for rx in [&mut rx1, &mut rx2] {
            let msg = rx.try_next().unwrap().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(msg.to_str().unwrap()).unwrap();
            assert_eq!(parsed["method"], "mqtt_brokers");
        }
    }

    // --- send_brokers ---

    #[test]
    fn test_send_brokers_empty_mqtt_map() {
        let mqtt_map = make_mqtt_map();
        let (tx, mut rx) = unbounded();

        send_brokers(&tx, &mqtt_map);

        // First message is now "settings"
        let settings_msg = rx.try_next().unwrap().unwrap();
        let settings: serde_json::Value =
            serde_json::from_str(settings_msg.to_str().unwrap()).unwrap();
        assert_eq!(settings["method"], "settings");
        assert!(settings["params"]["max_broker_bytes"].is_number());
        assert!(settings["params"]["max_message_size"].is_number());

        // Second message is "mqtt_brokers" with empty list
        let brokers_msg = rx.try_next().unwrap().unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(brokers_msg.to_str().unwrap()).unwrap();
        assert_eq!(parsed["method"], "mqtt_brokers");
        assert_eq!(parsed["params"], serde_json::json!([]));

        // Third message is "sync_complete"
        let sync_msg = rx.try_next().unwrap().unwrap();
        let sync: serde_json::Value = serde_json::from_str(sync_msg.to_str().unwrap()).unwrap();
        assert_eq!(sync["method"], "sync_complete");
    }

    // --- broadcast_pipelines / broadcast_commands ---

    #[test]
    fn test_broadcast_pipelines_empty_peer_map() {
        let peer_map = make_peer_map();
        // Should not panic with no peers and nonexistent path
        broadcast_pipelines(&peer_map, "/nonexistent/path");
    }

    #[test]
    fn test_broadcast_commands_empty_peer_map() {
        let peer_map = make_peer_map();
        broadcast_commands(&peer_map, "/nonexistent/path");
    }

    #[test]
    fn test_send_configs_sends_commands_and_pipelines() {
        let (tx, mut rx) = unbounded();
        // Use the test config with real command/pipeline files
        send_configs(&tx, "../test/config_source");

        // Should receive at least 2 messages (commands + pipelines)
        let msg1 = rx.try_next().unwrap().unwrap();
        let msg2 = rx.try_next().unwrap().unwrap();
        let parsed1: serde_json::Value = serde_json::from_str(msg1.to_str().unwrap()).unwrap();
        let parsed2: serde_json::Value = serde_json::from_str(msg2.to_str().unwrap()).unwrap();

        let methods: Vec<&str> = vec![
            parsed1["method"].as_str().unwrap(),
            parsed2["method"].as_str().unwrap(),
        ];
        assert!(methods.contains(&"commands"));
        assert!(methods.contains(&"pipelines"));
    }

    // --- Concurrency tests: verify no deadlock ---

    #[test]
    fn test_concurrent_send_message_and_status_no_deadlock() {
        use std::sync::Arc;
        use std::thread;

        let peer_map = make_peer_map();
        // Insert several peers
        let mut receivers = Vec::new();
        for port in 9001..9011 {
            let (_addr, rx) = insert_peer(&peer_map, port);
            receivers.push(rx);
        }

        let peer_map_clone = Arc::clone(&peer_map);
        let handle1 = thread::spawn(move || {
            for _ in 0..100 {
                let payload = bytes::Bytes::from("test");
                send_message_to_peers(&peer_map_clone, "broker:1883", "topic", &payload);
            }
        });

        let peer_map_clone2 = Arc::clone(&peer_map);
        let handle2 = thread::spawn(move || {
            for _ in 0..100 {
                send_broker_status_to_peers(&peer_map_clone2, "broker:1883", true);
            }
        });

        // If there's a deadlock, these joins will hang (test will time out)
        handle1.join().unwrap();
        handle2.join().unwrap();
    }

    #[test]
    fn test_concurrent_broadcast_brokers_and_send_messages_no_deadlock() {
        use std::sync::Arc;
        use std::thread;

        let peer_map = make_peer_map();
        let mqtt_map = make_mqtt_map();

        let mut receivers = Vec::new();
        for port in 9001..9006 {
            let (_addr, rx) = insert_peer(&peer_map, port);
            receivers.push(rx);
        }

        let pm1 = Arc::clone(&peer_map);
        let mm1 = Arc::clone(&mqtt_map);
        let handle1 = thread::spawn(move || {
            for _ in 0..100 {
                broadcast_brokers(&pm1, &mm1);
            }
        });

        let pm2 = Arc::clone(&peer_map);
        let handle2 = thread::spawn(move || {
            for _ in 0..100 {
                let payload = bytes::Bytes::from("data");
                send_message_to_peers(&pm2, "broker:1883", "t", &payload);
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();
    }

    #[test]
    fn test_concurrent_insert_remove_peers_no_deadlock() {
        use std::sync::Arc;
        use std::thread;

        let peer_map = make_peer_map();

        let pm1 = Arc::clone(&peer_map);
        let handle1 = thread::spawn(move || {
            for port in 10000..10050 {
                let addr = make_addr(port);
                let (tx, _rx) = unbounded();
                pm1.lock().unwrap().insert(addr, tx);
            }
        });

        let pm2 = Arc::clone(&peer_map);
        let handle2 = thread::spawn(move || {
            for _ in 0..50 {
                send_broker_status_to_peers(&pm2, "broker:1883", true);
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();
    }
}
