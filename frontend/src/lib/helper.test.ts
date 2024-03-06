import { test, expect } from 'vitest';
import { findbranchwithid } from './helper';
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
