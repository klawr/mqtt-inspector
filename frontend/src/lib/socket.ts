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

export function requestMqttBrokerRemoval(hostname: string, socket: WebSocket) {
    socket.send(`{
        "jsonrpc": "2.0",
        "method": "remove",
        "params": {
            "hostname": "${hostname}"
        }
    }`)
}

export function requestMqttBrokerConnection(hostname: string, socket: WebSocket) {
    // TODO: Implement id generation
    socket.send(`{
        "jsonrpc": "2.0",
        "method": "connect",
        "params": {
            "hostname": "${hostname}"
        }
    }`)
}

export function requestPublishMqttMessage(host: string, topic: string, payload: string, socket: WebSocket) {
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
            "host": "${host}",
            "topic": "${topic}",
            "payload": "${sanitized_payload}"
        }
    }`;

    socket.send(message);
}