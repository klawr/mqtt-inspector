import { test, expect } from 'vitest';
import {
    processConfigs,
    type CommandParam,
    processConnectionStatus,
    type MqttConnectionStatus,
    processBrokerRemoval,
    type BrokerParam,
    processBrokers
} from './ws_msg_handling';
import { AppState, type BrokerRepository } from './state';

test('processConfigs processes commands correctly', () => {
    const commands: CommandParam[] = [
        { id: '1', name: 'Command1', topic: 'topic1', payload: 'payload1' },
        { id: '2', name: 'Command2', topic: 'topic2', payload: 'payload2' },
        { id: '3', name: 'Command3', topic: 'topic3', payload: 'payload3' },
    ];

    const expectedResult = [
        { id: '0', text: 'Command1', topic: 'topic1', payload: 'payload1' },
        { id: '1', text: 'Command2', topic: 'topic2', payload: 'payload2' },
        { id: '2', text: 'Command3', topic: 'topic3', payload: 'payload3' },
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
    const commands: CommandParam[] = [{ id: '1', name: 'Command1', topic: 'topic1', payload: 'payload1' }];
    const result = processConfigs(commands);

    expect(result).toEqual([{ id: '0', text: 'Command1', topic: 'topic1', payload: 'payload1' }]);
});

test('processBrokerRemoval marks broker for deletion in AppState', () => {
    const appState = new AppState();
    appState.brokerRepository = {
        broker1: {
            markedForDeletion: false,
            topics: [],
            selectedTopic: null,
            pipeline: [],
            connected: false
        }
    };

    const result = processBrokerRemoval('broker1', appState);

    expect(result.brokerRepository['broker1'].markedForDeletion).toBe(true);
});

test('processBrokerRemoval handles non-existing broker correctly', () => {
    const appState = new AppState();
    appState.brokerRepository = {
        broker1: {
            markedForDeletion: false,
            topics: [],
            selectedTopic: null,
            pipeline: [],
            connected: false
        }
    };

    const result = processBrokerRemoval('nonexistentBroker', appState);

    expect(result.brokerRepository['broker1'].markedForDeletion).toBeFalsy();
});

test('processConnectionStatus updates connection status in AppState', () => {
    const appState = new AppState();
    appState.brokerRepository = {
        broker1: {
            connected: false, markedForDeletion: false,
            topics: [],
            selectedTopic: null,
            pipeline: []
        }
    };

    const connectionStatus: MqttConnectionStatus = { source: 'broker1', connected: true };
    const result = processConnectionStatus(connectionStatus, appState);

    expect(result.brokerRepository['broker1'].connected).toBe(true);
    expect(result.brokerRepository['broker1'].markedForDeletion).toBe(false);
});

test('processConnectionStatus handles non-existing broker correctly', () => {
    const appState = new AppState();
    appState.brokerRepository = {
        broker1: {
            connected: false, markedForDeletion: false,
            topics: [],
            selectedTopic: null,
            pipeline: []
        }
    };

    const connectionStatus: MqttConnectionStatus = { source: 'nonexistentBroker', connected: true };
    const result = processConnectionStatus(connectionStatus, appState);

    expect(result.brokerRepository['broker1'].connected).toBe(false);
    expect(result.brokerRepository['broker1'].markedForDeletion).toBe(false);
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
                'topic1': [{ payload: new Uint8Array([72, 101, 108, 108, 111]), timestamp: '2022-01-01' }],
                'topic2': [{ payload: new Uint8Array([87, 111, 114, 108, 100]), timestamp: '2022-01-02' }]
            }
        },
        {
            broker: 'broker2',
            connected: false,
            topics: {
                'topic3': [{ payload: new Uint8Array([72, 105]), timestamp: '2022-01-03' }],
                'topic4': [{ payload: new Uint8Array([70, 114, 111, 109]), timestamp: '2022-01-04' }]
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
            messages: [
                { delta_t: 0, text: 'Hello', timestamp: '2022-01-01' }
            ]
        },
        {
            id: 'topic2',
            text: 'topic2 (1 message)',
            children: undefined,
            number_of_messages: 1,
            original_text: 'topic2',
            messages: [
                { delta_t: 0, text: 'World', timestamp: '2022-01-02' }
            ]
        }
    ];

    const expectedBroker2Topics = [
        {
            id: 'topic3',
            text: 'topic3 (1 message)',
            children: undefined,
            number_of_messages: 1,
            original_text: 'topic3',
            messages: [
                { delta_t: 0, text: 'Hi', timestamp: '2022-01-03' }
            ]
        },
        {
            id: 'topic4',
            text: 'topic4 (1 message)',
            children: undefined,
            number_of_messages: 1,
            original_text: 'topic4',
            messages: [
                { delta_t: 0, text: 'From', timestamp: '2022-01-04' }
            ]
        }
    ];

    expect(result).toEqual({
        'broker1': {
            topics: expectedBroker1Topics,
            selectedTopic: null,
            pipeline: [],
            connected: true,
            markedForDeletion: false
        },
        'broker2': {
            topics: expectedBroker2Topics,
            selectedTopic: null,
            pipeline: [],
            connected: false,
            markedForDeletion: false
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