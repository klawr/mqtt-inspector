import { test, expect } from 'vitest';
import {
	requestCommandAddition,
	requestMqttBrokerConnection,
	requestMqttBrokerRemoval,
	requestPipelineAddition,
	requestPipelineRemoval,
	requestPublishMqttMessage,
	requestSubscribeTopic,
	requestUnsubscribeTopic
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

	requestPublishMqttMessage(host, topic, payload, false, socket as unknown as WebSocket);

	const expectedMessage = `{
            "jsonrpc": "2.0",
            "method": "publish",
            "params": {
                "host": "${host}",
                "topic": "${topic}",
                "payload": "${payload}",
                "retain": false
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

test('requestMqttBrokerConnection includes certificate paths when given', () => {
	const socket = new MockWebSocket();
	requestMqttBrokerConnection('broker:8883', socket as unknown as WebSocket, {
		use_tls: true,
		ca_cert: 'certs/ca.pem',
		client_cert: 'certs/client.crt',
		client_key: 'certs/client.key',
		client_key_password: 'key-pw'
	});

	const parsed = JSON.parse(socket.messages[0]);
	expect(parsed.method).toBe('connect');
	expect(parsed.params).toEqual({
		hostname: 'broker:8883',
		use_tls: true,
		ca_cert: 'certs/ca.pem',
		client_cert: 'certs/client.crt',
		client_key: 'certs/client.key',
		client_key_password: 'key-pw'
	});
});

test('requestMqttBrokerConnection omits empty certificate paths', () => {
	const socket = new MockWebSocket();
	requestMqttBrokerConnection('broker:8883', socket as unknown as WebSocket, {
		use_tls: true,
		ca_cert: '',
		client_cert: undefined,
		client_key: '',
		client_key_password: ''
	});

	const parsed = JSON.parse(socket.messages[0]);
	expect(parsed.params).toEqual({ hostname: 'broker:8883', use_tls: true });
});

test('requestMqttBrokerConnection includes access_password only when set', () => {
	const socket = new MockWebSocket();
	requestMqttBrokerConnection('broker:1883', socket as unknown as WebSocket, {
		access_password: 'view-pw'
	});
	requestMqttBrokerConnection('broker:1883', socket as unknown as WebSocket, {
		access_password: ''
	});

	expect(JSON.parse(socket.messages[0]).params).toEqual({
		hostname: 'broker:1883',
		access_password: 'view-pw'
	});
	expect(JSON.parse(socket.messages[1]).params).toEqual({ hostname: 'broker:1883' });
});

test('requestMqttBrokerConnection includes allowed_roles only when non-empty', () => {
	const socket = new MockWebSocket();
	requestMqttBrokerConnection('broker:1883', socket as unknown as WebSocket, {
		allowed_roles: ['plant-a', 'ops']
	});
	requestMqttBrokerConnection('broker:1883', socket as unknown as WebSocket, {
		allowed_roles: []
	});

	expect(JSON.parse(socket.messages[0]).params).toEqual({
		hostname: 'broker:1883',
		allowed_roles: ['plant-a', 'ops']
	});
	expect(JSON.parse(socket.messages[1]).params).toEqual({ hostname: 'broker:1883' });
});

test('requestSubscribeTopic sends broker and topic', () => {
	const socket = new MockWebSocket();
	requestSubscribeTopic('broker:1883', 'test/topic', socket as unknown as WebSocket);

	const parsed = JSON.parse(socket.messages[0]);
	expect(parsed.jsonrpc).toBe('2.0');
	expect(parsed.method).toBe('subscribe_topic');
	expect(parsed.params.broker).toBe('broker:1883');
	expect(parsed.params.topic).toBe('test/topic');
});

test('requestSubscribeTopic omits since_timestamp when not provided', () => {
	const socket = new MockWebSocket();
	requestSubscribeTopic('broker:1883', 'test/topic', socket as unknown as WebSocket);

	const parsed = JSON.parse(socket.messages[0]);
	expect('since_timestamp' in parsed.params).toBe(false);
});

test('requestSubscribeTopic includes since_timestamp for delta re-sync', () => {
	const socket = new MockWebSocket();
	requestSubscribeTopic(
		'broker:1883',
		'test/topic',
		socket as unknown as WebSocket,
		'2024-01-01T00:00:02Z'
	);

	const parsed = JSON.parse(socket.messages[0]);
	expect(parsed.params.since_timestamp).toBe('2024-01-01T00:00:02Z');
});

test('requestUnsubscribeTopic sends broker and topic', () => {
	const socket = new MockWebSocket();
	requestUnsubscribeTopic('broker:1883', 'test/topic', socket as unknown as WebSocket);

	const parsed = JSON.parse(socket.messages[0]);
	expect(parsed.method).toBe('unsubscribe_topic');
	expect(parsed.params.broker).toBe('broker:1883');
	expect(parsed.params.topic).toBe('test/topic');
});

test('requestPublishMqttMessage handles special characters in payload', () => {
	const socket = new MockWebSocket();
	requestPublishMqttMessage(
		'host',
		'topic',
		'{"key":"value"}',
		false,
		socket as unknown as WebSocket
	);

	const parsed = JSON.parse(socket.messages[0]);
	expect(parsed.params.payload).toBe('{"key":"value"}');
});

test('requestMqttBrokerConnection with port number in hostname', () => {
	const socket = new MockWebSocket();
	requestMqttBrokerConnection('mqtt.example.com:8883', socket as unknown as WebSocket);

	const parsed = JSON.parse(socket.messages[0]);
	expect(parsed.params.hostname).toBe('mqtt.example.com:8883');
});
