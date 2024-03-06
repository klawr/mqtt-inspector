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
    sync::{Arc, Mutex},
};

use rumqttc::{MqttOptions, QoS};
use serde;

#[derive(serde::Serialize)]
pub struct MqttMessage {
    pub timestamp: String,
    pub payload: Vec<u8>,
}

#[derive(serde::Serialize)]
pub struct MqttBroker {
    #[serde(skip)]
    pub client: rumqttc::Client,
    pub broker: String,
    pub connected: bool,
    pub topics: HashMap<String, Vec<MqttMessage>>,
}

pub type BrokerMap = Arc<Mutex<HashMap<String, MqttBroker>>>;

pub fn connect_to_mqtt_host(host: &str) -> (rumqttc::Client, rumqttc::Connection) {
    let id = uuid::Uuid::new_v4();
    println!("Connecting to Mqtt broker at {} with id {}", host, id);
    let hostname_ip = host.trim_matches('"').split(":").collect::<Vec<&str>>();
    let hostname = hostname_ip[0];
    let port = hostname_ip[1].parse::<u16>().unwrap();
    let mut mqttoptions = MqttOptions::new(id, hostname, port);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));
    mqttoptions.set_max_packet_size(1000000 * 1024, 1000000 * 1024);

    let (mut client, connection) = rumqttc::Client::new(mqttoptions, 10);
    client.subscribe("#", QoS::AtMostOnce).unwrap();

    return (client, connection);
}

pub fn publish_message(host: &str, topic: &str, payload: &str, mqtt_map: &BrokerMap) {
    let binding = mqtt_map.lock().unwrap();
    let broker = binding.get(host);

    if let Some(broker) = broker {
        match broker.client.clone().publish(
            topic,
            rumqttc::QoS::AtLeastOnce,
            false,
            payload.as_bytes(),
        ) {
            Ok(_) => {
                // Successfully published
            }
            Err(err) => {
                // Handle the error
                println!("Error publishing: {}", err);
            }
        }
    } else {
        println!("Can't publish. Broker {} not found", host);
    }
}
