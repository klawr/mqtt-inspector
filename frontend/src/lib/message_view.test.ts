import { test, expect } from 'vitest';
import {
	followMessages,
	followPinnedMessage,
	isLocked,
	newGroupView,
	resetForTopicSwitch,
	resetToLatest,
	selectMessageAt,
	setLocked,
	shownMessage,
	stepMessage
} from './message_view';
import { Message, type MessageSelection } from './state';

let seq = 0;
const msg = () =>
	new Message(`2026-01-01T00:00:${String(seq++).padStart(2, '0')}Z`, null, `m${seq}`);
const msgs = (n: number) => Array.from({ length: n }, msg);
const unlocked = (index = 0): MessageSelection => ({ index, evicted: false });

/** Simulates a full buffer: n new messages in front, the n oldest evicted at the end. */
function arriveWithEviction(list: Message[], n: number): Message[] {
	return [...msgs(n), ...list].slice(0, list.length);
}

test('a locked message stays shown when the buffer is full (count does not change)', () => {
	const sel = unlocked();
	let list = msgs(10);
	selectMessageAt(sel, list, 3);
	const shown = list[3];
	for (let i = 0; i < 5; i++) {
		list = arriveWithEviction(list, 1);
		followPinnedMessage(sel, list);
		expect(shownMessage(sel, list)).toBe(shown);
	}
	expect(sel.index).toBe(8);
	expect(sel.evicted).toBe(false);
});

test('a locked message stays shown when evictions arrive separately from new messages', () => {
	const sel = unlocked();
	let list = msgs(10);
	selectMessageAt(sel, list, 2);
	const shown = list[2];
	list.pop(); // eviction applied in place first
	followPinnedMessage(sel, list);
	list = [...msgs(3), ...list];
	followPinnedMessage(sel, list);
	expect(shownMessage(sel, list)).toBe(shown);
});

test('stepping moves relative to the shown message, even mid-stream', () => {
	const sel = unlocked();
	let list = msgs(10);
	selectMessageAt(sel, list, 4);
	list = arriveWithEviction(list, 2);
	followPinnedMessage(sel, list);
	const shown = shownMessage(sel, list)!;

	stepMessage(sel, list, 1); // older
	expect(shownMessage(sel, list)).toBe(list[list.indexOf(shown) + 1]);
	expect(sel.pinned).toBe(shownMessage(sel, list));

	stepMessage(sel, list, -1); // newer, back to where we were
	expect(shownMessage(sel, list)).toBe(shown);

	// The step's target stays pinned while more messages arrive.
	list = arriveWithEviction(list, 1);
	followPinnedMessage(sel, list);
	expect(shownMessage(sel, list)).toBe(shown);
});

test('stepping clamps at the newest and oldest message', () => {
	const sel = unlocked();
	const list = msgs(3);
	stepMessage(sel, list, -1);
	expect(sel.index).toBe(0);
	selectMessageAt(sel, list, 2);
	stepMessage(sel, list, 1);
	expect(sel.index).toBe(2);
});

test('the compare side is pinned independently of the main side', () => {
	const view = newGroupView();
	view.compareMessage = true;
	let list = msgs(10);
	selectMessageAt(view.main, list, 1);
	selectMessageAt(view.compare, list, 5);
	const [a, b] = [list[1], list[5]];
	list = arriveWithEviction(list, 3);
	followMessages(view, list);
	expect(shownMessage(view.main, list)).toBe(a);
	expect(shownMessage(view.compare, list)).toBe(b);
});

test('an unlocked selection keeps its position, i.e. follows the newest message', () => {
	const view = newGroupView();
	const list = arriveWithEviction(msgs(5), 2);
	followMessages(view, list);
	expect(view.main.index).toBe(0);
	expect(view.compare.index).toBe(1);
});

test('locking pins the shown message, unlocking releases it', () => {
	const sel = unlocked();
	let list = msgs(5);
	setLocked(sel, list, true);
	expect(isLocked(sel)).toBe(true);
	const shown = list[0];
	list = arriveWithEviction(list, 2);
	followPinnedMessage(sel, list);
	expect(shownMessage(sel, list)).toBe(shown);

	setLocked(sel, list, false);
	expect(isLocked(sel)).toBe(false);
	expect(sel.index).toBe(2); // stays where it was
});

test('locking with no messages has nothing to pin', () => {
	const sel = unlocked();
	setLocked(sel, [], true);
	expect(isLocked(sel)).toBe(false);
});

test('an evicted locked message stays shown and is flagged', () => {
	const sel = unlocked();
	let list = msgs(5);
	selectMessageAt(sel, list, 4); // the oldest
	const shown = list[4];
	list = arriveWithEviction(list, 1);
	followPinnedMessage(sel, list);
	expect(shownMessage(sel, list)).toBe(shown);
	expect(sel.evicted).toBe(true);
	expect(sel.index).toBe(4); // parked on the oldest kept

	// More traffic: still the same evicted message, no churn.
	list = arriveWithEviction(list, 3);
	followPinnedMessage(sel, list);
	expect(shownMessage(sel, list)).toBe(shown);

	// Nothing older to step to.
	stepMessage(sel, list, 1);
	expect(shownMessage(sel, list)).toBe(shown);

	// One step newer is the oldest kept message, and the flag clears.
	stepMessage(sel, list, -1);
	expect(sel.evicted).toBe(false);
	expect(shownMessage(sel, list)).toBe(list[4]);
});

test('unlocking an evicted message clears the flag', () => {
	const sel = unlocked();
	let list = msgs(3);
	selectMessageAt(sel, list, 2);
	list = arriveWithEviction(list, 1);
	followPinnedMessage(sel, list);
	setLocked(sel, list, false);
	expect(sel.evicted).toBe(false);
	expect(shownMessage(sel, list)).toBe(list[2]);
});

test('resetToLatest unlocks both sides but keeps compare mode', () => {
	const view = newGroupView();
	const list = msgs(5);
	view.compareMessage = true;
	selectMessageAt(view.main, list, 3);
	selectMessageAt(view.compare, list, 4);
	resetToLatest(view);
	expect(view).toEqual({ ...newGroupView(), compareMessage: true });
});

test('resetForTopicSwitch also leaves compare mode', () => {
	const view = newGroupView();
	view.compareMessage = true;
	selectMessageAt(view.main, msgs(3), 2);
	resetForTopicSwitch(view);
	expect(view).toEqual(newGroupView());
});
