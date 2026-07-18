import { test, expect } from 'vitest';
import { openTab, pinTab, activateTab, closeTab } from './tabs';
import type { BrokerRepositoryEntry, Treebranch } from './state';

function leaf(id: string): Treebranch {
	const text = id.split('/').pop() ?? id;
	return { id, text, original_text: text, number_of_messages: 0, messages: [] };
}

function makeEntry(...ids: string[]): BrokerRepositoryEntry {
	return {
		topics: ids.map(leaf),
		selectedTopic: null,
		openTabs: [],
		pipeline: [],
		connected: true,
		backendTotalBytes: 0,
		backendTotalMessages: 0,
		bytesPerSecond: 0,
		messagesPerSecond: 0,
		rateHistory: [],
		requiresAuth: false,
		authenticated: false
	};
}

test('openTab as preview activates and marks italic', () => {
	const entry = makeEntry('a', 'b');
	openTab(entry, 'a', { pin: false });
	expect(entry.openTabs).toEqual([{ id: 'a', preview: true }]);
	expect(entry.selectedTopic?.id).toBe('a');
});

test('selecting another topic reuses the single preview slot', () => {
	const entry = makeEntry('a', 'b');
	openTab(entry, 'a', { pin: false });
	openTab(entry, 'b', { pin: false });
	expect(entry.openTabs).toEqual([{ id: 'b', preview: true }]);
	expect(entry.selectedTopic?.id).toBe('b');
});

test('pinned tab is kept when opening a new preview', () => {
	const entry = makeEntry('a', 'b', 'c');
	openTab(entry, 'a', { pin: false });
	pinTab(entry, 'a');
	openTab(entry, 'b', { pin: false });
	openTab(entry, 'c', { pin: false });
	expect(entry.openTabs).toEqual([
		{ id: 'a', preview: false },
		{ id: 'c', preview: true }
	]);
	expect(entry.selectedTopic?.id).toBe('c');
});

test('opening an already-open tab just activates it without changing preview', () => {
	const entry = makeEntry('a', 'b');
	openTab(entry, 'a', { pin: false });
	pinTab(entry, 'a');
	openTab(entry, 'b', { pin: false });
	openTab(entry, 'a', { pin: false });
	expect(entry.openTabs).toEqual([
		{ id: 'a', preview: false },
		{ id: 'b', preview: true }
	]);
	expect(entry.selectedTopic?.id).toBe('a');
});

test('openTab with pin opens a pinned tab directly', () => {
	const entry = makeEntry('a');
	openTab(entry, 'a', { pin: true });
	expect(entry.openTabs).toEqual([{ id: 'a', preview: false }]);
});

test('openTab with pin on an existing preview pins it in place', () => {
	const entry = makeEntry('a');
	openTab(entry, 'a', { pin: false });
	openTab(entry, 'a', { pin: true });
	expect(entry.openTabs).toEqual([{ id: 'a', preview: false }]);
});

test('pinTab turns off preview', () => {
	const entry = makeEntry('a');
	openTab(entry, 'a', { pin: false });
	pinTab(entry, 'a');
	expect(entry.openTabs[0].preview).toBe(false);
});

test('activateTab only activates open tabs', () => {
	const entry = makeEntry('a', 'b');
	openTab(entry, 'a', { pin: true });
	activateTab(entry, 'b');
	expect(entry.selectedTopic?.id).toBe('a');
	activateTab(entry, 'a');
	expect(entry.selectedTopic?.id).toBe('a');
});

test('closeTab activates the right neighbour', () => {
	const entry = makeEntry('a', 'b', 'c');
	openTab(entry, 'a', { pin: true });
	openTab(entry, 'b', { pin: true });
	openTab(entry, 'c', { pin: true });
	activateTab(entry, 'b');
	closeTab(entry, 'b');
	expect(entry.openTabs.map((t) => t.id)).toEqual(['a', 'c']);
	expect(entry.selectedTopic?.id).toBe('c');
});

test('closeTab falls back to the left neighbour when closing the last tab', () => {
	const entry = makeEntry('a', 'b');
	openTab(entry, 'a', { pin: true });
	openTab(entry, 'b', { pin: true });
	activateTab(entry, 'b');
	closeTab(entry, 'b');
	expect(entry.selectedTopic?.id).toBe('a');
});

test('closing the only tab clears the selection', () => {
	const entry = makeEntry('a');
	openTab(entry, 'a', { pin: true });
	closeTab(entry, 'a');
	expect(entry.openTabs).toEqual([]);
	expect(entry.selectedTopic).toBeNull();
});

test('closeTab frees the closed leaf cached messages', () => {
	const entry = makeEntry('a', 'b');
	openTab(entry, 'a', { pin: true });
	// Simulate a populated cache for topic "a".
	const leafA = entry.topics.find((t) => t.id === 'a')!;
	leafA.messages = [{ timestamp: 't', text: 'x' } as unknown as (typeof leafA.messages)[number]];
	closeTab(entry, 'a');
	expect(leafA.messages).toEqual([]);
});

test('closing an inactive tab keeps the active selection', () => {
	const entry = makeEntry('a', 'b');
	openTab(entry, 'a', { pin: true });
	openTab(entry, 'b', { pin: true });
	activateTab(entry, 'b');
	closeTab(entry, 'a');
	expect(entry.selectedTopic?.id).toBe('b');
});
