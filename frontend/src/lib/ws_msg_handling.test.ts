import { test, expect } from 'vitest';
import {
	processConfigs,
	type CommandParam,
	processConnectionStatus,
	type MqttConnectionStatus,
	processBrokerRemoval,
	type BrokerParam,
	processBrokers,
	processMQTTMessage,
	processMQTTMessages,
	parseMqttWebSocketMessage,
	type MQTTMessageParam,
	processSettings,
	processRateHistorySample,
	type RateHistorySampleParam,
	processTopicSummaries,
	type TopicSummariesParam,
	processMQTTMessageMeta,
	type MQTTMessageMetaParam,
	processMessagesEvicted,
	type MessagesEvictedParam,
	processMQTTMessageMetaBatch,
	processMessagesEvictedBatch,
	processTopicMessagesClear,
	processPipelines,
	type PipelineParam
} from './ws_msg_handling';
import { AppState, type BrokerRepository, type Treebranch } from './state';

test('processConfigs processes commands correctly', () => {
	const commands: CommandParam[] = [
		{ id: '1', name: 'Command1', topic: 'topic1', payload: 'payload1' },
		{ id: '2', name: 'Command2', topic: 'topic2', payload: 'payload2' },
		{ id: '3', name: 'Command3', topic: 'topic3', payload: 'payload3' }
	];

	const expectedResult = [
		{ id: '0', text: 'Command1', topic: 'topic1', payload: 'payload1' },
		{ id: '1', text: 'Command2', topic: 'topic2', payload: 'payload2' },
		{ id: '2', text: 'Command3', topic: 'topic3', payload: 'payload3' }
	];

	const result = processConfigs(commands);

	expect(result).toEqual(expectedResult);
});

test('processConfigs handles empty array correctly', () => {
	const commands: CommandParam[] = [];
	const result = processConfigs(commands);

	expect(result).toEqual([]);
});

test('processConfigs processes a single command correctly', () => {
	const commands: CommandParam[] = [
		{ id: '1', name: 'Command1', topic: 'topic1', payload: 'payload1' }
	];
	const result = processConfigs(commands);

	expect(result).toEqual([{ id: '0', text: 'Command1', topic: 'topic1', payload: 'payload1' }]);
});

test('processBrokerRemoval deletes broker from AppState', () => {
	const appState = new AppState();
	appState.brokerRepository = {
		broker1: {
			topics: [],
			selectedTopic: null,
			pipeline: [],
			connected: false,
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			backendTotalMessages: 0,
			messagesPerSecond: 0,
			rateHistory: [],
		}
	};

	const result = processBrokerRemoval('broker1', appState);

	expect(result.brokerRepository['broker1']).toBeUndefined();
});

test('processBrokerRemoval clears selectedBroker when deleting the selected one', () => {
	const appState = new AppState();
	appState.selectedBroker = 'broker1';
	appState.brokerRepository = {
		broker1: {
			topics: [],
			selectedTopic: null,
			pipeline: [],
			connected: false,
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			backendTotalMessages: 0,
			messagesPerSecond: 0,
			rateHistory: [],
		}
	};

	const result = processBrokerRemoval('broker1', appState);

	expect(result.selectedBroker).toBe('');
	expect(result.selectedTopic).toBeNull();
});

test('processBrokerRemoval handles non-existing broker correctly', () => {
	const appState = new AppState();
	appState.brokerRepository = {
		broker1: {
			topics: [],
			selectedTopic: null,
			pipeline: [],
			connected: false,
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			backendTotalMessages: 0,
			messagesPerSecond: 0,
			rateHistory: [],
		}
	};

	const result = processBrokerRemoval('nonexistentBroker', appState);

	expect(result.brokerRepository['broker1']).toBeDefined();
});

test('processConnectionStatus updates connection status in AppState', () => {
	const appState = new AppState();
	appState.brokerRepository = {
		broker1: {
			connected: false,
			topics: [],
			selectedTopic: null,
			pipeline: [],
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			backendTotalMessages: 0,
			messagesPerSecond: 0,
			rateHistory: [],
		}
	};

	const connectionStatus: MqttConnectionStatus = { source: 'broker1', connected: true };
	const result = processConnectionStatus(connectionStatus, appState);

	expect(result.brokerRepository['broker1'].connected).toBe(true);
});

test('processConnectionStatus handles non-existing broker correctly', () => {
	const appState = new AppState();
	appState.brokerRepository = {
		broker1: {
			connected: false,
			topics: [],
			selectedTopic: null,
			pipeline: [],
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			backendTotalMessages: 0,
			messagesPerSecond: 0,
			rateHistory: [],
		}
	};

	const connectionStatus: MqttConnectionStatus = { source: 'nonexistentBroker', connected: true };
	const result = processConnectionStatus(connectionStatus, appState);

	expect(result.brokerRepository['broker1'].connected).toBe(false);
});

test('AppState class initializes correctly', () => {
	const appState = new AppState();

	expect(appState.selectedBroker).toBe('');
	expect(appState.selectedTopic).toBeNull();
	expect(appState.brokerRepository).toEqual({});
	expect(appState.pipelines).toEqual([]);
	expect(appState.commands).toEqual([]);
});

test('processBrokers updates BrokerRepository correctly', () => {
	const brokerRepository: BrokerRepository = {};

	const params: BrokerParam = [
		{
			broker: 'broker1',
			connected: true,
			topics: {}
		},
		{
			broker: 'broker2',
			connected: false,
			topics: {}
		}
	];

	const result = processBrokers(params, brokerRepository);

	expect(result).toEqual({
		broker1: {
			topics: [],
			selectedTopic: null,
			pipeline: [],
			connected: true,
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			backendTotalMessages: 0,
			messagesPerSecond: 0,
			rateHistory: []
		},
		broker2: {
			topics: [],
			selectedTopic: null,
			pipeline: [],
			connected: false,
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			backendTotalMessages: 0,
			messagesPerSecond: 0,
			rateHistory: []
		}
	});
});

test('processBrokers handles empty params correctly', () => {
	const brokerRepository: BrokerRepository = {};

	const params: BrokerParam = [];

	const result = processBrokers(params, brokerRepository);

	expect(result).toEqual({});
});

test('parseMqttWebSocketMessage parses binary mqtt frame', () => {
	const header = JSON.stringify({
		jsonrpc: '2.0',
		method: 'mqtt_message',
		params: {
			source: 'broker1',
			topic: 'topic1',
			timestamp: '2022-01-01T00:00:00.000Z',
			total_bytes: 5
		}
	});
	const headerBytes = new TextEncoder().encode(header);
	const payload = new TextEncoder().encode('Hello');
	const buffer = new Uint8Array(4 + headerBytes.length + payload.length);
	new DataView(buffer.buffer).setUint32(0, headerBytes.length, false);
	buffer.set(headerBytes, 4);
	buffer.set(payload, 4 + headerBytes.length);

	const parsed = parseMqttWebSocketMessage(buffer.buffer);

	expect(parsed?.method).toBe('mqtt_message');
	expect(parsed?.params.source).toBe('broker1');
	expect(new TextDecoder().decode(new Uint8Array(parsed?.params.payload))).toBe('Hello');
});

test('processMQTTMessage handles payload from parsed binary frame', () => {
	const app = new AppState();
	// Pre-create broker entry with topic tree (processMQTTMessage no longer auto-creates)
	app.brokerRepository['broker1'] = {
		topics: [
			{
				id: 'topic1',
				text: 'topic1 (1 message)',
				children: undefined,
				number_of_messages: 1,
				original_text: 'topic1',
				messages: []
			}
		],
		selectedTopic: null,
		pipeline: [],
		connected: true,
		backendTotalBytes: 17,
		bytesPerSecond: 0,
		backendTotalMessages: 0,
		messagesPerSecond: 0,
		rateHistory: []
	};
	const message: MQTTMessageParam = {
		source: 'broker1',
		topic: 'topic1',
		payload: new TextEncoder().encode('Hello from binary').buffer,
		timestamp: '2022-01-01T00:00:00.000Z',
		total_bytes: 17
	};

	processMQTTMessage(message, app);

	expect(app.brokerRepository['broker1'].topics[0].messages[0].text).toBe('Hello from binary');
});

test('processMQTTMessages applies batched messages in order', () => {
	const app = new AppState();
	// Pre-create broker entry with topic tree
	app.brokerRepository['broker1'] = {
		topics: [
			{
				id: 'topic1',
				text: 'topic1 (2 messages)',
				children: undefined,
				number_of_messages: 2,
				original_text: 'topic1',
				messages: []
			}
		],
		selectedTopic: null,
		pipeline: [],
		connected: true,
		backendTotalBytes: 11,
		bytesPerSecond: 0,
		backendTotalMessages: 0,
		messagesPerSecond: 0,
		rateHistory: []
	};
	const messages: MQTTMessageParam[] = [
		{
			source: 'broker1',
			topic: 'topic1',
			payload: new TextEncoder().encode('first').buffer,
			timestamp: '2022-01-01T00:00:00.000Z',
			total_bytes: 5
		},
		{
			source: 'broker1',
			topic: 'topic1',
			payload: new TextEncoder().encode('second').buffer,
			timestamp: '2022-01-01T00:00:01.000Z',
			total_bytes: 11
		}
	];

	processMQTTMessages(messages, app);

	expect(app.brokerRepository['broker1'].topics[0].messages.map((message) => message.text)).toEqual(
		['second', 'first']
	);
});

test('processSettings updates maxBrokerBytes on AppState', () => {
	const app = new AppState();
	const result = processSettings(
		{ max_broker_bytes: 256 * 1024 * 1024, max_message_size: 2 * 1024 * 1024 },
		app
	);
	expect(result.maxBrokerBytes).toBe(256 * 1024 * 1024);
});

test('processRateHistorySample appends entry and updates bytesPerSecond', () => {
	const app = new AppState();
	app.brokerRepository = {
		'broker1:1883': {
			topics: [],
			selectedTopic: null,
			pipeline: [],
			connected: true,
			backendTotalBytes: 5000,
			bytesPerSecond: 0,
			backendTotalMessages: 0,
			messagesPerSecond: 0,
			rateHistory: [],
		}
	};

	const params: RateHistorySampleParam = {
		source: 'broker1:1883',
		sample: { timestamp: 1700000000000, bytes_per_second: 1234.5, total_bytes: 5000 }
	};

	processRateHistorySample(params, app);
	const entry = app.brokerRepository['broker1:1883'];

	expect(entry.rateHistory).toHaveLength(1);
	expect(entry.rateHistory[0].bytesPerSecond).toBe(1234.5);
	expect(entry.rateHistory[0].totalBytes).toBe(5000);
	expect(entry.rateHistory[0].timestamp).toBe(1700000000000);
	expect(entry.bytesPerSecond).toBe(1234.5);
});

test('processRateHistorySample creates new array reference', () => {
	const app = new AppState();
	app.brokerRepository = {
		'broker1:1883': {
			topics: [],
			selectedTopic: null,
			pipeline: [],
			connected: true,
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			backendTotalMessages: 0,
			messagesPerSecond: 0,
			rateHistory: [],
		}
	};

	const oldRef = app.brokerRepository['broker1:1883'].rateHistory;
	processRateHistorySample(
		{
			source: 'broker1:1883',
			sample: { timestamp: Date.now(), bytes_per_second: 100, total_bytes: 0 }
		},
		app
	);
	const newRef = app.brokerRepository['broker1:1883'].rateHistory;

	// Must be a different array reference for Svelte reactivity
	expect(newRef).not.toBe(oldRef);
	expect(newRef).toHaveLength(1);
});

test('processRateHistorySample ignores unknown broker', () => {
	const app = new AppState();
	app.brokerRepository = {};

	const result = processRateHistorySample(
		{
			source: 'unknown:1883',
			sample: { timestamp: Date.now(), bytes_per_second: 100, total_bytes: 0 }
		},
		app
	);

	expect(result).toBe(app);
});

test('processBrokers populates rateHistory from backend data', () => {
	const brokerRepository: BrokerRepository = {};

	const params: BrokerParam = [
		{
			broker: 'broker1:1883',
			connected: true,
			topics: {},
			total_bytes: 5000,
			rate_history: [
				{ timestamp: 1700000000000, bytes_per_second: 100.5, total_bytes: 2000 },
				{ timestamp: 1700000010000, bytes_per_second: 200.5, total_bytes: 5000 }
			]
		}
	];

	const result = processBrokers(params, brokerRepository);

	expect(result['broker1:1883'].rateHistory).toHaveLength(2);
	expect(result['broker1:1883'].rateHistory[0]).toEqual({
		timestamp: 1700000000000,
		bytesPerSecond: 100.5,
		totalBytes: 2000
	});
	expect(result['broker1:1883'].rateHistory[1]).toEqual({
		timestamp: 1700000010000,
		bytesPerSecond: 200.5,
		totalBytes: 5000
	});
});

// ─── parseMqttWebSocketMessage edge cases ────────────────────────────

test('parseMqttWebSocketMessage returns null for buffer < 4 bytes', () => {
	const buffer = new Uint8Array([1, 2]).buffer;
	expect(parseMqttWebSocketMessage(buffer)).toBeNull();
});

test('parseMqttWebSocketMessage returns null for empty buffer', () => {
	const buffer = new ArrayBuffer(0);
	expect(parseMqttWebSocketMessage(buffer)).toBeNull();
});

test('parseMqttWebSocketMessage returns null when headerLength exceeds buffer', () => {
	const buffer = new Uint8Array(8);
	// Set header length to 9999 (way bigger than remaining bytes)
	new DataView(buffer.buffer).setUint32(0, 9999, false);
	expect(parseMqttWebSocketMessage(buffer.buffer)).toBeNull();
});

test('parseMqttWebSocketMessage returns null for non-mqtt_message method', () => {
	const header = JSON.stringify({
		jsonrpc: '2.0',
		method: 'other_method',
		params: { source: 'b' }
	});
	const headerBytes = new TextEncoder().encode(header);
	const buffer = new Uint8Array(4 + headerBytes.length);
	new DataView(buffer.buffer).setUint32(0, headerBytes.length, false);
	buffer.set(headerBytes, 4);

	expect(parseMqttWebSocketMessage(buffer.buffer)).toBeNull();
});

test('parseMqttWebSocketMessage handles empty payload (header only)', () => {
	const header = JSON.stringify({
		jsonrpc: '2.0',
		method: 'mqtt_message',
		params: { source: 'b', topic: 't', timestamp: 'ts' }
	});
	const headerBytes = new TextEncoder().encode(header);
	const buffer = new Uint8Array(4 + headerBytes.length);
	new DataView(buffer.buffer).setUint32(0, headerBytes.length, false);
	buffer.set(headerBytes, 4);

	const parsed = parseMqttWebSocketMessage(buffer.buffer);
	expect(parsed).not.toBeNull();
	expect(parsed?.params.source).toBe('b');
	expect(new Uint8Array(parsed?.params.payload).length).toBe(0);
});

test('parseMqttWebSocketMessage handles large payload', () => {
	const header = JSON.stringify({
		jsonrpc: '2.0',
		method: 'mqtt_message',
		params: { source: 'b', topic: 't', timestamp: 'ts' }
	});
	const headerBytes = new TextEncoder().encode(header);
	const payload = new Uint8Array(10000).fill(0xAB);
	const buffer = new Uint8Array(4 + headerBytes.length + payload.length);
	new DataView(buffer.buffer).setUint32(0, headerBytes.length, false);
	buffer.set(headerBytes, 4);
	buffer.set(payload, 4 + headerBytes.length);

	const parsed = parseMqttWebSocketMessage(buffer.buffer);
	expect(parsed).not.toBeNull();
	const resultPayload = new Uint8Array(parsed?.params.payload);
	expect(resultPayload.length).toBe(10000);
	expect(resultPayload[0]).toBe(0xAB);
});

// ─── processTopicSummaries ───────────────────────────────────────────

test('processTopicSummaries builds topic tree with correct counts', () => {
	const br: BrokerRepository = {};
	const params: TopicSummariesParam = {
		source: 'broker:1883',
		topics: {
			'a/b': { count: 5, latest_timestamp: 'ts1' },
			'a/c': { count: 3, latest_timestamp: 'ts2' }
		}
	};

	const result = processTopicSummaries(params, br);
	const entry = result['broker:1883'];
	expect(entry).toBeDefined();
	// Root node "a" should have 8 messages total (5 + 3)
	expect(entry.topics[0].original_text).toBe('a');
	expect(entry.topics[0].number_of_messages).toBe(8);
	// Two children: "b" with 5, "c" with 3
	expect(entry.topics[0].children).toHaveLength(2);
	const childB = entry.topics[0].children!.find((c) => c.original_text === 'b');
	const childC = entry.topics[0].children!.find((c) => c.original_text === 'c');
	expect(childB?.number_of_messages).toBe(5);
	expect(childC?.number_of_messages).toBe(3);
});

test('processTopicSummaries handles single topic with count 1', () => {
	const br: BrokerRepository = {};
	const params: TopicSummariesParam = {
		source: 'b:1883',
		topics: { 'test': { count: 1, latest_timestamp: 'ts' } }
	};

	const result = processTopicSummaries(params, br);
	expect(result['b:1883'].topics[0].number_of_messages).toBe(1);
	expect(result['b:1883'].topics[0].original_text).toBe('test');
});

test('processTopicSummaries handles deep topic path', () => {
	const br: BrokerRepository = {};
	const params: TopicSummariesParam = {
		source: 'b:1883',
		topics: { 'a/b/c/d/e': { count: 10, latest_timestamp: 'ts' } }
	};

	const result = processTopicSummaries(params, br);
	// Walk down the tree
	let branch = result['b:1883'].topics[0];
	expect(branch.original_text).toBe('a');
	expect(branch.number_of_messages).toBe(10);
	branch = branch.children![0];
	expect(branch.original_text).toBe('b');
	branch = branch.children![0];
	expect(branch.original_text).toBe('c');
	branch = branch.children![0];
	expect(branch.original_text).toBe('d');
	branch = branch.children![0];
	expect(branch.original_text).toBe('e');
	expect(branch.number_of_messages).toBe(10);
	expect(branch.children).toBeUndefined();
});

test('processTopicSummaries with empty topics is noop', () => {
	const br: BrokerRepository = {};
	const params: TopicSummariesParam = {
		source: 'b:1883',
		topics: {}
	};

	const result = processTopicSummaries(params, br);
	expect(result['b:1883'].topics).toHaveLength(0);
});

test('processTopicSummaries creates broker entry if missing', () => {
	const br: BrokerRepository = {};
	const params: TopicSummariesParam = {
		source: 'new:1883',
		topics: { 'x': { count: 1, latest_timestamp: 'ts' } }
	};

	const result = processTopicSummaries(params, br);
	expect(result['new:1883']).toBeDefined();
	expect(result['new:1883'].connected).toBe(true);
});

// ─── processMQTTMessageMeta ──────────────────────────────────────────

test('processMQTTMessageMeta creates broker entry and updates tree', () => {
	const app = new AppState();
	const meta: MQTTMessageMetaParam = {
		source: 'new_broker:1883',
		topic: 'test/topic',
		timestamp: '2024-01-01T00:00:00Z',
		payload_size: 100,
		total_bytes: 100,
		topic_message_count: 1
	};

	const result = processMQTTMessageMeta(meta, app);

	expect(result.brokerRepository['new_broker:1883']).toBeDefined();
	expect(result.selectedBroker).toBe('new_broker:1883');
	const topics = result.brokerRepository['new_broker:1883'].topics;
	expect(topics[0].original_text).toBe('test');
	expect(topics[0].number_of_messages).toBe(1);
});

test('processMQTTMessageMeta increments existing topic count', () => {
	const app = new AppState();
	app.brokerRepository['b:1883'] = {
		topics: [],
		selectedTopic: null,
		pipeline: [],
		connected: true,
		backendTotalBytes: 0,
		bytesPerSecond: 0,
		backendTotalMessages: 0,
		messagesPerSecond: 0,
		rateHistory: []
	};

	const meta: MQTTMessageMetaParam = {
		source: 'b:1883',
		topic: 'test',
		timestamp: '2024-01-01T00:00:00Z',
		payload_size: 50,
		total_bytes: 50,
		topic_message_count: 1
	};

	processMQTTMessageMeta(meta, app);
	processMQTTMessageMeta({ ...meta, total_bytes: 100, topic_message_count: 2 }, app);

	expect(app.brokerRepository['b:1883'].topics[0].number_of_messages).toBe(2);
});

test('processMQTTMessageMeta does not overwrite selectedBroker', () => {
	const app = new AppState();
	app.selectedBroker = 'existing:1883';
	app.brokerRepository['existing:1883'] = {
		topics: [],
		selectedTopic: null,
		pipeline: [],
		connected: true,
		backendTotalBytes: 0,
		bytesPerSecond: 0,
		backendTotalMessages: 0,
		messagesPerSecond: 0,
		rateHistory: []
	};

	const meta: MQTTMessageMetaParam = {
		source: 'other:1883',
		topic: 'test',
		timestamp: 'ts',
		payload_size: 10,
		total_bytes: 10,
		topic_message_count: 1
	};

	processMQTTMessageMeta(meta, app);
	expect(app.selectedBroker).toBe('existing:1883');
});

test('processMQTTMessageMeta updates backendTotalBytes', () => {
	const app = new AppState();
	const meta: MQTTMessageMetaParam = {
		source: 'b:1883',
		topic: 't',
		timestamp: 'ts',
		payload_size: 500,
		total_bytes: 12345,
		topic_message_count: 1
	};

	processMQTTMessageMeta(meta, app);
	expect(app.brokerRepository['b:1883'].backendTotalBytes).toBe(12345);
});

// ─── processMessagesEvicted ──────────────────────────────────────────

function makeBrokerWithTopicTree(): AppState {
	const app = new AppState();
	app.brokerRepository['b:1883'] = {
		topics: [
			{
				id: 'a',
				text: 'a (10 messages, 1 subtopic with 5 messages)',
				original_text: 'a',
				number_of_messages: 10,
				messages: [],
				children: [
					{
						id: 'a/b',
						text: 'b (5 messages)',
						original_text: 'b',
						number_of_messages: 5,
						messages: [
							{ timestamp: 'ts1', text: 'msg1' },
							{ timestamp: 'ts2', text: 'msg2' },
							{ timestamp: 'ts3', text: 'msg3' },
							{ timestamp: 'ts4', text: 'msg4' },
							{ timestamp: 'ts5', text: 'msg5' }
						]
					}
				]
			}
		],
		selectedTopic: null,
		pipeline: [],
		connected: true,
		backendTotalBytes: 1000,
		bytesPerSecond: 0,
		backendTotalMessages: 0,
		messagesPerSecond: 0,
		rateHistory: []
	};
	return app;
}

test('processMessagesEvicted decrements topic tree counts', () => {
	const app = makeBrokerWithTopicTree();
	const params: MessagesEvictedParam = {
		source: 'b:1883',
		topic: 'a/b',
		count: 2,
		topic_message_count: 3
	};

	processMessagesEvicted(params, app);

	const root = app.brokerRepository['b:1883'].topics[0];
	expect(root.number_of_messages).toBe(8); // 10 - 2
	const child = root.children![0];
	expect(child.number_of_messages).toBe(3); // 5 - 2
});

test('processMessagesEvicted removes oldest messages from selected topic', () => {
	const app = makeBrokerWithTopicTree();
	const childBranch = app.brokerRepository['b:1883'].topics[0].children![0];
	app.brokerRepository['b:1883'].selectedTopic = childBranch;

	const params: MessagesEvictedParam = {
		source: 'b:1883',
		topic: 'a/b',
		count: 2,
		topic_message_count: 3
	};

	processMessagesEvicted(params, app);

	// Should remove 2 oldest (from the end since they're newest-first)
	expect(childBranch.messages).toHaveLength(3);
	expect(childBranch.messages[0].text).toBe('msg1');
	expect(childBranch.messages[2].text).toBe('msg3');
});

test('processMessagesEvicted handles non-existing broker', () => {
	const app = new AppState();
	const params: MessagesEvictedParam = {
		source: 'missing:1883',
		topic: 'a/b',
		count: 5,
		topic_message_count: 0
	};

	// Should not throw
	const result = processMessagesEvicted(params, app);
	expect(result).toBe(app);
});

test('processMessagesEvicted handles non-existing topic in tree', () => {
	const app = makeBrokerWithTopicTree();
	const params: MessagesEvictedParam = {
		source: 'b:1883',
		topic: 'nonexistent/topic',
		count: 3,
		topic_message_count: 0
	};

	// Should not throw, tree unchanged
	processMessagesEvicted(params, app);
	expect(app.brokerRepository['b:1883'].topics[0].number_of_messages).toBe(10);
});

test('processMessagesEvicted removes leaf node when count reaches 0', () => {
	const app = makeBrokerWithTopicTree();
	const params: MessagesEvictedParam = {
		source: 'b:1883',
		topic: 'a/b',
		count: 5,
		topic_message_count: 0
	};

	processMessagesEvicted(params, app);

	const root = app.brokerRepository['b:1883'].topics[0];
	expect(root.number_of_messages).toBe(5); // 10 - 5
	// "b" had 5 messages, all evicted → should be removed
	expect(root.children).toHaveLength(0);
});

// ─── processMQTTMessageMetaBatch / processMessagesEvictedBatch ───────

test('processMQTTMessageMetaBatch processes all items', () => {
	const app = new AppState();
	const metas: MQTTMessageMetaParam[] = [
		{ source: 'b:1883', topic: 't1', timestamp: 'ts1', payload_size: 10, total_bytes: 10, topic_message_count: 1 },
		{ source: 'b:1883', topic: 't2', timestamp: 'ts2', payload_size: 20, total_bytes: 30, topic_message_count: 1 },
		{ source: 'b:1883', topic: 't3', timestamp: 'ts3', payload_size: 30, total_bytes: 60, topic_message_count: 1 }
	];

	processMQTTMessageMetaBatch(metas, app);

	expect(app.brokerRepository['b:1883'].topics).toHaveLength(3);
	expect(app.brokerRepository['b:1883'].backendTotalBytes).toBe(60);
});

test('processMessagesEvictedBatch processes all items', () => {
	const app = makeBrokerWithTopicTree();
	const items: MessagesEvictedParam[] = [
		{ source: 'b:1883', topic: 'a/b', count: 1, topic_message_count: 4 },
		{ source: 'b:1883', topic: 'a/b', count: 1, topic_message_count: 3 }
	];

	processMessagesEvictedBatch(items, app);

	const root = app.brokerRepository['b:1883'].topics[0];
	expect(root.number_of_messages).toBe(8); // 10 - 1 - 1
});

// ─── processTopicMessagesClear ───────────────────────────────────────

test('processTopicMessagesClear clears messages from selected topic', () => {
	const app = makeBrokerWithTopicTree();
	app.selectedBroker = 'b:1883';
	const childBranch = app.brokerRepository['b:1883'].topics[0].children![0];
	app.brokerRepository['b:1883'].selectedTopic = childBranch;

	processTopicMessagesClear(app);

	expect(childBranch.messages).toHaveLength(0);
});

test('processTopicMessagesClear is noop without selected broker', () => {
	const app = new AppState();
	// Should not throw
	processTopicMessagesClear(app);
});

test('processTopicMessagesClear is noop without selected topic', () => {
	const app = new AppState();
	app.selectedBroker = 'b:1883';
	app.brokerRepository['b:1883'] = {
		topics: [],
		selectedTopic: null,
		pipeline: [],
		connected: true,
		backendTotalBytes: 0,
		bytesPerSecond: 0,
		backendTotalMessages: 0,
		messagesPerSecond: 0,
		rateHistory: []
	};

	processTopicMessagesClear(app);
	// No throw
});

// ─── processPipelines ────────────────────────────────────────────────

test('processPipelines transforms pipeline params', () => {
	const params: PipelineParam[] = [
		{ id: '1', name: 'pipe1', pipeline: [{ topic: 't1' }, { topic: 't2' }] },
		{ id: '2', name: 'pipe2', pipeline: [{ topic: 't3' }] }
	];

	const result = processPipelines(params);

	expect(result).toHaveLength(2);
	expect(result[0].text).toBe('pipe1');
	expect(result[0].pipeline).toHaveLength(2);
	expect(result[1].text).toBe('pipe2');
});

test('processPipelines handles empty array', () => {
	const result = processPipelines([]);
	expect(result).toEqual([]);
});

// ─── processMQTTMessage edge cases ───────────────────────────────────

test('processMQTTMessage does nothing for non-existing broker', () => {
	const app = new AppState();
	const message: MQTTMessageParam = {
		source: 'missing:1883',
		topic: 'test',
		payload: new TextEncoder().encode('data').buffer,
		timestamp: 'ts'
	};

	const result = processMQTTMessage(message, app);
	expect(result).toBe(app);
});

test('processMQTTMessage does nothing for non-existing topic in tree', () => {
	const app = new AppState();
	app.brokerRepository['b:1883'] = {
		topics: [
			{
				id: 'other',
				text: 'other',
				original_text: 'other',
				number_of_messages: 1,
				messages: [],
				children: undefined
			}
		],
		selectedTopic: null,
		pipeline: [],
		connected: true,
		backendTotalBytes: 0,
		bytesPerSecond: 0,
		backendTotalMessages: 0,
		messagesPerSecond: 0,
		rateHistory: []
	};

	const message: MQTTMessageParam = {
		source: 'b:1883',
		topic: 'nonexistent',
		payload: new TextEncoder().encode('data').buffer,
		timestamp: 'ts'
	};

	processMQTTMessage(message, app);
	// "other" topic should be untouched
	expect(app.brokerRepository['b:1883'].topics[0].messages).toHaveLength(0);
});

test('processMQTTMessage inserts messages newest-first', () => {
	const app = new AppState();
	app.brokerRepository['b:1883'] = {
		topics: [
			{
				id: 'test',
				text: 'test (2 messages)',
				original_text: 'test',
				number_of_messages: 2,
				messages: [],
				children: undefined
			}
		],
		selectedTopic: null,
		pipeline: [],
		connected: true,
		backendTotalBytes: 0,
		bytesPerSecond: 0,
		backendTotalMessages: 0,
		messagesPerSecond: 0,
		rateHistory: []
	};

	const msg1: MQTTMessageParam = {
		source: 'b:1883',
		topic: 'test',
		payload: new TextEncoder().encode('first').buffer,
		timestamp: '2024-01-01T00:00:00.000Z'
	};
	const msg2: MQTTMessageParam = {
		source: 'b:1883',
		topic: 'test',
		payload: new TextEncoder().encode('second').buffer,
		timestamp: '2024-01-01T00:00:05.000Z'
	};

	processMQTTMessage(msg1, app);
	processMQTTMessage(msg2, app);

	const messages = app.brokerRepository['b:1883'].topics[0].messages;
	expect(messages).toHaveLength(2);
	// Newest first
	expect(messages[0].text).toBe('second');
	expect(messages[1].text).toBe('first');
});

// ─── processSettings edge cases ──────────────────────────────────────

test('processSettings stores max_broker_bytes correctly', () => {
	const app = new AppState();
	const result = processSettings({ max_broker_bytes: 512 * 1024 * 1024, max_message_size: 1024 * 1024 }, app);
	expect(result.maxBrokerBytes).toBe(512 * 1024 * 1024);
});

// ─── processBrokers edge cases ───────────────────────────────────────

test('processBrokers updates existing broker status', () => {
	const br: BrokerRepository = {
		'b:1883': {
			topics: [
				{
					id: 'existing',
					text: 'existing',
					original_text: 'existing',
					number_of_messages: 5,
					messages: [],
					children: undefined
				}
			],
			selectedTopic: null,
			pipeline: [],
			connected: false,
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			backendTotalMessages: 0,
			messagesPerSecond: 0,
			rateHistory: []
		}
	};
	const params: BrokerParam = [
		{ broker: 'b:1883', connected: true, topics: {}, total_bytes: 999 }
	];

	const result = processBrokers(params, br);

	expect(result['b:1883'].connected).toBe(true);
	expect(result['b:1883'].backendTotalBytes).toBe(999);
	// Topics should be preserved (not overwritten)
	expect(result['b:1883'].topics).toHaveLength(1);
	expect(result['b:1883'].topics[0].original_text).toBe('existing');
});
