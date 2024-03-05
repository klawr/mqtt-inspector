/*
 * Copyright (c) 2024 Kai Lawrence
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
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use rumqttc::{MqttOptions, QoS};
use serde;

#[derive(serde::Serialize)]
pub struct MqttMessage {
    pub timestamp: String,
    pub payload: String,
}

#[derive(serde::Serialize)]
pub struct MqttTopics {
    pub topic: String,
    pub messages: Vec<MqttMessage>,
}

#[derive(serde::Serialize)]
pub struct MqttBroker {
    #[serde(skip)]
    pub client: rumqttc::Client,
    pub broker: String,
    pub connected: bool,
    pub topics: Vec<MqttTopics>,
}

// TODO SocketAddr should be String?
pub type Map = Arc<Mutex<HashMap<SocketAddr, MqttBroker>>>;

pub fn connect_to_mqtt_host(host: &str, port: u16) -> (rumqttc::Client, rumqttc::Connection) {
    let id = uuid::Uuid::new_v4();
    println!(
        "Connecting to Mqtt broker at {}:{} with id {}",
        host, port, id
    );
    let mut mqttoptions = MqttOptions::new(id, host, port);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));
    mqttoptions.set_max_packet_size(1000000 * 1024, 1000000 * 1024);

    let (mut client, connection) = rumqttc::Client::new(mqttoptions, 10);
    client.subscribe("#", QoS::AtMostOnce).unwrap();

    return (client, connection);
}

pub fn publish_message(ip: &str, port: &str, topic: &str, payload: &str, mqtt_map: Map) {
    mqtt_map.lock().unwrap().iter().for_each(|(addr, broker)| {
        if addr.ip().to_string() == ip && addr.port().to_string() == port {
            broker
                .client
                .clone()
                .publish(topic, rumqttc::QoS::AtLeastOnce, false, payload.as_bytes())
                .unwrap();
        }
    });
}
