export function requestMqttBrokerConnection(ip: string, port: string, socket: WebSocket) {
    // TODO: Implement id generation
    socket.send(`{
        "jsonrpc": "2.0",
        "method": "connect",
        "params": {
            "ip": "${ip}",
            "port": "${port}"
        },
        "id": "1"
    }`)
}

export function requestPublishMqttMessage(ip: string, port: string, topic: string, payload: string, socket: WebSocket) {
    // TODO: Implement id generation
    socket.send(`{
        "jsonrpc": "2.0",
        "method": "publish",
        "params": {
            "ip": "${ip}",
            "port": "${port}",
            "topic": "${topic}",
            "payload": "${payload}"
        },
        "id": "1"
    }`)
}