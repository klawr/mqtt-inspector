import { test, expect } from 'vitest';
import { prettyPrint } from './messages';

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
