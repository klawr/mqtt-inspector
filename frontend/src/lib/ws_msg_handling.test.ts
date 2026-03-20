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
	type MQTTMessageParam,
	processSettings,
	processSyncComplete
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
			totalBytes: 0,
			backendTotalBytes: 0
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
			totalBytes: 0,
			backendTotalBytes: 0
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
			totalBytes: 0,
			backendTotalBytes: 0
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
			totalBytes: 0,
			backendTotalBytes: 0
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
			totalBytes: 0,
			backendTotalBytes: 0
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
			topics: {
				topic1: [{ payload: new Uint8Array([72, 101, 108, 108, 111]), timestamp: '2022-01-01' }],
				topic2: [{ payload: new Uint8Array([87, 111, 114, 108, 100]), timestamp: '2022-01-02' }]
			}
		},
		{
			broker: 'broker2',
			connected: false,
			topics: {
				topic3: [{ payload: new Uint8Array([72, 105]), timestamp: '2022-01-03' }],
				topic4: [{ payload: new Uint8Array([70, 114, 111, 109]), timestamp: '2022-01-04' }]
			}
		}
	];

	const result = processBrokers(params, decoder as unknown as TextDecoder, brokerRepository);

	const expectedBroker1Topics = [
		{
			id: 'topic1',
			text: 'topic1 (1 message)',
			children: undefined,
			number_of_messages: 1,
			original_text: 'topic1',
			messages: [{ delta_t: 0, text: 'Hello', timestamp: '2022-01-01' }]
		},
		{
			id: 'topic2',
			text: 'topic2 (1 message)',
			children: undefined,
			number_of_messages: 1,
			original_text: 'topic2',
			messages: [{ delta_t: 0, text: 'World', timestamp: '2022-01-02' }]
		}
	];

	const expectedBroker2Topics = [
		{
			id: 'topic3',
			text: 'topic3 (1 message)',
			children: undefined,
			number_of_messages: 1,
			original_text: 'topic3',
			messages: [{ delta_t: 0, text: 'Hi', timestamp: '2022-01-03' }]
		},
		{
			id: 'topic4',
			text: 'topic4 (1 message)',
			children: undefined,
			number_of_messages: 1,
			original_text: 'topic4',
			messages: [{ delta_t: 0, text: 'From', timestamp: '2022-01-04' }]
		}
	];

	expect(result).toEqual({
		broker1: {
			topics: expectedBroker1Topics,
			selectedTopic: null,
			pipeline: [],
			connected: true,
			totalBytes: 10,
			backendTotalBytes: 0
		},
		broker2: {
			topics: expectedBroker2Topics,
			selectedTopic: null,
			pipeline: [],
			connected: false,
			totalBytes: 6,
			backendTotalBytes: 0
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

test('processMQTTMessage evicts oldest messages when byte budget exceeded', () => {
	const decoder = new MockTextDecoder();
	const app = new AppState();

	// Send many messages to push past the 64 MB byte budget
	// Each message payload is 1 MB of text
	const oneMB = 'x'.repeat(1024 * 1024);
	for (let i = 0; i < 70; i++) {
		const timestamp = new Date(2022, 0, 1, 0, 0, i).toISOString();
		const message: MQTTMessageParam = {
			source: 'broker1',
			topic: `topic${i % 3}`,
			payload: new TextEncoder().encode(oneMB),
			timestamp
		};
		processMQTTMessage(message, decoder as unknown as TextDecoder, app);
	}

	// totalBytes should be at or under 64 MB
	expect(app.brokerRepository['broker1'].totalBytes).toBeLessThanOrEqual(64 * 1024 * 1024);

	// Should have evicted some messages — fewer than 70 total
	let totalMessages = 0;
	function countMessages(branches: typeof app.brokerRepository['broker1']['topics']) {
		for (const branch of branches) {
			totalMessages += branch.messages.length;
			if (branch.children) countMessages(branch.children);
		}
	}
	countMessages(app.brokerRepository['broker1'].topics);
	expect(totalMessages).toBeLessThan(70);
	expect(totalMessages).toBeGreaterThan(0);
});

test('processSettings updates maxBrokerBytes on AppState', () => {
	const app = new AppState();
	const result = processSettings({ max_broker_bytes: 256 * 1024 * 1024, max_message_size: 2 * 1024 * 1024 }, app);
	expect(result.maxBrokerBytes).toBe(256 * 1024 * 1024);
});

test('processSyncComplete sets syncComplete flag', () => {
	const app = new AppState();
	expect(app.syncComplete).toBe(false);
	const result = processSyncComplete(app);
	expect(result.syncComplete).toBe(true);
});
