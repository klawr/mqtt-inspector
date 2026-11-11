/*
 * Copyright (c) 2026 Kai Lawrence
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

import type { GroupView, Message, MessageSelection } from './state';

// Message navigation for an editor group. The toolbar, the editor pane and the
// message handling all go through these, so a selection's index and its pinned
// message always change together. Messages are newest first.

function newSelection(index: number): MessageSelection {
	return { index, pinned: undefined, evicted: false };
}

/** Fresh view: following the latest message, compare against the one before. */
export function newGroupView(): GroupView {
	return { main: newSelection(0), compare: newSelection(1), compareMessage: false };
}

export function isLocked(selection: MessageSelection): boolean {
	return selection.pinned !== undefined;
}

/** The message to display: the selected one, or the pinned one if it was evicted. */
export function shownMessage(
	selection: MessageSelection,
	messages: readonly Message[]
): Message | undefined {
	return selection.evicted ? selection.pinned : messages[selection.index];
}

/** Show the message at `index` and lock onto it (what clicking or stepping does). */
export function selectMessageAt(
	selection: MessageSelection,
	messages: readonly Message[],
	index: number
) {
	selection.index = Math.max(0, Math.min(index, messages.length - 1));
	selection.pinned = messages[selection.index];
	selection.evicted = false;
}

/** Step relative to the shown message (+1 = older, -1 = newer) and lock onto it. */
export function stepMessage(
	selection: MessageSelection,
	messages: readonly Message[],
	delta: number
) {
	if (selection.evicted) {
		// The evicted message is older than everything kept: one step newer is
		// the oldest kept message, and there is nothing older.
		if (delta < 0) selectMessageAt(selection, messages, messages.length + delta);
		return;
	}
	selectMessageAt(selection, messages, selection.index + delta);
}

/** Lock onto the shown message, or unlock (stay at the current position). */
export function setLocked(
	selection: MessageSelection,
	messages: readonly Message[],
	locked: boolean
) {
	selection.pinned = locked ? messages[selection.index] : undefined;
	selection.evicted = false;
}

/** Back to the newest message, following new arrivals on both sides. */
export function resetToLatest(view: GroupView) {
	view.main = newSelection(0);
	view.compare = newSelection(1);
}

/** Defaults when the pane switches to another topic. */
export function resetForTopicSwitch(view: GroupView) {
	resetToLatest(view);
	view.compareMessage = false;
}

/**
 * Re-resolve a selection after its message list changed (messages prepended,
 * evicted from the end, or merged out of order). A locked selection follows its
 * pinned message; an unlocked one keeps its position (0 = follow the newest).
 */
export function followPinnedMessage(selection: MessageSelection, messages: readonly Message[]) {
	const { pinned } = selection;
	if (!pinned || messages[selection.index] === pinned) return;
	const found = messages.indexOf(pinned);
	if (found >= 0) {
		selection.index = found;
		selection.evicted = false;
	} else {
		// Evicted: keep showing it, and park the index on the oldest kept message
		// so stepping newer continues from there.
		selection.index = Math.max(0, messages.length - 1);
		selection.evicted = true;
	}
}

/** Re-resolve both sides of a view after its message list changed. */
export function followMessages(view: GroupView, messages: readonly Message[]) {
	followPinnedMessage(view.main, messages);
	followPinnedMessage(view.compare, messages);
}
