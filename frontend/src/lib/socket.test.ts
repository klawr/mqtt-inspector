import { test, expect } from 'vitest';
import {
	requestCommandAddition,
	requestMqttBrokerConnection,
	requestMqttBrokerRemoval,
	requestPipelineAddition,
	requestPipelineRemoval,
	requestPublishMqttMessage
} from './socket';

class MockWebSocket {
	messages: string[] = [];
	send(message: string) {
		this.messages.push(message);
	}
}

test('requestPublishMqttMessage sends correct message to WebSocket', () => {
	const socket = new MockWebSocket();
	const host = 'mqtt.example.com';
	const topic = 'test/topic';
	const payload = 'Hello, World!';

	requestPublishMqttMessage(host, topic, payload, socket as unknown as WebSocket);

	const expectedMessage = `{
            "jsonrpc": "2.0",
            "method": "publish",
            "params": {
                "host": "${host}",
                "topic": "${topic}",
                "payload": "${payload}"
            }
        }`;

	expect(socket.messages.length).toBe(1);
	expect(socket.messages[0].replace(/\s/g, '')).toBe(expectedMessage.replace(/\s/g, ''));
});

test('requestCommandAddition sends correct message to WebSocket', () => {
	const socket = new MockWebSocket();
	const saveCommandName = 'cmd1';
	const topic = 'topic1';
	const payload = 'payload1';

	requestCommandAddition(saveCommandName, topic, payload, socket as unknown as WebSocket);

	const expectedMessage = `{
      "jsonrpc": "2.0",
      "method": "save_command",
      "params": { "name": "${saveCommandName}", "topic": "${topic}", "payload": "${payload}" }
    }`;

	expect(socket.messages.length).toBe(1);
	expect(socket.messages[0].replace(/\s/g, '')).toBe(expectedMessage.replace(/\s/g, ''));
});

test('requestPipelineAddition sends correct message to WebSocket', () => {
	const socket = new MockWebSocket();
	const pipelineName = 'pipeline1';
	const newPipeline = [{ topic: 'topic1' }, { topic: 'topic2' }];

	requestPipelineAddition(pipelineName, newPipeline, socket as unknown as WebSocket);

	const expectedMessage = `{
      "jsonrpc": "2.0",
      "method": "save_pipeline",
      "params": { "name": "${pipelineName}", "pipeline": ${JSON.stringify(newPipeline)} }
    }`;

	expect(socket.messages.length).toBe(1);
	expect(socket.messages[0].replace(/\s/g, '')).toBe(expectedMessage.replace(/\s/g, ''));
});

test('requestMqttBrokerRemoval sends correct message to WebSocket', () => {
	const socket = new MockWebSocket();
	const hostname = 'mqtt.example.com';

	requestMqttBrokerRemoval(hostname, socket as unknown as WebSocket);

	const expectedMessage = `{
      "jsonrpc": "2.0",
      "method": "remove",
      "params": { "hostname": "${hostname}" }
    }`;

	expect(socket.messages.length).toBe(1);
	expect(socket.messages[0].replace(/\s/g, '')).toBe(expectedMessage.replace(/\s/g, ''));
});

test('requestPipelineRemoval sends correct message to WebSocket', () => {
	const socket = new MockWebSocket();
	const pipeline = 'pipeline1';

	requestPipelineRemoval(pipeline, socket as unknown as WebSocket);

	const expectedMessage = `{
      "jsonrpc": "2.0",
      "method": "remove_pipeline",
      "params": { "name": "${pipeline}" }
    }`;

	expect(socket.messages.length).toBe(1);
	expect(socket.messages[0].replace(/\s/g, '')).toBe(expectedMessage.replace(/\s/g, ''));
});

test('requestMqttBrokerConnection sends correct message to WebSocket', () => {
	const socket = new MockWebSocket();
	const hostname = 'mqtt.example.com';

	requestMqttBrokerConnection(hostname, socket as unknown as WebSocket);

	const expectedMessage = `{
      "jsonrpc": "2.0",
      "method": "connect",
      "params": { "hostname": "${hostname}" }
    }`;

	expect(socket.messages.length).toBe(1);
	expect(socket.messages[0].replace(/\s/g, '')).toBe(expectedMessage.replace(/\s/g, ''));
});
