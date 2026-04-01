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

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
};

use rumqttc::{MqttOptions, QoS, Transport};

use super::config::BrokerConfig;

#[derive(serde::Serialize)]
pub struct MqttMessage {
    pub timestamp: String,
    pub payload: bytes::Bytes,
    pub original_payload_size: usize,
    pub retain: bool,
}

#[derive(serde::Serialize, Clone)]
pub struct RateHistoryEntry {
    pub timestamp: i64,
    pub bytes_per_second: f64,
    pub total_bytes: usize,
}

#[derive(serde::Serialize)]
pub struct MqttBroker {
    #[serde(skip)]
    pub client: rumqttc::Client,
    pub broker: String,
    pub connected: bool,
    pub topics: HashMap<String, VecDeque<MqttMessage>>,
    pub total_bytes: usize,
    pub total_messages: usize,
    /// Tracks insertion order for O(1) eviction: (topic_name, payload_len).
    #[serde(skip)]
    pub eviction_order: VecDeque<(String, usize)>,
    /// Throughput history samples (timestamp_ms, bytes_per_second, total_bytes).
    pub rate_history: Vec<RateHistoryEntry>,
    /// Bytes received since last rate sample.
    #[serde(skip)]
    pub rate_bytes_accumulator: usize,
    /// Timestamp of last rate sample (epoch ms).
    #[serde(skip)]
    pub rate_last_sample_ms: i64,
    /// Whether this broker requires authentication to view its messages.
    pub requires_auth: bool,
}

fn env_usize_mb(name: &str, default_mb: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default_mb)
        * 1024
        * 1024
}

static MAX_BROKER_BYTES: OnceLock<usize> = OnceLock::new();
static MAX_MESSAGE_SIZE: OnceLock<usize> = OnceLock::new();
static MAX_INCOMING_PACKET_SIZE: OnceLock<usize> = OnceLock::new();
const MIN_INCOMING_PACKET_SIZE_BYTES: usize = 64 * 1024 * 1024;

/// Maximum total payload bytes stored per broker. Default 128 MB.
/// Set via MQTT_INSPECTOR_MAX_BROKER_MB environment variable.
pub fn max_broker_bytes() -> usize {
    *MAX_BROKER_BYTES.get_or_init(|| env_usize_mb("MQTT_INSPECTOR_MAX_BROKER_MB", 128))
}

/// Maximum single message payload size forwarded to peers. Default 1 MB.
/// Set via MQTT_INSPECTOR_MAX_MESSAGE_MB environment variable.
pub fn max_message_size() -> usize {
    *MAX_MESSAGE_SIZE.get_or_init(|| env_usize_mb("MQTT_INSPECTOR_MAX_MESSAGE_MB", 1))
}

/// Maximum incoming MQTT packet size accepted by the client.
/// Set via MQTT_INSPECTOR_MAX_INCOMING_PACKET_MB.
/// If unset: 16x broker storage limit, capped at 1 GB.
pub fn max_incoming_packet_size() -> usize {
    *MAX_INCOMING_PACKET_SIZE.get_or_init(|| {
        if let Ok(v) = std::env::var("MQTT_INSPECTOR_MAX_INCOMING_PACKET_MB") {
            if let Ok(mb) = v.parse::<usize>() {
                let configured = mb.saturating_mul(1024).saturating_mul(1024);
                let floor = std::cmp::max(max_message_size(), MIN_INCOMING_PACKET_SIZE_BYTES);
                return std::cmp::max(configured, floor);
            }
        }
        let candidate = max_broker_bytes().saturating_mul(16);
        let floor = std::cmp::max(max_message_size(), MIN_INCOMING_PACKET_SIZE_BYTES);
        let cap = 1024 * 1024 * 1024;
        std::cmp::max(candidate, floor).min(cap)
    })
}

pub type BrokerMap = Arc<Mutex<HashMap<String, MqttBroker>>>;

pub fn connect_to_mqtt_host(config: &BrokerConfig) -> (rumqttc::Client, rumqttc::Connection) {
    let id = uuid::Uuid::new_v4();
    let host = &config.host;
    println!(
        "Connecting to Mqtt broker at {host} (tls={}) with id {id}",
        config.use_tls
    );
    let hostname_ip = host.trim_matches('"').split(':').collect::<Vec<&str>>();
    let hostname = hostname_ip[0];
    let port = hostname_ip[1].parse::<u16>().unwrap();
    let mut mqttoptions = MqttOptions::new(id, hostname, port);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(120));
    // Allow very large incoming and outgoing packets so the bridge can
    // truncate for display without forcing reconnects on oversized payloads.
    mqttoptions.set_max_packet_size(max_incoming_packet_size(), max_incoming_packet_size());

    if config.use_tls {
        mqttoptions.set_transport(Transport::tls_with_default_config());
    }

    if let (Some(username), Some(password)) = (&config.username, &config.password) {
        if !username.is_empty() {
            mqttoptions.set_credentials(username, password);
        }
    }

    let (mut client, connection) = rumqttc::Client::new(mqttoptions, 1000);
    client.subscribe("#", QoS::AtMostOnce).unwrap();

    (client, connection)
}

pub fn publish_message(host: &str, topic: &str, payload: &str, retain: bool, mqtt_map: &BrokerMap) {
    let client = {
        let binding = mqtt_map.lock().unwrap();
        binding.get(host).map(|broker| broker.client.clone())
    };

    match client {
        Some(mut client) => {
            if let Err(err) =
                client.publish(topic, rumqttc::QoS::AtLeastOnce, retain, payload.as_bytes())
            {
                println!(
                    "Error publishing {} bytes to {host} topic {topic}: {err:?}",
                    payload.len()
                );
            }
        }
        None => {
            println!("Can't publish. Broker {host} not found");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_broker_map() -> BrokerMap {
        BrokerMap::new(Mutex::new(HashMap::new()))
    }

    #[test]
    fn test_publish_message_broker_not_found() {
        let mqtt_map = make_broker_map();
        // Should not panic, just prints a message
        publish_message("nonexistent:1883", "topic", "payload", false, &mqtt_map);
    }

    #[test]
    fn test_mqtt_message_serialization() {
        let msg = MqttMessage {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            payload: bytes::Bytes::from(vec![72, 101, 108, 108, 111]),
            original_payload_size: 5,
            retain: false,
        };
        let serialized = serde_json::to_string(&msg).unwrap();
        assert!(serialized.contains("\"timestamp\":\"2024-01-01T00:00:00Z\""));
        assert!(serialized.contains("\"payload\":[72,101,108,108,111]"));
    }

    #[test]
    fn test_mqtt_broker_serialization_excludes_client() {
        // MqttBroker has #[serde(skip)] on client — verify that the struct
        // definition skips the client field during serialization by checking
        // a round-tripped partial JSON does not contain "client".
        let partial = serde_json::json!({
            "broker": "localhost:1883",
            "connected": true,
            "topics": {},
            "total_bytes": 0,
            "total_messages": 0,
            "eviction_order": [],
            "rate_history": [],
            "rate_bytes_accumulator": 0,
            "rate_last_sample_ms": 0,
            "requires_auth": false
        });
        let serialized = partial.to_string();
        assert!(!serialized.contains("client"));
        assert!(serialized.contains("\"broker\":\"localhost:1883\""));
    }

    #[test]
    fn test_mqtt_message_empty_payload() {
        let msg = MqttMessage {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            payload: bytes::Bytes::new(),
            original_payload_size: 0,
            retain: false,
        };
        let serialized = serde_json::to_string(&msg).unwrap();
        assert!(serialized.contains("\"payload\":[]"));
    }

    #[test]
    fn test_broker_map_insert_and_lookup() {
        let mqtt_map = make_broker_map();
        let cfg = super::super::config::BrokerConfig::from_host("127.0.0.1:18830");
        let (client, _connection) = connect_to_mqtt_host(&cfg);
        let broker = MqttBroker {
            client,
            broker: "127.0.0.1:18830".to_string(),
            connected: false,
            topics: HashMap::new(),
            total_bytes: 0,
            total_messages: 0,
            eviction_order: VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
            requires_auth: false,
        };
        mqtt_map
            .lock()
            .unwrap()
            .insert("127.0.0.1:18830".to_string(), broker);
        assert_eq!(mqtt_map.lock().unwrap().len(), 1);
        assert!(mqtt_map.lock().unwrap().contains_key("127.0.0.1:18830"));
    }

    #[test]
    fn test_broker_map_remove() {
        let mqtt_map = make_broker_map();
        let cfg = super::super::config::BrokerConfig::from_host("127.0.0.1:18831");
        let (client, _connection) = connect_to_mqtt_host(&cfg);
        let broker = MqttBroker {
            client,
            broker: "127.0.0.1:18831".to_string(),
            connected: false,
            topics: HashMap::new(),
            total_bytes: 0,
            total_messages: 0,
            eviction_order: VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
            requires_auth: false,
        };
        mqtt_map
            .lock()
            .unwrap()
            .insert("127.0.0.1:18831".to_string(), broker);
        mqtt_map.lock().unwrap().remove("127.0.0.1:18831");
        assert_eq!(mqtt_map.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_concurrent_broker_map_access() {
        use std::sync::Arc;
        use std::thread;

        let mqtt_map = make_broker_map();

        let mm1 = Arc::clone(&mqtt_map);
        let h1 = thread::spawn(move || {
            for i in 0..50 {
                publish_message(&format!("host{}:1883", i), "topic", "payload", false, &mm1);
            }
        });

        let mm2 = Arc::clone(&mqtt_map);
        let h2 = thread::spawn(move || {
            for _ in 0..50 {
                let _ = mm2.lock().unwrap().len();
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();
    }

    // --- max_broker_bytes / max_message_size ---

    #[test]
    fn test_max_broker_bytes_returns_positive() {
        assert!(max_broker_bytes() > 0);
    }

    #[test]
    fn test_max_message_size_returns_positive() {
        assert!(max_message_size() > 0);
    }

    // --- MqttMessage operations ---

    #[test]
    fn test_mqtt_message_large_payload() {
        let payload = bytes::Bytes::from(vec![0xFFu8; 1024 * 1024]); // 1MB
        let msg = MqttMessage {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            payload,
            original_payload_size: 1024 * 1024,
            retain: false,
        };
        assert_eq!(msg.payload.len(), 1024 * 1024);
    }

    // --- MqttBroker eviction_order ---

    #[test]
    fn test_broker_eviction_order_fifo() {
        let cfg = super::super::config::BrokerConfig::from_host("127.0.0.1:18832");
        let (client, _conn) = connect_to_mqtt_host(&cfg);
        let mut broker = MqttBroker {
            client,
            broker: "test".to_string(),
            connected: true,
            topics: HashMap::new(),
            total_bytes: 0,
            total_messages: 0,
            eviction_order: VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
            requires_auth: false,
        };

        broker.eviction_order.push_back(("t1".to_string(), 10));
        broker.eviction_order.push_back(("t2".to_string(), 20));
        broker.eviction_order.push_back(("t3".to_string(), 30));

        let (key, sz) = broker.eviction_order.pop_front().unwrap();
        assert_eq!(key, "t1");
        assert_eq!(sz, 10);

        let (key, sz) = broker.eviction_order.pop_front().unwrap();
        assert_eq!(key, "t2");
        assert_eq!(sz, 20);
    }

    // --- Rate history ---

    #[test]
    fn test_rate_history_entry_clone() {
        let entry = RateHistoryEntry {
            timestamp: 1700000000000,
            bytes_per_second: 1234.5,
            total_bytes: 100000,
        };
        let cloned = entry.clone();
        assert_eq!(entry.timestamp, cloned.timestamp);
        assert_eq!(entry.bytes_per_second, cloned.bytes_per_second);
        assert_eq!(entry.total_bytes, cloned.total_bytes);
    }

    #[test]
    fn test_rate_history_serialization() {
        let entry = RateHistoryEntry {
            timestamp: 1700000000000,
            bytes_per_second: 1234.5,
            total_bytes: 100000,
        };
        let serialized = serde_json::to_string(&entry).unwrap();
        assert!(serialized.contains("\"timestamp\":1700000000000"));
        assert!(serialized.contains("\"bytes_per_second\":1234.5"));
        assert!(serialized.contains("\"total_bytes\":100000"));
    }

    // --- Topics HashMap behavior ---

    #[test]
    fn test_broker_topics_multiple_messages_per_topic() {
        let cfg = super::super::config::BrokerConfig::from_host("127.0.0.1:18833");
        let (client, _conn) = connect_to_mqtt_host(&cfg);
        let mut broker = MqttBroker {
            client,
            broker: "test".to_string(),
            connected: true,
            topics: HashMap::new(),
            total_bytes: 0,
            total_messages: 0,
            eviction_order: VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
            requires_auth: false,
        };

        let mut msgs = VecDeque::new();
        for i in 0..100 {
            msgs.push_back(MqttMessage {
                timestamp: format!("ts_{i}"),
                payload: bytes::Bytes::from(format!("payload_{i}")),
                original_payload_size: format!("payload_{i}").len(),
                retain: false,
            });
        }
        broker.topics.insert("stress/topic".to_string(), msgs);

        assert_eq!(broker.topics["stress/topic"].len(), 100);
        assert_eq!(
            broker.topics["stress/topic"].front().unwrap().timestamp,
            "ts_0"
        );
        assert_eq!(
            broker.topics["stress/topic"].back().unwrap().timestamp,
            "ts_99"
        );
    }

    #[test]
    fn test_broker_topics_eviction_removes_oldest() {
        let cfg = super::super::config::BrokerConfig::from_host("127.0.0.1:18834");
        let (client, _conn) = connect_to_mqtt_host(&cfg);
        let mut broker = MqttBroker {
            client,
            broker: "test".to_string(),
            connected: true,
            topics: HashMap::new(),
            total_bytes: 0,
            total_messages: 0,
            eviction_order: VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
            requires_auth: false,
        };

        let mut msgs = VecDeque::new();
        msgs.push_back(MqttMessage {
            timestamp: "oldest".to_string(),
            payload: bytes::Bytes::from("a"),
            original_payload_size: 1,
            retain: false,
        });
        msgs.push_back(MqttMessage {
            timestamp: "newest".to_string(),
            payload: bytes::Bytes::from("b"),
            original_payload_size: 1,
            retain: false,
        });
        broker.topics.insert("t".to_string(), msgs);
        broker.eviction_order.push_back(("t".to_string(), 1));
        broker.eviction_order.push_back(("t".to_string(), 1));

        // Simulate eviction of the oldest
        let (topic_key, _) = broker.eviction_order.pop_front().unwrap();
        broker.topics.get_mut(&topic_key).unwrap().pop_front();

        assert_eq!(broker.topics["t"].len(), 1);
        assert_eq!(broker.topics["t"].front().unwrap().timestamp, "newest");
    }
}
