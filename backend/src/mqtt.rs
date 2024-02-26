use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use rumqttc::{MqttOptions, QoS};

pub type Map = Arc<Mutex<HashMap<SocketAddr, rumqttc::Client>>>;

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
    mqtt_map.lock().unwrap().iter().for_each(|(addr, client)| {
        if addr.ip().to_string() == ip && addr.port().to_string() == port {
            client
                .clone()
                .publish(topic, rumqttc::QoS::AtLeastOnce, false, payload.as_bytes())
                .unwrap();
        }
    });
}
