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
	processSyncComplete,
	processRateHistorySample,
	type RateHistorySampleParam
} from './ws_msg_handling';
import { AppState, type BrokerRepository } from './state';

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

class MockTextDecoder {
	decode(input?: ArrayBufferView | ArrayBuffer, options?: TextDecodeOptions): string {
		if (input) {
			return new TextDecoder().decode(input, options);
		}
		return '';
	}
}
test('processBrokers updates BrokerRepository correctly', () => {
	const brokerRepository: BrokerRepository = {};
	const decoder = new MockTextDecoder();

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

	const result = processBrokers(params, decoder as unknown as TextDecoder, brokerRepository);

	expect(result).toEqual({
		broker1: {
			topics: [],
			selectedTopic: null,
			pipeline: [],
			connected: true,
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			rateHistory: []
		},
		broker2: {
			topics: [],
			selectedTopic: null,
			pipeline: [],
			connected: false,
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			rateHistory: []
		}
	});
});

test('processBrokers handles empty params correctly', () => {
	const brokerRepository: BrokerRepository = {};
	const decoder = new MockTextDecoder();

	const params: BrokerParam = [];

	const result = processBrokers(params, decoder as unknown as TextDecoder, brokerRepository);

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
	const decoder = new MockTextDecoder();
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
		rateHistory: []
	};
	const message: MQTTMessageParam = {
		source: 'broker1',
		topic: 'topic1',
		payload: new TextEncoder().encode('Hello from binary').buffer,
		timestamp: '2022-01-01T00:00:00.000Z',
		total_bytes: 17
	};

	processMQTTMessage(message, decoder as unknown as TextDecoder, app);

	expect(app.brokerRepository['broker1'].topics[0].messages[0].text).toBe('Hello from binary');
});

test('processMQTTMessages applies batched messages in order', () => {
	const decoder = new MockTextDecoder();
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

	processMQTTMessages(messages, decoder as unknown as TextDecoder, app);

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

test('processSyncComplete sets syncComplete flag', () => {
	const app = new AppState();
	expect(app.syncComplete).toBe(false);
	const result = processSyncComplete(app);
	expect(result.syncComplete).toBe(true);
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
	const decoder = new MockTextDecoder();

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

	const result = processBrokers(params, decoder as unknown as TextDecoder, brokerRepository);

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
