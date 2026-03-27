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

use futures_channel::mpsc::Sender;

/// Per-peer outbound channel capacity. Messages are dropped (not buffered
/// indefinitely) when a slow WebSocket client falls this far behind.
pub const PEER_CHANNEL_CAPACITY: usize = 1024;

/// Disconnect peers that keep their outbound queue full for this many
/// consecutive send attempts. This protects the backend from spending the rest
/// of the run faning out to a peer that no longer keeps up.
const PEER_MAX_CONSECUTIVE_FULL_SENDS: usize = 128;

// ─── Notification batching ───────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
pub struct PendingMeta {
    pub source: String,
    pub topic: String,
    pub timestamp: String,
    pub payload_size: usize,
    pub total_bytes: usize,
    pub topic_message_count: usize,
}

#[derive(Clone, serde::Serialize)]
pub struct PendingEviction {
    pub source: String,
    pub topic: String,
    pub count: usize,
    pub topic_message_count: usize,
}

#[derive(Default)]
pub struct NotificationBuffer {
    pub metas: Vec<PendingMeta>,
    pub evictions: Vec<PendingEviction>,
}

pub type NotificationBuf = Arc<Mutex<NotificationBuffer>>;

/// Drain the notification buffer and send batched messages to all connected
/// peers. Called periodically by a dedicated timer thread (e.g. every 100 ms).
pub fn flush_notification_buffer(buf: &NotificationBuf, peer_map: &PeerMap) {
    let (metas, evictions) = {
        let mut lock = buf.lock().unwrap();
        if lock.metas.is_empty() && lock.evictions.is_empty() {
            return;
        }
        (
            std::mem::take(&mut lock.metas),
            std::mem::take(&mut lock.evictions),
        )
    };

    if !evictions.is_empty() {
        let message = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "messages_evicted_batch",
            params: serde_json::json!(evictions),
        };
        if let Ok(serialized) = serde_json::to_string(&message) {
            send_serialized_to_peers(peer_map, &serialized, "messages_evicted_batch");
        }
    }

    if !metas.is_empty() {
        let message = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "mqtt_message_meta_batch",
            params: serde_json::json!(metas),
        };
        if let Ok(serialized) = serde_json::to_string(&message) {
            send_serialized_to_peers(peer_map, &serialized, "mqtt_message_meta_batch");
        }
    }
}

// ─── Notification buffer helpers (called from broker loops) ──────────

pub fn buffer_message_meta(
    buf: &NotificationBuf,
    source: &str,
    topic: &str,
    timestamp: &str,
    payload_size: usize,
    total_bytes: usize,
    topic_message_count: usize,
) {
    buf.lock().unwrap().metas.push(PendingMeta {
        source: source.to_string(),
        topic: topic.to_string(),
        timestamp: timestamp.to_string(),
        payload_size,
        total_bytes,
        topic_message_count,
    });
}

pub fn buffer_evictions(buf: &NotificationBuf, source: &str, evictions: &[(String, usize, usize)]) {
    let mut lock = buf.lock().unwrap();
    for (topic, count, topic_message_count) in evictions {
        lock.evictions.push(PendingEviction {
            source: source.to_string(),
            topic: topic.clone(),
            count: *count,
            topic_message_count: *topic_message_count,
        });
    }
}

pub struct PeerConnection {
    pub tx: Sender<warp::filters::ws::Message>,
    consecutive_full: usize,
    dropped_messages: usize,
    pub selected_broker: Option<String>,
    pub selected_topic: Option<String>,
}

impl PeerConnection {
    pub(crate) fn new(tx: Sender<warp::filters::ws::Message>) -> Self {
        Self {
            tx,
            consecutive_full: 0,
            dropped_messages: 0,
            selected_broker: None,
            selected_topic: None,
        }
    }

    fn mark_success(&mut self) {
        self.consecutive_full = 0;
    }

    fn mark_full(&mut self) -> bool {
        self.dropped_messages += 1;
        self.consecutive_full += 1;
        self.consecutive_full >= PEER_MAX_CONSECUTIVE_FULL_SENDS
    }
}

pub type PeerMap = Arc<Mutex<HashMap<SocketAddr, PeerConnection>>>;

fn send_message_to_peer_map(
    peer_map: &PeerMap,
    message_kind: &str,
    build_message: impl Fn() -> warp::filters::ws::Message,
) {
    let mut to_remove = Vec::new();
    let mut peers = peer_map.lock().unwrap();
    for (addr, peer) in peers.iter_mut() {
        match peer.tx.try_send(build_message()) {
            Ok(_) => {
                peer.mark_success();
            }
            Err(err) => {
                if err.is_disconnected() {
                    println!("Peer {addr} is closed while sending {message_kind}. Removing from peer map.");
                    to_remove.push(*addr);
                } else if peer.mark_full() {
                    println!(
                        "Peer {addr} fell behind on {message_kind} and dropped {} messages. Disconnecting slow peer.",
                        peer.dropped_messages
                    );
                    to_remove.push(*addr);
                }
            }
        }
    }
    for addr in to_remove {
        peers.remove(&addr);
    }
}

fn send_serialized_to_peers(peer_map: &PeerMap, serialized: &str, message_kind: &str) {
    let msg = warp::filters::ws::Message::text(serialized);
    send_message_to_peer_map(peer_map, message_kind, || msg.clone())
}

fn build_binary_mqtt_frame(
    source: &str,
    topic: &str,
    timestamp: &str,
    payload: &[u8],
    total_bytes: Option<usize>,
) -> Option<Vec<u8>> {
    let header = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "mqtt_message",
        params: serde_json::json!({
            "source": source,
            "timestamp": timestamp,
            "topic": topic,
            "total_bytes": total_bytes,
        }),
    };
    let header_bytes = serde_json::to_vec(&header).ok()?;
    let header_len: u32 = header_bytes.len().try_into().ok()?;

    let mut frame = Vec::with_capacity(4 + header_bytes.len() + payload.len());
    frame.extend_from_slice(&header_len.to_be_bytes());
    frame.extend_from_slice(&header_bytes);
    frame.extend_from_slice(payload);
    Some(frame)
}

/// Send full message payload ONLY to peers that have selected this broker+topic.
pub fn send_message_to_subscribed_peers(
    peer_map: &PeerMap,
    source: &str,
    topic: &str,
    payload: &bytes::Bytes,
    total_bytes: usize,
    timestamp: &str,
) {
    let binary_frame = match build_binary_mqtt_frame(
        source,
        topic,
        timestamp,
        payload.as_ref(),
        Some(total_bytes),
    ) {
        Some(frame) => frame,
        None => return,
    };

    let mut to_remove = Vec::new();
    let mut peers = peer_map.lock().unwrap();
    for (addr, peer) in peers.iter_mut() {
        let watching = peer.selected_broker.as_deref() == Some(source)
            && peer.selected_topic.as_deref() == Some(topic);
        if !watching {
            continue;
        }
        match peer
            .tx
            .try_send(warp::filters::ws::Message::binary(binary_frame.clone()))
        {
            Ok(_) => {
                peer.mark_success();
            }
            Err(err) => {
                if err.is_disconnected() || peer.mark_full() {
                    to_remove.push(*addr);
                }
            }
        }
    }
    for addr in to_remove {
        peers.remove(&addr);
    }
}

/// Handle a `select_topic` request from a specific peer.
/// Updates the peer's selected broker+topic and sends existing messages for that topic.
pub fn handle_select_topic(
    peer_map: &PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    addr: SocketAddr,
    broker: Option<&str>,
    topic: Option<&str>,
) {
    // Phase 1: Collect messages from the mqtt_map (if a topic is selected)
    let messages: Vec<(String, bytes::Bytes)> =
        if let (Some(broker_name), Some(topic_name)) = (broker, topic) {
            let mqtt_lock = mqtt_map.lock().unwrap();
            if let Some(broker_data) = mqtt_lock.get(broker_name) {
                if let Some(topic_msgs) = broker_data.topics.get(topic_name) {
                    topic_msgs
                        .iter()
                        .rev() // newest first
                        .map(|msg| (msg.timestamp.clone(), msg.payload.clone()))
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

    // Phase 2: Update the peer's selection and get a sender clone
    let sender = {
        let mut peers = peer_map.lock().unwrap();
        if let Some(peer) = peers.get_mut(&addr) {
            peer.selected_broker = broker.map(|s| s.to_string());
            peer.selected_topic = topic.map(|s| s.to_string());
            Some(peer.tx.clone())
        } else {
            None
        }
    };

    let Some(mut tx) = sender else {
        return;
    };

    // Phase 3: Send topic_messages_clear
    let clear_msg = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "topic_messages_clear",
        params: serde_json::json!({}),
    };
    if let Ok(serialized) = serde_json::to_string(&clear_msg) {
        let _ = tx.try_send(warp::filters::ws::Message::text(serialized));
    }

    // Phase 4: Stream existing messages for the topic (newest first)
    if let (Some(source), Some(topic_name)) = (broker, topic) {
        for (timestamp, payload) in &messages {
            if let Some(frame) =
                build_binary_mqtt_frame(source, topic_name, timestamp, payload, None)
            {
                if tx
                    .try_send(warp::filters::ws::Message::binary(frame))
                    .is_err()
                {
                    break;
                }
            }
        }
    }

    // Phase 5: Send topic_sync_complete
    let done = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "topic_sync_complete",
        params: serde_json::json!({}),
    };
    if let Ok(serialized) = serde_json::to_string(&done) {
        let _ = tx.try_send(warp::filters::ws::Message::text(serialized));
    }
}

pub fn send_rate_sample_to_peers(
    peer_map: &PeerMap,
    source: &str,
    sample: &mqtt::RateHistoryEntry,
) {
    let message = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "rate_history_sample",
        params: serde_json::json!({
            "source": source,
            "sample": sample,
        }),
    };

    let serialized = match serde_json::to_string(&message) {
        Ok(s) => s,
        Err(_) => return,
    };

    send_serialized_to_peers(peer_map, &serialized, "rate_history_sample");
}

pub fn send_broker_status_to_peers(peer_map: &PeerMap, source: &str, status: bool) {
    // Serialize once outside the lock
    let message = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "mqtt_connection_status",
        params: serde_json::json!({
            "source": source,
            "connected": status,
        }),
    };

    let serialized = match serde_json::to_string(&message) {
        Ok(s) => s,
        Err(_) => return,
    };

    send_serialized_to_peers(peer_map, &serialized, "mqtt_connection_status");
}

pub fn send_configs(sender: &mut Sender<warp::filters::ws::Message>, config_path: &str) {
    send_commands(sender, &format!("{config_path}/commands"));
    send_pipelines(sender, &format!("{config_path}/pipelines"));
}

pub fn send_commands(sender: &mut Sender<warp::filters::ws::Message>, commands_path: &str) {
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
            match sender.try_send(warp::filters::ws::Message::text(serialized)) {
                Ok(_) => { /* Implement Logging */ }
                Err(err) => println!("Error sending message: {err:?}"),
            }
        } else {
            eprintln!("Failed to serialize commands jsonjpc")
        }
    } else {
        eprintln!("Failed to read commands file from {commands_path}");
    }
}

pub fn send_pipelines(sender: &mut Sender<warp::filters::ws::Message>, pipelines_path: &str) {
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
            match sender.try_send(warp::filters::ws::Message::text(serialized)) {
                Ok(_) => { /* Implement Logging */ }
                Err(err) => println!("Error sending message: {err:?}"),
            }
        } else {
            eprintln!("Failed to serialize commands jsonjpc")
        }
    }
}

pub fn broadcast_brokers(peer_map: &PeerMap, mqtt_map: &mqtt::BrokerMap) {
    // Only send broker metadata, not the full message store
    let serialized = {
        let binding = mqtt_map.lock().unwrap();
        let summaries: Vec<serde_json::Value> = binding
            .values()
            .map(|broker| {
                serde_json::json!({
                    "broker": broker.broker,
                    "connected": broker.connected,
                    "topics": {},
                    "total_bytes": broker.total_bytes,
                    "rate_history": broker.rate_history,
                })
            })
            .collect();
        let message = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "mqtt_brokers",
            params: serde_json::json!(summaries),
        };
        serde_json::to_string(&message)
    };

    match serialized {
        Ok(serialized) => {
            let mut peers = peer_map.lock().unwrap();
            for (_addr, peer) in peers.iter_mut() {
                let _ = peer
                    .tx
                    .try_send(warp::filters::ws::Message::text(serialized.clone()));
            }
        }
        Err(_) => println!("Failed to serialize brokers."),
    }
}

pub fn send_brokers(tx: &mut Sender<warp::filters::ws::Message>, mqtt_map: &mqtt::BrokerMap) {
    // First, send settings so the frontend knows the configured limits
    send_settings(tx);

    // Clone data out of the lock so we don't hold mqtt_map during serialization.
    // Under heavy load, holding the lock here blocks the MQTT receive loop long
    // enough to trip the broker's keep-alive timeout.
    let (broker_summaries, topic_summaries) = {
        let binding = mqtt_map.lock().unwrap();

        let summaries: Vec<serde_json::Value> = binding
            .values()
            .map(|broker| {
                serde_json::json!({
                    "broker": broker.broker,
                    "connected": broker.connected,
                    "topics": {},
                    "total_bytes": broker.total_bytes,
                    "rate_history": broker.rate_history,
                })
            })
            .collect();

        // Collect topic summaries: (broker_name, topic_name, count, latest_timestamp)
        let mut topic_sums: Vec<serde_json::Value> = Vec::new();
        for broker in binding.values() {
            let mut topics_map = serde_json::Map::new();
            for (topic, messages) in &broker.topics {
                let latest_ts = messages.back().map(|m| m.timestamp.as_str()).unwrap_or("");
                topics_map.insert(
                    topic.clone(),
                    serde_json::json!({
                        "count": messages.len(),
                        "latest_timestamp": latest_ts,
                    }),
                );
            }
            topic_sums.push(serde_json::json!({
                "source": broker.broker,
                "topics": topics_map,
            }));
        }

        (summaries, topic_sums)
    }; // mqtt_map lock released here

    // Phase 1: Send broker metadata (without full topic data) so UI renders immediately
    let meta_msg = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "mqtt_brokers",
        params: serde_json::json!(broker_summaries),
    };
    if let Ok(serialized) = serde_json::to_string(&meta_msg) {
        let _ = tx.try_send(warp::filters::ws::Message::text(serialized));
    }

    // Phase 2: Send topic summaries (name + count) instead of full messages
    for summary in &topic_summaries {
        let msg = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "topic_summaries",
            params: summary.clone(),
        };
        if let Ok(serialized) = serde_json::to_string(&msg) {
            if tx
                .try_send(warp::filters::ws::Message::text(serialized))
                .is_err()
            {
                break;
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
        let _ = tx.try_send(warp::filters::ws::Message::text(serialized));
    }
}

pub fn send_settings(tx: &mut Sender<warp::filters::ws::Message>) {
    let message = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "settings",
        params: serde_json::json!({
            "max_broker_bytes": mqtt::max_broker_bytes(),
            "max_message_size": mqtt::max_message_size(),
        }),
    };
    if let Ok(serialized) = serde_json::to_string(&message) {
        let _ = tx.try_send(warp::filters::ws::Message::text(serialized));
    }
}

pub fn broadcast_pipelines(peer_map: &PeerMap, config_path: &str) {
    peer_map
        .lock()
        .unwrap()
        .iter_mut()
        .for_each(|(_addr, peer)| {
            send_pipelines(&mut peer.tx, &format!("{config_path}/pipelines"))
        });
}

pub fn broadcast_commands(peer_map: &PeerMap, config_path: &str) {
    peer_map
        .lock()
        .unwrap()
        .iter_mut()
        .for_each(|(_addr, peer)| send_commands(&mut peer.tx, &format!("{config_path}/commands")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_channel::mpsc::channel;
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
        futures_channel::mpsc::Receiver<warp::filters::ws::Message>,
    ) {
        let addr = make_addr(port);
        let (tx, rx) = channel(PEER_CHANNEL_CAPACITY);
        peer_map
            .lock()
            .unwrap()
            .insert(addr, PeerConnection::new(tx));
        (addr, rx)
    }

    // --- send_message_to_subscribed_peers ---

    #[test]
    fn test_send_message_to_subscribed_peers_only_delivers_to_watchers() {
        let peer_map = make_peer_map();
        let (addr1, mut rx1) = insert_peer(&peer_map, 9001);
        let (_addr2, mut rx2) = insert_peer(&peer_map, 9002);

        // Set peer 1 to watch test/topic on broker:1883
        {
            let mut peers = peer_map.lock().unwrap();
            let peer = peers.get_mut(&addr1).unwrap();
            peer.selected_broker = Some("broker:1883".to_string());
            peer.selected_topic = Some("test/topic".to_string());
        }

        let payload = bytes::Bytes::from("hello");
        send_message_to_subscribed_peers(
            &peer_map,
            "broker:1883",
            "test/topic",
            &payload,
            100,
            "2024-01-01T00:00:00Z",
        );

        // Peer 1 should receive (watching the topic)
        let msg = rx1.try_recv().unwrap();
        assert!(msg.is_binary());
        let bytes = msg.as_bytes();
        let header_len = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
        let parsed: serde_json::Value = serde_json::from_slice(&bytes[4..4 + header_len]).unwrap();
        assert_eq!(parsed["method"], "mqtt_message");
        assert_eq!(&bytes[4 + header_len..], b"hello");

        // Peer 2 should NOT receive (not watching)
        assert!(rx2.try_recv().is_err());
    }

    #[test]
    fn test_send_message_to_subscribed_peers_no_watchers() {
        let peer_map = make_peer_map();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);

        let payload = bytes::Bytes::from("hello");
        send_message_to_subscribed_peers(
            &peer_map,
            "broker:1883",
            "test/topic",
            &payload,
            100,
            "2024-01-01T00:00:00Z",
        );

        // No peer is watching, should not receive anything
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_send_message_to_subscribed_peers_removes_closed_peers() {
        let peer_map = make_peer_map();
        let (addr_closed, rx_closed) = insert_peer(&peer_map, 9001);
        let (_addr_open, mut rx_open) = insert_peer(&peer_map, 9002);

        // Both watch the same topic, but close peer 1's receiver
        {
            let mut peers = peer_map.lock().unwrap();
            for (_, peer) in peers.iter_mut() {
                peer.selected_broker = Some("broker:1883".to_string());
                peer.selected_topic = Some("topic".to_string());
            }
        }
        drop(rx_closed);

        let payload = bytes::Bytes::from("test");
        send_message_to_subscribed_peers(
            &peer_map,
            "broker:1883",
            "topic",
            &payload,
            0,
            "2024-01-01T00:00:00Z",
        );

        // Closed peer should be removed
        let peers = peer_map.lock().unwrap();
        assert!(!peers.contains_key(&addr_closed));
        assert_eq!(peers.len(), 1);
        drop(peers);

        // Open peer should still get the message
        let msg = rx_open.try_recv().unwrap();
        assert!(msg.is_binary());
    }

    #[test]
    fn test_flush_removes_persistently_full_peers() {
        let peer_map = make_peer_map();
        let buf = make_notification_buf();
        let addr_full = make_addr(9001);
        let (tx_full, _rx_full) = channel(PEER_CHANNEL_CAPACITY);
        peer_map
            .lock()
            .unwrap()
            .insert(addr_full, PeerConnection::new(tx_full));

        let mut removed = false;

        for _ in 0..(PEER_CHANNEL_CAPACITY + PEER_MAX_CONSECUTIVE_FULL_SENDS * 4) {
            buffer_message_meta(
                &buf,
                "broker:1883",
                "topic",
                "2024-01-01T00:00:00Z",
                1,
                0,
                1,
            );
            flush_notification_buffer(&buf, &peer_map);

            if !peer_map.lock().unwrap().contains_key(&addr_full) {
                removed = true;
                break;
            }
        }

        assert!(removed);
        assert!(peer_map.lock().unwrap().is_empty());
    }

    // --- send_broker_status_to_peers ---

    #[test]
    fn test_send_broker_status_to_peers_connected() {
        let peer_map = make_peer_map();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);

        send_broker_status_to_peers(&peer_map, "broker:1883", true);

        let msg = rx.try_recv().unwrap();
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

        let msg = rx.try_recv().unwrap();
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
            let msg = rx.try_recv().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(msg.to_str().unwrap()).unwrap();
            assert_eq!(parsed["method"], "mqtt_brokers");
        }
    }

    // --- send_brokers ---

    #[test]
    fn test_send_brokers_empty_mqtt_map() {
        let mqtt_map = make_mqtt_map();
        let (mut tx, mut rx) = channel(PEER_CHANNEL_CAPACITY);

        send_brokers(&mut tx, &mqtt_map);

        // First message is now "settings"
        let settings_msg = rx.try_recv().unwrap();
        let settings: serde_json::Value =
            serde_json::from_str(settings_msg.to_str().unwrap()).unwrap();
        assert_eq!(settings["method"], "settings");
        assert!(settings["params"]["max_broker_bytes"].is_number());
        assert!(settings["params"]["max_message_size"].is_number());

        // Third message is "mqtt_brokers" with empty list
        let brokers_msg = rx.try_recv().unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(brokers_msg.to_str().unwrap()).unwrap();
        assert_eq!(parsed["method"], "mqtt_brokers");
        assert_eq!(parsed["params"], serde_json::json!([]));

        // Fourth message is "sync_complete"
        let sync_msg = rx.try_recv().unwrap();
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
        let (mut tx, mut rx) = channel(PEER_CHANNEL_CAPACITY);
        // Use the test config with real command/pipeline files
        send_configs(&mut tx, "../test/config_source");

        // Should receive at least 2 messages (commands + pipelines)
        let msg1 = rx.try_recv().unwrap();
        let msg2 = rx.try_recv().unwrap();
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
        let buf = make_notification_buf();
        // Insert several peers
        let mut receivers = Vec::new();
        for port in 9001..9011 {
            let (_addr, rx) = insert_peer(&peer_map, port);
            receivers.push(rx);
        }

        let pm1 = Arc::clone(&peer_map);
        let buf1 = Arc::clone(&buf);
        let handle1 = thread::spawn(move || {
            for _ in 0..100 {
                buffer_message_meta(
                    &buf1,
                    "broker:1883",
                    "topic",
                    "2024-01-01T00:00:00Z",
                    4,
                    0,
                    1,
                );
                flush_notification_buffer(&buf1, &pm1);
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
        let buf = make_notification_buf();

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
        let buf2 = Arc::clone(&buf);
        let handle2 = thread::spawn(move || {
            for _ in 0..100 {
                buffer_message_meta(&buf2, "broker:1883", "t", "2024-01-01T00:00:00Z", 4, 0, 1);
                flush_notification_buffer(&buf2, &pm2);
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
                let (tx, _rx) = channel(PEER_CHANNEL_CAPACITY);
                pm1.lock().unwrap().insert(addr, PeerConnection::new(tx));
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

    // --- flush_notification_buffer ---

    fn make_notification_buf() -> NotificationBuf {
        NotificationBuf::new(Mutex::new(NotificationBuffer::default()))
    }

    #[test]
    fn test_flush_empty_buffer_is_noop() {
        let peer_map = make_peer_map();
        let buf = make_notification_buf();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);
        flush_notification_buffer(&buf, &peer_map);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_flush_sends_batched_metas() {
        let peer_map = make_peer_map();
        let buf = make_notification_buf();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);

        buffer_message_meta(&buf, "broker:1883", "t/1", "ts1", 5, 100, 1);
        buffer_message_meta(&buf, "broker:1883", "t/2", "ts2", 10, 200, 2);

        flush_notification_buffer(&buf, &peer_map);

        let msg = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(msg.to_str().unwrap()).unwrap();
        assert_eq!(parsed["method"], "mqtt_message_meta_batch");
        let params = parsed["params"].as_array().unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0]["topic"], "t/1");
        assert_eq!(params[1]["topic"], "t/2");

        // Buffer should be empty after flush
        assert!(buf.lock().unwrap().metas.is_empty());
    }

    #[test]
    fn test_flush_sends_batched_evictions() {
        let peer_map = make_peer_map();
        let buf = make_notification_buf();
        let (_addr, mut rx) = insert_peer(&peer_map, 9001);

        buffer_evictions(
            &buf,
            "broker:1883",
            &[("t/1".to_string(), 3, 10), ("t/2".to_string(), 1, 5)],
        );

        flush_notification_buffer(&buf, &peer_map);

        let msg = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(msg.to_str().unwrap()).unwrap();
        assert_eq!(parsed["method"], "messages_evicted_batch");
        let params = parsed["params"].as_array().unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0]["count"], 3);
        assert_eq!(params[1]["count"], 1);
    }
}
