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
    // Hey if you find a better way, feel free to help me out
    const sanitized_payload = payload
        .replace(/\\/g, '\\\\')
        .replace(/\//g, '\\/')
        .replace(/"/g, '\\"')
        .replace(/\n/g, '\\n')
        .replace(/\r/g, '\\r')
        .replace(/\t/g, '\\t')
        .replace(/\f/g, '\\f');

    const message = `{
        "jsonrpc": "2.0",
        "method": "publish",
        "params": {
            "ip": "${ip}",
            "port": "${port}",
            "topic": "${topic}",
            "payload": "${sanitized_payload}"
        }
    }`;

    socket.send(message);
}