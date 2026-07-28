import { test, expect } from 'vitest';
import {
	activateTab,
	allGroups,
	closeGroup,
	closeTab,
	focusedGroup,
	focusGroup,
	moveTab,
	moveTabToNewSplit,
	openInFocusedGroup,
	reorderTab,
	splitGroup,
	subscribedTopicIds,
	serializeLayout,
	deserializeLayout,
	newLayout
} from './layout';
import type { BrokerRepositoryEntry, LayoutNode, Treebranch } from './state';

function leaf(id: string): Treebranch {
	const text = id.split('/').pop() ?? id;
	return { id, text, original_text: text, number_of_messages: 0, messages: [] };
}

function makeEntry(...ids: string[]): BrokerRepositoryEntry {
	return {
		topics: ids.map(leaf),
		selectedTopic: null,
		...newLayout(),
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

function activeIds(entry: BrokerRepositoryEntry): (string | null)[] {
	return allGroups(entry.layout).map((g) => g.activeTopicId);
}

test('openInFocusedGroup opens a preview tab and syncs selectedTopic', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: false });
	expect(focusedGroup(entry).tabs).toEqual([{ id: 'a', preview: true }]);
	expect(entry.selectedTopic?.id).toBe('a');
});

test('subscribedTopicIds is the union of every group active topic', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: true });
	splitGroup(entry, entry.activeGroupId, 'right', 'b');
	expect([...subscribedTopicIds(entry)].sort()).toEqual(['a', 'b']);
});

test('splitGroup right creates a row split with the new group focused', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: true });
	const firstGroupId = entry.activeGroupId;
	splitGroup(entry, firstGroupId, 'right', 'b');

	expect(entry.layout.type).toBe('split');
	const split = entry.layout as Extract<LayoutNode, { type: 'split' }>;
	expect(split.direction).toBe('row');
	expect(split.children.length).toBe(2);
	expect(activeIds(entry)).toEqual(['a', 'b']);
	// New group (right) is focused and holds b pinned.
	expect(entry.activeGroupId).not.toBe(firstGroupId);
	expect(focusedGroup(entry).tabs).toEqual([{ id: 'b', preview: false }]);
});

test('splitGroup down creates a column split', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: true });
	splitGroup(entry, entry.activeGroupId, 'down', 'b');
	expect((entry.layout as Extract<LayoutNode, { type: 'split' }>).direction).toBe('column');
});

test('nested split: splitting a child in the other direction nests', () => {
	const entry = makeEntry('a', 'b', 'c');
	openInFocusedGroup(entry, 'a', { pin: true });
	splitGroup(entry, entry.activeGroupId, 'right', 'b'); // row [a, b]
	splitGroup(entry, entry.activeGroupId, 'down', 'c'); // b becomes column [b, c]
	const root = entry.layout as Extract<LayoutNode, { type: 'split' }>;
	expect(root.direction).toBe('row');
	expect(root.children.length).toBe(2);
	const second = root.children[1];
	expect(second.type).toBe('split');
	expect((second as Extract<LayoutNode, { type: 'split' }>).direction).toBe('column');
	expect(allGroups(entry.layout).length).toBe(3);
});

test('same-direction split merges into the existing split instead of nesting', () => {
	const entry = makeEntry('a', 'b', 'c');
	openInFocusedGroup(entry, 'a', { pin: true });
	const g1 = entry.activeGroupId;
	splitGroup(entry, g1, 'right', 'b'); // row [a, b]
	splitGroup(entry, g1, 'right', 'c'); // row [a, c, b] (no nesting)
	const root = entry.layout as Extract<LayoutNode, { type: 'split' }>;
	expect(root.direction).toBe('row');
	expect(root.children.every((c) => c.type === 'group')).toBe(true);
	expect(root.children.length).toBe(3);
});

test('moveTab moves a tab to another group and closes the emptied source (collapse)', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: true });
	const g1 = entry.activeGroupId;
	splitGroup(entry, g1, 'right', 'b');
	const g2 = entry.activeGroupId;

	moveTab(entry, g1, g2, 'a'); // g1 had only 'a' → becomes empty → closed → split collapses
	expect(entry.layout.type).toBe('group');
	expect(allGroups(entry.layout).length).toBe(1);
	expect(
		focusedGroup(entry)
			.tabs.map((t) => t.id)
			.sort()
	).toEqual(['a', 'b']);
});

test('reorderTab moves a tab before another tab in the same group', () => {
	const entry = makeEntry('a', 'b', 'c');
	openInFocusedGroup(entry, 'a', { pin: true });
	openInFocusedGroup(entry, 'b', { pin: true });
	openInFocusedGroup(entry, 'c', { pin: true });
	const g1 = entry.activeGroupId;

	reorderTab(entry, g1, 'c', 'a', false);
	expect(focusedGroup(entry).tabs.map((t) => t.id)).toEqual(['c', 'a', 'b']);
});

test('reorderTab moves a tab after another tab in the same group', () => {
	const entry = makeEntry('a', 'b', 'c');
	openInFocusedGroup(entry, 'a', { pin: true });
	openInFocusedGroup(entry, 'b', { pin: true });
	openInFocusedGroup(entry, 'c', { pin: true });
	const g1 = entry.activeGroupId;

	reorderTab(entry, g1, 'a', 'b', true);
	expect(focusedGroup(entry).tabs.map((t) => t.id)).toEqual(['b', 'a', 'c']);
});

test('reorderTab with a null target moves the tab to the end', () => {
	const entry = makeEntry('a', 'b', 'c');
	openInFocusedGroup(entry, 'a', { pin: true });
	openInFocusedGroup(entry, 'b', { pin: true });
	openInFocusedGroup(entry, 'c', { pin: true });
	const g1 = entry.activeGroupId;

	reorderTab(entry, g1, 'a', null);
	expect(focusedGroup(entry).tabs.map((t) => t.id)).toEqual(['b', 'c', 'a']);
});

test('reorderTab does not change the active tab or unrelated groups', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: true });
	openInFocusedGroup(entry, 'b', { pin: true });
	const g1 = entry.activeGroupId;
	activateTab(entry, g1, 'a');

	reorderTab(entry, g1, 'b', 'a', false);
	expect(focusedGroup(entry).activeTopicId).toBe('a');
	expect(focusedGroup(entry).tabs.map((t) => t.id)).toEqual(['b', 'a']);
});

test('reorderTab is a no-op for unknown group or tab ids', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: true });
	openInFocusedGroup(entry, 'b', { pin: true });
	const g1 = entry.activeGroupId;
	const before = focusedGroup(entry).tabs.map((t) => t.id);

	reorderTab(entry, 'missing-group', 'a', 'b', false);
	reorderTab(entry, g1, 'missing-tab', 'b', false);
	expect(focusedGroup(entry).tabs.map((t) => t.id)).toEqual(before);
});

test('moveTabToNewSplit removes from source and creates a new split', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: true });
	openInFocusedGroup(entry, 'b', { pin: true });
	const g1 = entry.activeGroupId;
	moveTabToNewSplit(entry, g1, 'right', g1, 'b');
	expect(allGroups(entry.layout).length).toBe(2);
	// b now lives alone in the new group; a stays in the original.
	const groups = allGroups(entry.layout);
	expect(groups.map((g) => g.tabs.map((t) => t.id))).toContainEqual(['b']);
	expect(groups.map((g) => g.tabs.map((t) => t.id))).toContainEqual(['a']);
});

test('closeGroup collapses the split back to a single group', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: true });
	const g1 = entry.activeGroupId;
	splitGroup(entry, g1, 'right', 'b');
	const g2 = entry.activeGroupId;
	closeGroup(entry, g2);
	expect(entry.layout.type).toBe('group');
	expect(focusedGroup(entry).tabs.map((t) => t.id)).toEqual(['a']);
	expect(entry.activeGroupId).toBe(g1);
});

test('closing the only group keeps one empty group', () => {
	const entry = makeEntry('a');
	openInFocusedGroup(entry, 'a', { pin: true });
	closeGroup(entry, entry.activeGroupId);
	expect(allGroups(entry.layout).length).toBe(1);
	expect(focusedGroup(entry).tabs).toEqual([]);
	expect(entry.selectedTopic).toBeNull();
});

test('closeTab frees the leaf cache when no group still shows the topic', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: true });
	const leafA = entry.topics.find((t) => t.id === 'a')!;
	leafA.messages = [{ timestamp: 't' } as unknown as (typeof leafA.messages)[number]];
	closeTab(entry, entry.activeGroupId, 'a');
	expect(leafA.messages).toEqual([]);
});

test('closeTab keeps the cache when another group still shows the topic', () => {
	const entry = makeEntry('a');
	openInFocusedGroup(entry, 'a', { pin: true });
	const g1 = entry.activeGroupId;
	splitGroup(entry, g1, 'right', 'a'); // 'a' now open in both groups
	const g2 = entry.activeGroupId;
	const leafA = entry.topics.find((t) => t.id === 'a')!;
	leafA.messages = [{ timestamp: 't' } as unknown as (typeof leafA.messages)[number]];
	closeTab(entry, g2, 'a'); // still open in g1
	expect(leafA.messages.length).toBe(1);
});

test('serializeLayout/deserializeLayout round-trips a nested split', () => {
	const entry = makeEntry('a', 'b', 'c');
	openInFocusedGroup(entry, 'a', { pin: true });
	const g1 = entry.activeGroupId;
	splitGroup(entry, g1, 'right', 'b'); // row [a, b]
	splitGroup(entry, entry.activeGroupId, 'down', 'c'); // b → column [b, c], c focused

	const restored = deserializeLayout(serializeLayout(entry));
	expect(restored).not.toBeNull();
	entry.layout = restored!.layout;
	entry.activeGroupId = restored!.activeGroupId;

	// Same structure: row root, second child is a column split.
	const root = entry.layout as Extract<LayoutNode, { type: 'split' }>;
	expect(root.direction).toBe('row');
	expect((root.children[1] as Extract<LayoutNode, { type: 'split' }>).direction).toBe('column');
	// Same set of open topics and the focused group restored to 'c'.
	expect([...subscribedTopicIds(entry)].sort()).toEqual(['a', 'b', 'c']);
	expect(focusedGroup(entry).activeTopicId).toBe('c');
});

test('deserializeLayout returns null on malformed input', () => {
	expect(deserializeLayout('not json')).toBeNull();
	expect(deserializeLayout('{}')).toBeNull();
});

test('focusGroup switches the focused group and updates selectedTopic', () => {
	const entry = makeEntry('a', 'b');
	openInFocusedGroup(entry, 'a', { pin: true });
	const g1 = entry.activeGroupId;
	splitGroup(entry, g1, 'right', 'b');
	expect(entry.selectedTopic?.id).toBe('b');
	focusGroup(entry, g1);
	expect(entry.selectedTopic?.id).toBe('a');
});
