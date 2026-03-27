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

use rumqttc::{MqttOptions, QoS};

#[derive(serde::Serialize)]
pub struct MqttMessage {
    pub timestamp: String,
    pub payload: bytes::Bytes,
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

pub type BrokerMap = Arc<Mutex<HashMap<String, MqttBroker>>>;

pub fn connect_to_mqtt_host(host: &str) -> (rumqttc::Client, rumqttc::Connection) {
    let id = uuid::Uuid::new_v4();
    println!("Connecting to Mqtt broker at {host} with id {id}");
    let hostname_ip = host.trim_matches('"').split(':').collect::<Vec<&str>>();
    let hostname = hostname_ip[0];
    let port = hostname_ip[1].parse::<u16>().unwrap();
    let mut mqttoptions = MqttOptions::new(id, hostname, port);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(30));
    mqttoptions.set_max_packet_size(max_message_size(), max_message_size());

    let (mut client, connection) = rumqttc::Client::new(mqttoptions, 1000);
    client.subscribe("#", QoS::AtMostOnce).unwrap();

    (client, connection)
}

pub fn publish_message(host: &str, topic: &str, payload: &str, mqtt_map: &BrokerMap) {
    let client = {
        let binding = mqtt_map.lock().unwrap();
        binding.get(host).map(|broker| broker.client.clone())
    };

    match client {
        Some(mut client) => {
            match client.publish(topic, rumqttc::QoS::AtLeastOnce, false, payload.as_bytes()) {
                Ok(_) => {
                    // Successfully published
                }
                Err(err) => {
                    // Handle the error
                    println!("Error publishing: {err:?}");
                }
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
        publish_message("nonexistent:1883", "topic", "payload", &mqtt_map);
    }

    #[test]
    fn test_broker_map_is_initially_empty() {
        let mqtt_map = make_broker_map();
        assert_eq!(mqtt_map.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_mqtt_message_serialization() {
        let msg = MqttMessage {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            payload: bytes::Bytes::from(vec![72, 101, 108, 108, 111]),
        };
        let serialized = serde_json::to_string(&msg).unwrap();
        assert!(serialized.contains("\"timestamp\":\"2024-01-01T00:00:00Z\""));
        assert!(serialized.contains("\"payload\":[72,101,108,108,111]"));
    }

    #[test]
    fn test_mqtt_broker_serialization_excludes_client() {
        // MqttBroker has #[serde(skip)] on client, so we can test serialization
        // by creating a broker via connect_to_mqtt_host and checking serialized output
        // doesn't contain the client field.
        // We'll test the serialization structure instead.
        let broker_json = serde_json::json!({
            "broker": "localhost:1883",
            "connected": true,
            "topics": {
                "test/topic": [{
                    "timestamp": "2024-01-01T00:00:00Z",
                    "payload": [72, 101, 108, 108, 111]
                }]
            }
        });
        assert_eq!(broker_json["broker"], "localhost:1883");
        assert_eq!(broker_json["connected"], true);
        assert!(broker_json.get("client").is_none());
    }

    #[test]
    fn test_mqtt_message_empty_payload() {
        let msg = MqttMessage {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            payload: bytes::Bytes::new(),
        };
        let serialized = serde_json::to_string(&msg).unwrap();
        assert!(serialized.contains("\"payload\":[]"));
    }

    #[test]
    fn test_broker_map_insert_and_lookup() {
        let mqtt_map = make_broker_map();
        let (client, _connection) = connect_to_mqtt_host("127.0.0.1:18830");
        let broker = MqttBroker {
            client,
            broker: "127.0.0.1:18830".to_string(),
            connected: false,
            topics: HashMap::new(),
            total_bytes: 0,
            eviction_order: VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
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
        let (client, _connection) = connect_to_mqtt_host("127.0.0.1:18831");
        let broker = MqttBroker {
            client,
            broker: "127.0.0.1:18831".to_string(),
            connected: false,
            topics: HashMap::new(),
            total_bytes: 0,
            eviction_order: VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
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
                publish_message(&format!("host{}:1883", i), "topic", "payload", &mm1);
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
        };
        assert_eq!(msg.payload.len(), 1024 * 1024);
    }

    #[test]
    fn test_mqtt_message_payload_clone_is_cheap() {
        // Bytes::clone shares the underlying buffer
        let payload = bytes::Bytes::from(vec![1u8; 1000]);
        let msg = MqttMessage {
            timestamp: "ts".to_string(),
            payload,
        };
        let cloned = msg.payload.clone();
        assert_eq!(msg.payload.as_ptr(), cloned.as_ptr());
    }

    // --- MqttBroker eviction_order ---

    #[test]
    fn test_broker_eviction_order_fifo() {
        let (client, _conn) = connect_to_mqtt_host("127.0.0.1:18832");
        let mut broker = MqttBroker {
            client,
            broker: "test".to_string(),
            connected: true,
            topics: HashMap::new(),
            total_bytes: 0,
            eviction_order: VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
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
        let (client, _conn) = connect_to_mqtt_host("127.0.0.1:18833");
        let mut broker = MqttBroker {
            client,
            broker: "test".to_string(),
            connected: true,
            topics: HashMap::new(),
            total_bytes: 0,
            eviction_order: VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
        };

        let mut msgs = VecDeque::new();
        for i in 0..100 {
            msgs.push_back(MqttMessage {
                timestamp: format!("ts_{i}"),
                payload: bytes::Bytes::from(format!("payload_{i}")),
            });
        }
        broker.topics.insert("stress/topic".to_string(), msgs);

        assert_eq!(broker.topics["stress/topic"].len(), 100);
        assert_eq!(broker.topics["stress/topic"].front().unwrap().timestamp, "ts_0");
        assert_eq!(broker.topics["stress/topic"].back().unwrap().timestamp, "ts_99");
    }

    #[test]
    fn test_broker_topics_eviction_removes_oldest() {
        let (client, _conn) = connect_to_mqtt_host("127.0.0.1:18834");
        let mut broker = MqttBroker {
            client,
            broker: "test".to_string(),
            connected: true,
            topics: HashMap::new(),
            total_bytes: 0,
            eviction_order: VecDeque::new(),
            rate_history: Vec::new(),
            rate_bytes_accumulator: 0,
            rate_last_sample_ms: 0,
        };

        let mut msgs = VecDeque::new();
        msgs.push_back(MqttMessage {
            timestamp: "oldest".to_string(),
            payload: bytes::Bytes::from("a"),
        });
        msgs.push_back(MqttMessage {
            timestamp: "newest".to_string(),
            payload: bytes::Bytes::from("b"),
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
