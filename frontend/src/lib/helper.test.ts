import { test, expect } from 'vitest';
import { findbranchwithid, formatDuration, getAllTopics, shouldFilterItem } from './helper';
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

// ─── getAllTopics ─────────────────────────────────────────────────────

test('getAllTopics flattens tree to array', () => {
	const result = getAllTopics(tree);
	// tree has: Root, Child 1, Child 2 = 3 nodes
	expect(result).toHaveLength(3);
	expect(result.map((t) => t.id)).toEqual(['1', '2', '3']);
});

test('getAllTopics returns empty for empty tree', () => {
	const result = getAllTopics([]);
	expect(result).toHaveLength(0);
});

test('getAllTopics handles deeply nested tree', () => {
	const deep: Treebranch[] = [
		{
			id: 'a',
			text: 'a',
			original_text: 'a',
			number_of_messages: 1,
			messages: [],
			children: [
				{
					id: 'a/b',
					text: 'b',
					original_text: 'b',
					number_of_messages: 1,
					messages: [],
					children: [
						{
							id: 'a/b/c',
							text: 'c',
							original_text: 'c',
							number_of_messages: 1,
							messages: [],
							children: [
								{
									id: 'a/b/c/d',
									text: 'd',
									original_text: 'd',
									number_of_messages: 1,
									messages: []
								}
							]
						}
					]
				}
			]
		}
	];

	const result = getAllTopics(deep);
	expect(result).toHaveLength(4);
	expect(result.map((t) => t.id)).toEqual(['a', 'a/b', 'a/b/c', 'a/b/c/d']);
});

test('getAllTopics handles nodes without children', () => {
	const flat: Treebranch[] = [
		{ id: 'x', text: 'x', original_text: 'x', number_of_messages: 0, messages: [] },
		{ id: 'y', text: 'y', original_text: 'y', number_of_messages: 0, messages: [] }
	];

	const result = getAllTopics(flat);
	expect(result).toHaveLength(2);
});

// ─── shouldFilterItem ────────────────────────────────────────────────

test('shouldFilterItem returns true for empty filter value', () => {
	expect(shouldFilterItem({ text: 'anything' }, '')).toBe(true);
});

test('shouldFilterItem matches case-insensitively for lowercase input', () => {
	expect(shouldFilterItem({ text: 'Hello WORLD' }, 'hello')).toBe(true);
	expect(shouldFilterItem({ text: 'Hello WORLD' }, 'world')).toBe(true);
});

test('shouldFilterItem does not match uppercase filter value', () => {
	// value is not lowercased by the implementation
	expect(shouldFilterItem({ text: 'Hello WORLD' }, 'HELLO')).toBe(false);
});

test('shouldFilterItem matches partial text', () => {
	expect(shouldFilterItem({ text: 'test/topic/data' }, 'topic')).toBe(true);
});

test('shouldFilterItem returns false for non-matching text', () => {
	expect(shouldFilterItem({ text: 'sensor/temperature' }, 'humidity')).toBe(false);
});

test('shouldFilterItem handles special characters in text', () => {
	expect(shouldFilterItem({ text: '$SYS/broker/load' }, '$sys')).toBe(true);
	expect(shouldFilterItem({ text: 'topic.with.dots' }, '.with.')).toBe(true);
});

// ─── findbranchwithid edge cases ─────────────────────────────────────

test('findbranchwithid returns undefined for empty tree', () => {
	expect(findbranchwithid('1', [])).toBeUndefined();
});

test('findbranchwithid returns undefined for undefined tree', () => {
	expect(findbranchwithid('1', undefined)).toBeUndefined();
});
