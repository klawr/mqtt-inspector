// Copyright (c) 2024-2025 Kai Lawrence
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

/**
 * Per-topic throughput (bytes/s and messages/s), tracked client-side from the
 * live message meta notifications.
 *
 * Every message is counted on its topic and on each parent prefix, so the rate
 * of a branch is the cumulative rate of everything below it (including its own
 * messages). Counts are kept in 1 s buckets over the last minute, so memory
 * stays constant per topic no matter how fast it publishes.
 *
 * The window adapts to the traffic: busy topics use the last 10 s so the rate
 * reacts quickly; slow topics use the whole minute so a topic publishing every
 * few seconds (or once a minute) shows a steady rate instead of flickering.
 */

/** Short window (seconds), used when it holds enough messages. */
export const SHORT_WINDOW_S = 10;
/** Long window (seconds); also how long a quiet topic keeps showing a rate. */
export const LONG_WINDOW_S = 60;
/** Minimum messages in the short window to trust it over the long one. */
export const SHORT_WINDOW_MIN_MESSAGES = 5;

export type RateWindow = 'short' | 'long';

export type TopicRate = {
	bytesPerSecond: number;
	messagesPerSecond: number;
	/** The window the rate was measured over. */
	window: RateWindow;
};

type RateBuckets = {
	// per 1 s bucket, indexed by (second % LONG_WINDOW_S)
	bytes: Float64Array;
	messages: Float64Array;
	lastSecond: number; // the most recent second written to the buckets
	firstMs: number; // when this topic was first seen (to not under-report new topics)
};

const rates = new Map<string, RateBuckets>();

function key(source: string, topicPath: string): string {
	return `${source}\0${topicPath}`;
}

/** Zero the buckets for the seconds that passed since the last write. */
function advance(r: RateBuckets, second: number) {
	const gap = second - r.lastSecond;
	if (gap <= 0) return;
	if (gap >= LONG_WINDOW_S) {
		r.bytes.fill(0);
		r.messages.fill(0);
	} else {
		for (let s = r.lastSecond + 1; s <= second; s++) {
			r.bytes[s % LONG_WINDOW_S] = 0;
			r.messages[s % LONG_WINDOW_S] = 0;
		}
	}
	r.lastSecond = second;
}

/** Record one message of `bytes` on `topic` and on every parent prefix of it. */
export function recordTopicMessage(
	source: string,
	topic: string,
	bytes: number,
	now: number = Date.now()
) {
	const second = Math.floor(now / 1000);
	const slot = second % LONG_WINDOW_S;
	let end = -1;
	do {
		end = topic.indexOf('/', end + 1);
		const prefix = end === -1 ? topic : topic.slice(0, end);
		const k = key(source, prefix);
		let r = rates.get(k);
		if (!r) {
			r = {
				bytes: new Float64Array(LONG_WINDOW_S),
				messages: new Float64Array(LONG_WINDOW_S),
				lastSecond: second,
				firstMs: now
			};
			rates.set(k, r);
		}
		advance(r, second);
		r.bytes[slot] += bytes;
		r.messages[slot] += 1;
	} while (end !== -1);
}

/** Sum the newest `windowS` buckets (the current, partial second included). */
function sumWindow(r: RateBuckets, second: number, windowS: number) {
	let bytes = 0;
	let messages = 0;
	for (let i = 0; i < windowS; i++) {
		const slot = (((second - i) % LONG_WINDOW_S) + LONG_WINDOW_S) % LONG_WINDOW_S;
		bytes += r.bytes[slot];
		messages += r.messages[slot];
	}
	return { bytes, messages };
}

/**
 * Rates for a topic (cumulative for a branch). Zero when nothing arrived within
 * the last minute; idle entries are dropped.
 *
 * Pass `window` to force a window, e.g. so a branch and its children are
 * measured the same way and can be subtracted from each other.
 */
export function topicRate(
	source: string,
	topicPath: string,
	now: number = Date.now(),
	window?: RateWindow
): TopicRate {
	const k = key(source, topicPath);
	const r = rates.get(k);
	if (!r) return { bytesPerSecond: 0, messagesPerSecond: 0, window: window ?? 'short' };
	const second = Math.floor(now / 1000);
	if (second - r.lastSecond >= LONG_WINDOW_S) {
		rates.delete(k);
		return { bytesPerSecond: 0, messagesPerSecond: 0, window: window ?? 'short' };
	}
	advance(r, second);

	// The current bucket only covers part of its second, so divide by the time
	// actually spanned instead of the full window to avoid under-reporting.
	const partial = (now % 1000) / 1000;
	const short = sumWindow(r, second, SHORT_WINDOW_S);
	const chosen: RateWindow =
		window ?? (short.messages >= SHORT_WINDOW_MIN_MESSAGES ? 'short' : 'long');

	if (chosen === 'short') {
		const spanned = SHORT_WINDOW_S - 1 + partial;
		return {
			bytesPerSecond: short.bytes / spanned,
			messagesPerSecond: short.messages / spanned,
			window: 'short'
		};
	}
	const long = sumWindow(r, second, LONG_WINDOW_S);
	// A topic first seen less than a minute ago hasn't had a full minute to
	// accumulate; divide by the time we actually watched it (at least the short
	// window, so a single fresh message doesn't read as a huge rate).
	const watched = Math.max(SHORT_WINDOW_S - 1 + partial, (now - r.firstMs) / 1000);
	const spanned = Math.min(LONG_WINDOW_S - 1 + partial, watched);
	return {
		bytesPerSecond: long.bytes / spanned,
		messagesPerSecond: long.messages / spanned,
		window: 'long'
	};
}

/** Forget all rates of a broker (e.g. when it is removed). */
export function clearTopicRates(source: string) {
	const prefix = `${source}\0`;
	for (const k of rates.keys()) {
		if (k.startsWith(prefix)) rates.delete(k);
	}
}
