import { test, expect } from 'vitest';
import { getTopicSwitchResetState, prettyPrint } from './messages';

test('prettyPrint should return the same input if it is not a valid JSON string', () => {
	const input = 'invalid JSON string';
	expect(prettyPrint(input)).toBe(input);
});

test('prettyPrint - should pretty print a valid JSON string', () => {
	const input = '{"foo":"bar","nested":{"baz":"qux"}}';
	const expectedOutput = `{
  "foo": "bar",
  "nested": {
    "baz": "qux"
  }
}`;
	expect(prettyPrint(input)).toBe(expectedOutput);
});

test('prettyPrint should handle deeply nested JSON strings', () => {
	const input = '{"foo":"bar","nested":{"baz":"qux","deeply":{"nested":"value"}}}';
	const expectedOutput = `{
  "foo": "bar",
  "nested": {
    "baz": "qux",
    "deeply": {
      "nested": "value"
    }
  }
}`;
	expect(prettyPrint(input)).toBe(expectedOutput);
});

test('prettyPrint should handle arrays in JSON strings', () => {
	const input = '{"foo":["bar","baz"]}';
	const expectedOutput = `{
  "foo": [
    "bar",
    "baz"
  ]
}`;
	expect(prettyPrint(input)).toBe(expectedOutput);
});

test('prettyPrint should handle empty JSON strings', () => {
	const input = '{}';
	const expectedOutput = `{}`;
	expect(prettyPrint(input)).toBe(expectedOutput);
});

test('getTopicSwitchResetState should reset to latest message selection defaults', () => {
  const state = getTopicSwitchResetState(5);

  expect(state).toEqual({
    selectedIndex: 0,
    selectedIndexCompare: 1,
    lockedIndex: false,
    lockedIndexCompare: false,
    compareMessage: false,
    prevMessageCount: 5
  });
});

test('getTopicSwitchResetState should handle empty topics', () => {
  const state = getTopicSwitchResetState(0);

  expect(state.prevMessageCount).toBe(0);
  expect(state.selectedIndex).toBe(0);
  expect(state.lockedIndex).toBe(false);
});
