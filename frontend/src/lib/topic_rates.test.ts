import { test, expect, beforeEach } from 'vitest';
import {
	LONG_WINDOW_S,
	SHORT_WINDOW_MIN_MESSAGES,
	SHORT_WINDOW_S,
	clearTopicRates,
	recordTopicMessage,
	topicRate
} from './topic_rates';
import { formatMsgRate, formatRate } from './helper';

beforeEach(() => {
	clearTopicRates('b');
	clearTopicRates('other');
});

const SHORT_SPAN = SHORT_WINDOW_S - 1;
const LONG_SPAN = LONG_WINDOW_S - 1;

/** Publish `count` messages of `bytes` on `topic`, spread evenly over `seconds`
 *  ending at `end` (ms). Returns `end`. */
function publish(topic: string, count: number, bytes: number, seconds: number, end: number) {
	// oldest first, like real traffic
	for (let i = count - 1; i >= 0; i--) {
		recordTopicMessage('b', topic, bytes, end - Math.floor((i * seconds * 1000) / count));
	}
	return end;
}

test('topicRate is 0 for unknown topics', () => {
	const r = topicRate('b', 'nothing/here', 0);
	expect(r.bytesPerSecond).toBe(0);
	expect(r.messagesPerSecond).toBe(0);
});

test('busy topics use the short window; branches are cumulative', () => {
	const t = 1_000_000; // start of a second
	for (let i = 0; i < 10; i++) {
		recordTopicMessage('b', 'home/a', 100, t);
		recordTopicMessage('b', 'home/b', 200, t);
	}
	const a = topicRate('b', 'home/a', t);
	expect(a.window).toBe('short');
	expect(a.messagesPerSecond).toBeCloseTo(10 / SHORT_SPAN);
	expect(a.bytesPerSecond).toBeCloseTo(1000 / SHORT_SPAN);
	expect(topicRate('b', 'home', t).bytesPerSecond).toBeCloseTo(3000 / SHORT_SPAN);
	expect(topicRate('b', 'ho', t).messagesPerSecond).toBe(0); // not a path prefix
});

test('slow topics use the long window and keep a steady rate', () => {
	// One message every 10 s for over a minute, watched since well before.
	recordTopicMessage('b', 'warmup', 1, 0); // irrelevant topic
	const start = 2_000_000;
	recordTopicMessage('b', 'slow', 50, start - 120_000); // first seen 2 min ago
	for (let s = 0; s <= 70; s += 10) recordTopicMessage('b', 'slow', 50, start + s * 1000);
	const now = start + 70_000;
	const r = topicRate('b', 'slow', now);
	expect(r.window).toBe('long');
	// 6 messages fall into the last 60 s
	expect(r.messagesPerSecond).toBeCloseTo(6 / LONG_SPAN);
	// still shown 50 s after the last message
	expect(topicRate('b', 'slow', now + 50_000).messagesPerSecond).toBeGreaterThan(0);
});

test('switches to the short window once it holds enough messages', () => {
	const t = 3_000_000;
	publish('x', SHORT_WINDOW_MIN_MESSAGES - 1, 1, 5, t);
	expect(topicRate('b', 'x', t).window).toBe('long');
	recordTopicMessage('b', 'x', 1, t);
	expect(topicRate('b', 'x', t).window).toBe('short');
});

test('a forced window measures children like their branch', () => {
	const t = 4_000_000;
	publish('p/busy', 20, 10, 5, t);
	recordTopicMessage('b', 'p/quiet', 10, t);
	const branch = topicRate('b', 'p', t);
	expect(branch.window).toBe('short');
	expect(topicRate('b', 'p/quiet', t).window).toBe('long');
	expect(topicRate('b', 'p/quiet', t, branch.window).window).toBe('short');
});

test('new topics are not under-reported before a full minute has passed', () => {
	const t = 5_000_000;
	// first seen 30 s ago, 3 messages since then
	publish('fresh', 3, 1, 30, t);
	const r = topicRate('b', 'fresh', t);
	expect(r.window).toBe('long');
	expect(r.messagesPerSecond).toBeCloseTo(3 / 30, 1);
});

test('entries expire after the long window and brokers do not mix', () => {
	const t = 6_000_000;
	recordTopicMessage('b', 't', 100, t);
	recordTopicMessage('other', 't', 5000, t);
	expect(topicRate('b', 't', t).bytesPerSecond).toBeCloseTo(100 / SHORT_SPAN);
	expect(topicRate('b', 't', t + LONG_WINDOW_S * 1000).messagesPerSecond).toBe(0);
	clearTopicRates('other');
	expect(topicRate('other', 't', t).messagesPerSecond).toBe(0);
});

test('message rates below 1/s are shown per minute, byte rates always per second', () => {
	expect(formatMsgRate(0)).toBe('0 msg/s');
	expect(formatMsgRate(0.1)).toBe('6.0 msg/min');
	expect(formatMsgRate(0.5)).toBe('30 msg/min');
	expect(formatMsgRate(2.5)).toBe('2.5 msg/s');
	expect(formatMsgRate(1500)).toBe('1.5k msg/s');
	expect(formatRate(0)).toBe('0 B/s');
	expect(formatRate(0.25)).toBe('0.3 B/s');
	expect(formatRate(100)).toBe('100 B/s');
	expect(formatRate(2048)).toBe('2.0 KB/s');
});
