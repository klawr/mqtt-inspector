/*
 * Copyright (c) 2025 Kai Lawrence
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

import { findbranchwithid } from './helper';
import type { BrokerRepositoryEntry, EditorGroup, GroupView, LayoutNode } from './state';

export type SplitEdge = 'left' | 'right' | 'up' | 'down';

let groupCounter = 0;
function nextGroupId(): string {
	groupCounter += 1;
	return `g${groupCounter}`;
}

function defaultGroupView(): GroupView {
	return {
		selectedIndex: 0,
		lockedIndex: false,
		compareMessage: false,
		selectedIndexCompare: 1,
		lockedIndexCompare: false
	};
}

function createGroup(): EditorGroup {
	return { id: nextGroupId(), tabs: [], activeTopicId: null, view: defaultGroupView() };
}

/** A fresh layout with a single empty editor group. */
export function newLayout(): { layout: LayoutNode; activeGroupId: string } {
	const group = createGroup();
	return { layout: { type: 'group', group }, activeGroupId: group.id };
}

// ─── Tree traversal ──────────────────────────────────────────────────

/** All editor groups in left-to-right / top-to-bottom order. */
export function allGroups(node: LayoutNode): EditorGroup[] {
	if (node.type === 'group') {
		return [node.group];
	}
	return node.children.flatMap(allGroups);
}

function findGroup(entry: BrokerRepositoryEntry, groupId: string): EditorGroup | undefined {
	return allGroups(entry.layout).find((g) => g.id === groupId);
}

export function focusedGroup(entry: BrokerRepositoryEntry): EditorGroup {
	return findGroup(entry, entry.activeGroupId) ?? allGroups(entry.layout)[0];
}

/** Union of every group's active topic — the set of topics to stream live. */
export function subscribedTopicIds(entry: BrokerRepositoryEntry): Set<string> {
	const ids = new Set<string>();
	for (const g of allGroups(entry.layout)) {
		if (g.activeTopicId) ids.add(g.activeTopicId);
	}
	return ids;
}

/** Keep `entry.selectedTopic` pointing at the focused group's active topic. */
export function syncSelectedTopic(entry: BrokerRepositoryEntry) {
	const active = focusedGroup(entry)?.activeTopicId ?? null;
	entry.selectedTopic = active ? findbranchwithid(active, entry.topics) ?? null : null;
}

/** Free a topic's cached messages if no group still has it open. */
function freeCacheIfUnused(entry: BrokerRepositoryEntry, topicId: string) {
	const stillOpen = allGroups(entry.layout).some((g) => g.tabs.some((t) => t.id === topicId));
	if (!stillOpen) {
		const leaf = findbranchwithid(topicId, entry.topics);
		if (leaf) leaf.messages = [];
	}
}

// ─── Group-level tab operations ──────────────────────────────────────

/** VS Code preview/pin tab semantics within a single group. */
function openTab(group: EditorGroup, topicId: string, { pin }: { pin: boolean }) {
	const existing = group.tabs.find((tab) => tab.id === topicId);
	if (existing) {
		if (pin) existing.preview = false;
		group.activeTopicId = topicId;
		return;
	}
	if (pin) {
		group.tabs = [...group.tabs, { id: topicId, preview: false }];
	} else {
		const previewIndex = group.tabs.findIndex((tab) => tab.preview);
		if (previewIndex >= 0) {
			const next = group.tabs.slice();
			next[previewIndex] = { id: topicId, preview: true };
			group.tabs = next;
		} else {
			group.tabs = [...group.tabs, { id: topicId, preview: true }];
		}
	}
	group.activeTopicId = topicId;
}

function pinTabInGroup(group: EditorGroup, topicId: string) {
	const tab = group.tabs.find((t) => t.id === topicId);
	if (tab && tab.preview) tab.preview = false;
}

function removeTab(group: EditorGroup, topicId: string) {
	const index = group.tabs.findIndex((tab) => tab.id === topicId);
	if (index < 0) return;
	const wasActive = group.activeTopicId === topicId;
	group.tabs = group.tabs.filter((tab) => tab.id !== topicId);
	if (wasActive) {
		const neighbour = group.tabs[index] ?? group.tabs[index - 1] ?? null;
		group.activeTopicId = neighbour ? neighbour.id : null;
	}
}

// ─── Entry-level operations (focus, open, close) ─────────────────────

export function focusGroup(entry: BrokerRepositoryEntry, groupId: string) {
	if (findGroup(entry, groupId)) {
		entry.activeGroupId = groupId;
		syncSelectedTopic(entry);
	}
}

export function openInFocusedGroup(
	entry: BrokerRepositoryEntry,
	topicId: string,
	{ pin }: { pin: boolean }
) {
	openTab(focusedGroup(entry), topicId, { pin });
	syncSelectedTopic(entry);
}

export function activateTab(entry: BrokerRepositoryEntry, groupId: string, topicId: string) {
	const group = findGroup(entry, groupId);
	if (group && group.tabs.some((t) => t.id === topicId)) {
		group.activeTopicId = topicId;
		entry.activeGroupId = groupId;
		syncSelectedTopic(entry);
	}
}

export function pinTab(entry: BrokerRepositoryEntry, groupId: string, topicId: string) {
	const group = findGroup(entry, groupId);
	if (group) {
		pinTabInGroup(group, topicId);
		syncSelectedTopic(entry);
	}
}

export function closeTab(entry: BrokerRepositoryEntry, groupId: string, topicId: string) {
	const group = findGroup(entry, groupId);
	if (!group) return;
	removeTab(group, topicId);
	if (group.tabs.length === 0) {
		closeGroup(entry, groupId);
	}
	freeCacheIfUnused(entry, topicId);
	syncSelectedTopic(entry);
}

/** Close every tab in a group except `keepTopicId`. */
export function closeOtherTabs(entry: BrokerRepositoryEntry, groupId: string, keepTopicId: string) {
	const group = findGroup(entry, groupId);
	if (!group) return;
	for (const id of group.tabs.map((t) => t.id)) {
		if (id !== keepTopicId) closeTab(entry, groupId, id);
	}
}

// ─── Split / move / close-group tree operations ──────────────────────

function edgeToSplit(edge: SplitEdge): { direction: 'row' | 'column'; before: boolean } {
	switch (edge) {
		case 'left':
			return { direction: 'row', before: true };
		case 'right':
			return { direction: 'row', before: false };
		case 'up':
			return { direction: 'column', before: true };
		case 'down':
			return { direction: 'column', before: false };
	}
}

type Located = {
	node: LayoutNode;
	parent: Extract<LayoutNode, { type: 'split' }> | null;
	index: number;
};

function locate(
	node: LayoutNode,
	groupId: string,
	parent: Extract<LayoutNode, { type: 'split' }> | null = null,
	index = -1
): Located | null {
	if (node.type === 'group') {
		return node.group.id === groupId ? { node, parent, index } : null;
	}
	for (let i = 0; i < node.children.length; i++) {
		const found = locate(node.children[i], groupId, node, i);
		if (found) return found;
	}
	return null;
}

function equalSizes(count: number): number[] {
	return new Array(count).fill(100 / count);
}

/** Insert `newNode` beside the target group along `edge`, merging into an existing
 *  split of the same direction or wrapping the target in a new split. */
function insertBeside(
	entry: BrokerRepositoryEntry,
	targetGroupId: string,
	edge: SplitEdge,
	newNode: LayoutNode
) {
	const { direction, before } = edgeToSplit(edge);
	const found = locate(entry.layout, targetGroupId);
	if (!found) return;

	if (found.parent && found.parent.direction === direction) {
		const insertAt = before ? found.index : found.index + 1;
		found.parent.children.splice(insertAt, 0, newNode);
		found.parent.sizes = equalSizes(found.parent.children.length);
		return;
	}

	const splitNode: LayoutNode = {
		type: 'split',
		direction,
		children: before ? [newNode, found.node] : [found.node, newNode],
		sizes: [50, 50]
	};
	if (found.parent) {
		found.parent.children[found.index] = splitNode;
	} else {
		entry.layout = splitNode;
	}
}

/** Split a group along `edge`, opening `topicId` (pinned) in the new group. */
export function splitGroup(
	entry: BrokerRepositoryEntry,
	groupId: string,
	edge: SplitEdge,
	topicId: string
) {
	const group = createGroup();
	openTab(group, topicId, { pin: true });
	insertBeside(entry, groupId, edge, { type: 'group', group });
	entry.activeGroupId = group.id;
	syncSelectedTopic(entry);
}

/** Move a tab from one existing group to another (dropped on its tab strip). */
export function moveTab(
	entry: BrokerRepositoryEntry,
	fromGroupId: string,
	toGroupId: string,
	topicId: string
) {
	const to = findGroup(entry, toGroupId);
	if (!to) return;
	if (fromGroupId === toGroupId) {
		activateTab(entry, toGroupId, topicId);
		return;
	}
	const from = findGroup(entry, fromGroupId);
	if (from) removeTab(from, topicId);
	openTab(to, topicId, { pin: true });
	entry.activeGroupId = toGroupId;
	if (from && from.tabs.length === 0) {
		closeGroup(entry, fromGroupId);
	}
	syncSelectedTopic(entry);
}

/** Move a tab onto a group's edge → create a new split holding that tab. */
export function moveTabToNewSplit(
	entry: BrokerRepositoryEntry,
	targetGroupId: string,
	edge: SplitEdge,
	fromGroupId: string,
	topicId: string
) {
	const from = findGroup(entry, fromGroupId);
	if (from) removeTab(from, topicId);
	splitGroup(entry, targetGroupId, edge, topicId);
	if (from && from.tabs.length === 0 && from.id !== entry.activeGroupId) {
		closeGroup(entry, fromGroupId);
	}
	syncSelectedTopic(entry);
}

/** Remove `groupId` from `node`, collapsing single-child splits. Returns the new
 *  subtree, or null if it became empty. */
function removeGroupNode(node: LayoutNode, groupId: string): LayoutNode | null {
	if (node.type === 'group') {
		return node.group.id === groupId ? null : node;
	}
	const children: LayoutNode[] = [];
	const sizes: number[] = [];
	node.children.forEach((child, i) => {
		const kept = removeGroupNode(child, groupId);
		if (kept) {
			children.push(kept);
			sizes.push(node.sizes[i] ?? 100 / node.children.length);
		}
	});
	if (children.length === 0) return null;
	if (children.length === 1) return children[0]; // collapse
	const total = sizes.reduce((a, b) => a + b, 0) || 1;
	return {
		type: 'split',
		direction: node.direction,
		children,
		sizes: sizes.map((s) => (s / total) * 100)
	};
}

// ─── URL (de)serialisation ───────────────────────────────────────────

type SerializedNode =
	| { t: 'g'; tabs: { id: string; preview: boolean }[]; a: string | null; f?: boolean }
	| { t: 's'; d: 'row' | 'column'; z: number[]; c: SerializedNode[] };

function serializeNode(node: LayoutNode, activeGroupId: string): SerializedNode {
	if (node.type === 'group') {
		return {
			t: 'g',
			tabs: node.group.tabs.map((tab) => ({ id: tab.id, preview: tab.preview })),
			a: node.group.activeTopicId,
			...(node.group.id === activeGroupId ? { f: true } : {})
		};
	}
	return {
		t: 's',
		d: node.direction,
		z: node.sizes,
		c: node.children.map((child) => serializeNode(child, activeGroupId))
	};
}

/** Serialise the layout tree to a compact JSON string for the URL. */
export function serializeLayout(entry: BrokerRepositoryEntry): string {
	return JSON.stringify(serializeNode(entry.layout, entry.activeGroupId));
}

/** Rebuild a layout from `serializeLayout` output. Group ids are regenerated;
 *  the previously focused group is restored as `activeGroupId`. Returns null on
 *  malformed input. */
export function deserializeLayout(
	json: string
): { layout: LayoutNode; activeGroupId: string } | null {
	let data: SerializedNode;
	try {
		data = JSON.parse(json);
	} catch {
		return null;
	}
	let focusedId: string | null = null;
	let firstId: string | null = null;

	function build(node: SerializedNode): LayoutNode | null {
		if (!node || typeof node !== 'object') return null;
		if (node.t === 'g') {
			const group = createGroup();
			group.tabs = (node.tabs ?? [])
				.filter((tab) => tab && typeof tab.id === 'string')
				.map((tab) => ({ id: tab.id, preview: !!tab.preview }));
			group.activeTopicId = typeof node.a === 'string' ? node.a : null;
			firstId ??= group.id;
			if (node.f) focusedId = group.id;
			return { type: 'group', group };
		}
		if (node.t === 's' && Array.isArray(node.c)) {
			const children = node.c.map(build).filter((c): c is LayoutNode => c !== null);
			if (children.length === 0) return null;
			if (children.length === 1) return children[0];
			const sizes =
				Array.isArray(node.z) && node.z.length === children.length
					? node.z
					: equalSizes(children.length);
			return { type: 'split', direction: node.d === 'column' ? 'column' : 'row', children, sizes };
		}
		return null;
	}

	const layout = build(data);
	if (!layout || !firstId) return null;
	return { layout, activeGroupId: focusedId ?? firstId };
}

export function closeGroup(entry: BrokerRepositoryEntry, groupId: string) {
	const groups = allGroups(entry.layout);
	const closing = groups.find((g) => g.id === groupId);
	if (!closing) return;
	const closingTopics = closing.tabs.map((t) => t.id);

	if (groups.length <= 1) {
		// Never leave zero groups — keep an empty one instead.
		closing.tabs = [];
		closing.activeTopicId = null;
	} else {
		entry.layout = removeGroupNode(entry.layout, groupId) ?? newLayout().layout;
		if (entry.activeGroupId === groupId) {
			entry.activeGroupId = allGroups(entry.layout)[0].id;
		}
	}
	for (const topicId of closingTopics) freeCacheIfUnused(entry, topicId);
	syncSelectedTopic(entry);
}
