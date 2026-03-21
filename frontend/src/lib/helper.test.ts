import { test, expect } from 'vitest';
import { findbranchwithid, formatDuration } from './helper';
import { type Treebranch } from './state';

const tree: Treebranch[] = [
	{
		id: '1',
		text: 'Root',
		original_text: 'Root',
		number_of_messages: 2,
		messages: [
			{ timestamp: '2022-01-01', text: 'Hello' },
			{ timestamp: '2022-01-02', text: 'World' }
		],
		children: [
			{
				id: '2',
				text: 'Child 1',
				original_text: 'Child 1',
				number_of_messages: 1,
				messages: [{ timestamp: '2022-01-03', text: 'Child message' }]
			},
			{
				id: '3',
				text: 'Child 2',
				original_text: 'Child 2',
				number_of_messages: 0,
				messages: []
			}
		]
	}
];

test('findbranchwithid finds existing branch by ID', () => {
	const result = findbranchwithid('2', tree);
	expect(result).toBeDefined();
	expect(result?.id).toBe('2');
});

test('findbranchwithid returns undefined for non-existing branch', () => {
	const result = findbranchwithid('nonexistent', tree);
	expect(result).toBeUndefined();
});

test('findbranchwithid finds branch with children', () => {
	const result = findbranchwithid('1', tree);
	expect(result).toBeDefined();
	expect(result?.id).toBe('1');
	expect(result?.children).toBeDefined();
	expect(result?.children?.length).toBeGreaterThan(0);
});

test('findbranchwithid finds branch with messages', () => {
	const result = findbranchwithid('1', tree);
	expect(result).toBeDefined();
	expect(result?.id).toBe('1');
	expect(result?.messages).toBeDefined();
	expect(result?.messages.length).toBeGreaterThan(0);
});

test('formatDuration shows milliseconds up to 9999', () => {
	expect(formatDuration(0)).toBe('0 ms');
	expect(formatDuration(500)).toBe('500 ms');
	expect(formatDuration(9999)).toBe('9999 ms');
});

test('formatDuration shows seconds from 10000ms up to 599s', () => {
	expect(formatDuration(10000)).toBe('10.0 s');
	expect(formatDuration(30000)).toBe('30.0 s');
	expect(formatDuration(599000)).toBe('599.0 s');
});

test('formatDuration shows minutes from 600s up to 720min', () => {
	expect(formatDuration(600 * 1000)).toBe('10.0 min');
	expect(formatDuration(720 * 60 * 1000)).toBe('720.0 min');
});

test('formatDuration shows hours from 720min up to 48h', () => {
	expect(formatDuration(721 * 60 * 1000)).toBe('12.0 h');
	expect(formatDuration(48 * 60 * 60 * 1000)).toBe('48.0 h');
});

test('formatDuration shows days beyond 48h', () => {
	expect(formatDuration(49 * 60 * 60 * 1000)).toBe('2.0 d');
	expect(formatDuration(7 * 24 * 60 * 60 * 1000)).toBe('7.0 d');
});
