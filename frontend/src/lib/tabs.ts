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
import type { BrokerRepositoryEntry } from './state';

/** Point `selectedTopic` at the leaf for `topicId` (or clear it if not found). */
function setActive(entry: BrokerRepositoryEntry, topicId: string | null) {
	entry.selectedTopic = topicId ? findbranchwithid(topicId, entry.topics) ?? null : null;
}

/**
 * Open a topic in the tab bar following VS Code editor-tab semantics.
 *
 * - Already open: just activate it. If `pin` is set, also pin it.
 * - Not open, `pin` false: open as a *preview* tab, reusing (replacing in place)
 *   an existing preview tab if there is one; otherwise append.
 * - Not open, `pin` true: open as a pinned tab (appended).
 */
export function openTab(entry: BrokerRepositoryEntry, topicId: string, { pin }: { pin: boolean }) {
	const existing = entry.openTabs.find((tab) => tab.id === topicId);
	if (existing) {
		if (pin) {
			existing.preview = false;
			entry.openTabs = [...entry.openTabs];
		}
		setActive(entry, topicId);
		return;
	}

	if (pin) {
		entry.openTabs = [...entry.openTabs, { id: topicId, preview: false }];
	} else {
		const previewIndex = entry.openTabs.findIndex((tab) => tab.preview);
		if (previewIndex >= 0) {
			const next = entry.openTabs.slice();
			next[previewIndex] = { id: topicId, preview: true };
			entry.openTabs = next;
		} else {
			entry.openTabs = [...entry.openTabs, { id: topicId, preview: true }];
		}
	}
	setActive(entry, topicId);
}

/** Pin an open tab so it is no longer reused for the next preview selection. */
export function pinTab(entry: BrokerRepositoryEntry, topicId: string) {
	const tab = entry.openTabs.find((t) => t.id === topicId);
	if (tab && tab.preview) {
		tab.preview = false;
		entry.openTabs = [...entry.openTabs];
	}
}

/** Make an already-open tab the active one. */
export function activateTab(entry: BrokerRepositoryEntry, topicId: string) {
	if (entry.openTabs.some((tab) => tab.id === topicId)) {
		setActive(entry, topicId);
	}
}

/** Close a tab. If it was the active one, activate the nearest neighbour. */
export function closeTab(entry: BrokerRepositoryEntry, topicId: string) {
	const index = entry.openTabs.findIndex((tab) => tab.id === topicId);
	if (index < 0) {
		return;
	}
	const wasActive = entry.selectedTopic?.id === topicId;
	entry.openTabs = entry.openTabs.filter((tab) => tab.id !== topicId);

	// Free the closed tab's cached messages so we never hold content for a topic
	// that is no longer shown. Reopening it triggers a fresh sync from the backend.
	const closedLeaf = findbranchwithid(topicId, entry.topics);
	if (closedLeaf) {
		closedLeaf.messages = [];
	}

	if (wasActive) {
		const neighbour = entry.openTabs[index] ?? entry.openTabs[index - 1] ?? null;
		setActive(entry, neighbour ? neighbour.id : null);
	}
}
