/*
 * Copyright (c) 2024 Kai Lawrence
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

import { findbranchwithid, formatMsgRate, formatRate } from './helper';
import { clearTopicRates, recordTopicMessage, topicRate, type TopicRate } from './topic_rates';
import { allGroups, newLayout } from './layout';
import { followMessages } from './message_view';
import { Message } from './state';
import type {
	AppState,
	BrokerRepository,
	BrokerRepositoryEntry,
	MessageProperties,
	MqttProtocol,
	Treebranch
} from './state';
import type { Session } from '../store';

/** Sliding-window rate tracker state per broker. */
type RateWindowState = {
	samples: { t: number; b: number }[];
	start: number;
	sumBytes: number;
};

/** Sliding-window rate tracker: per-broker list of (timestamp_ms, byteCount) samples. */
const rateWindows: Record<string, RateWindowState> = {};
const RATE_WINDOW_MS = 3000;

function updateRates(
	broker: string,
	bytesReceived: number
): { bytesPerSecond: number; messagesPerSecond: number } {
	const now = Date.now();
	if (!rateWindows[broker]) {
		rateWindows[broker] = { samples: [], start: 0, sumBytes: 0 };
	}
	const window = rateWindows[broker];
	window.samples.push({ t: now, b: bytesReceived });
	window.sumBytes += bytesReceived;

	// Prune entries older than the window using a moving head index.
	const cutoff = now - RATE_WINDOW_MS;
	while (window.start < window.samples.length && window.samples[window.start].t < cutoff) {
		window.sumBytes -= window.samples[window.start].b;
		window.start += 1;
	}

	if (window.start > 1024 && window.start * 2 > window.samples.length) {
		window.samples = window.samples.slice(window.start);
		window.start = 0;
	}

	if (window.sumBytes < 0) {
		window.sumBytes = 0;
	}

	const seconds = RATE_WINDOW_MS / 1000;
	return {
		bytesPerSecond: window.sumBytes / seconds,
		messagesPerSecond: (window.samples.length - window.start) / seconds
	};
}

function findLeafBranch(topics: Treebranch[], topicPath: string): Treebranch | null {
	const parts = topicPath.split('/');
	let branches = topics;
	let found: Treebranch | null = null;
	for (const part of parts) {
		found = branches.find((b) => b.original_text === part) ?? null;
		if (!found) return null;
		branches = found.children ?? [];
	}
	return found;
}

/**
 * Compare two RFC 3339 timestamps at full precision (> 0 if `a` is newer).
 * `Date` keeps only milliseconds, but the backend sends up to nanoseconds, and
 * messages arriving in a burst often share a millisecond.
 */
export function compareTimestamps(a: string, b: string): number {
	return Date.parse(a) - Date.parse(b) || subMillisecond(a) - subMillisecond(b);
}

/** The fraction-of-second digits beyond milliseconds, in nanoseconds. */
function subMillisecond(timestamp: string): number {
	const digits = /\.\d{3}(\d*)/.exec(timestamp)?.[1] ?? '';
	return Number(digits.slice(0, 6).padEnd(6, '0'));
}

function sortMessagesNewestFirst<T extends { timestamp: string }>(messages: T[]): T[] {
	return messages.sort((left, right) => compareTimestamps(right.timestamp, left.timestamp));
}

function mergeMessagesNewestFirst(existing: Message[], incoming: Message[]): Message[] {
	const merged: Message[] = [];
	let incomingIndex = 0;
	let existingIndex = 0;

	while (incomingIndex < incoming.length && existingIndex < existing.length) {
		if (
			compareTimestamps(incoming[incomingIndex].timestamp, existing[existingIndex].timestamp) >= 0
		) {
			merged.push(incoming[incomingIndex]);
			incomingIndex += 1;
		} else {
			merged.push(existing[existingIndex]);
			existingIndex += 1;
		}
	}

	if (incomingIndex < incoming.length) {
		merged.push(...incoming.slice(incomingIndex));
	}
	if (existingIndex < existing.length) {
		merged.push(...existing.slice(existingIndex));
	}

	return merged;
}

export type CommandParam = { id: string; name: string; topic: string; payload: string };
export type BrokerParam = {
	broker: string;
	connected: boolean;
	topics: {
		[key: string]: { payload: ArrayBuffer; timestamp: string }[];
	};
	total_bytes?: number;
	total_messages?: number;
	rate_history?: { timestamp: number; bytes_per_second: number; total_bytes: number }[];
	requires_auth?: boolean;
	protocol?: MqttProtocol | null;
}[];
export type MqttConnectionStatus = {
	source: string;
	connected: boolean;
	/** Only sent on connect. */
	protocol?: MqttProtocol;
};
export type SessionParam = { mode?: string; user?: string; can_edit?: boolean };
export type PipelineParam = { id: string; name: string; pipeline: { topic: string }[] };
export type MQTTMessageParam = {
	source: string;
	topic: string;
	payload: ArrayBuffer;
	timestamp: string;
	original_payload_size?: number;
	total_bytes?: number;
	retain?: boolean;
	properties?: MessageProperties;
};

export type MQTTMessageMetaParam = {
	source: string;
	topic: string;
	timestamp: string;
	payload_size: number;
	total_bytes: number;
	topic_message_count: number;
	retain?: boolean;
};

export type TopicSummariesParam = {
	source: string;
	topics: { [key: string]: { count: number; latest_timestamp: string } };
};

export type MessagesEvictedParam = {
	source: string;
	topic: string;
	count: number;
	topic_message_count: number;
};

export function parseMqttWebSocketMessage(buffer: ArrayBuffer) {
	const bytes = new Uint8Array(buffer);
	if (bytes.length < 4) {
		return null;
	}
	const view = new DataView(buffer);
	const headerLength = view.getUint32(0, false);
	const headerStart = 4;
	const headerEnd = headerStart + headerLength;
	if (headerEnd > bytes.length) {
		return null;
	}
	const headerJson = new TextDecoder().decode(bytes.slice(headerStart, headerEnd));
	const header = JSON.parse(headerJson);
	if (header.method !== 'mqtt_message') {
		return null;
	}
	return {
		jsonrpc: header.jsonrpc,
		method: header.method,
		params: {
			...header.params,
			payload: bytes.slice(headerEnd).buffer
		}
	};
}

export function processConfigs(commands: CommandParam[]) {
	return commands.map((e, id: number) => ({
		id: `${id}`,
		text: e.name,
		topic: e.topic,
		payload: e.payload
	}));
}

export function processBrokerRemoval(params: string, app: AppState) {
	if (app.brokerRepository[params]) {
		if (app.selectedBroker === params) {
			app.selectedBroker = '';
			app.selectedTopic = null;
		}
		delete app.brokerRepository[params];
		delete rateWindows[params];
		clearTopicRates(params);
	}

	return app;
}

export function processConnectionStatus(params: MqttConnectionStatus, app: AppState) {
	if (!app.brokerRepository[params.source]) {
		return app;
	}
	app.brokerRepository[params.source].connected = params.connected;
	if (params.protocol) {
		app.brokerRepository[params.source].protocol = params.protocol;
	}

	return app;
}

function ensureBrokerEntry(
	brokerRepository: BrokerRepository,
	broker: string
): BrokerRepositoryEntry {
	if (!brokerRepository[broker]) {
		brokerRepository[broker] = {
			topics: [],
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
	return brokerRepository[broker];
}

export function processBrokers(params: BrokerParam, brokerRepository: BrokerRepository) {
	// `mqtt_brokers` is a full authoritative snapshot on every (re)connect, so
	// drop any entries the backend no longer knows about (e.g. removed while
	// this client was disconnected and missed the `broker_removal` event).
	const current = new Set(params.map((param) => param.broker));
	for (const broker of Object.keys(brokerRepository)) {
		if (!current.has(broker)) {
			delete brokerRepository[broker];
			delete rateWindows[broker];
		}
	}

	params.forEach((param) => {
		const requiresAuth = param.requires_auth ?? false;
		if (!brokerRepository[param.broker]) {
			brokerRepository[param.broker] = {
				topics: [],
				selectedTopic: null,
				...newLayout(),
				pipeline: [],
				connected: param.connected,
				backendTotalBytes: param.total_bytes ?? 0,
				backendTotalMessages: param.total_messages ?? 0,
				bytesPerSecond: 0,
				messagesPerSecond: 0,
				rateHistory: (param.rate_history ?? []).map((e) => ({
					timestamp: e.timestamp,
					bytesPerSecond: e.bytes_per_second,
					totalBytes: e.total_bytes
				})),
				requiresAuth,
				authenticated: false,
				protocol: param.protocol ?? undefined
			};
		} else {
			brokerRepository[param.broker].connected = param.connected;
			brokerRepository[param.broker].requiresAuth = requiresAuth;
			brokerRepository[param.broker].protocol =
				param.protocol ?? brokerRepository[param.broker].protocol;
			brokerRepository[param.broker].backendTotalBytes =
				param.total_bytes ?? brokerRepository[param.broker].backendTotalBytes;
			brokerRepository[param.broker].backendTotalMessages =
				param.total_messages ?? brokerRepository[param.broker].backendTotalMessages;
			// Update rate history from backend (authoritative source)
			if (param.rate_history) {
				brokerRepository[param.broker].rateHistory = param.rate_history.map((e) => ({
					timestamp: e.timestamp,
					bytesPerSecond: e.bytes_per_second,
					totalBytes: e.total_bytes
				}));
			}
		}
	});

	return brokerRepository;
}

/// Process topic summaries from the backend (initial sync).
/// Builds the topic tree structure with correct counts but no message content.
export function processTopicSummaries(
	params: TopicSummariesParam,
	brokerRepository: BrokerRepository
) {
	const entry = ensureBrokerEntry(brokerRepository, params.source);

	for (const [topic, info] of Object.entries(params.topics)) {
		entry.topics = addToTopicTreeMeta(params.source, topic, entry.topics, info.count);
	}

	return brokerRepository;
}

function addToPipeline(
	source: string,
	topic: string,
	timestamp: string,
	brokerRepository: BrokerRepository
) {
	const pipeline = brokerRepository[source]?.pipeline;
	const index = pipeline.findIndex((e) => !e.timestamp);
	if (index === -1 || pipeline[index].topic !== topic) {
		return;
	}
	pipeline[index].timestamp = timestamp;
	pipeline[index].topic = topic;

	if (index === 0) {
		pipeline[index].delta_t = 0;
	} else {
		const prevMessage = pipeline[index - 1];
		const nextMessage = pipeline[index];
		const prevTimestamp = new Date(prevMessage.timestamp!).getTime();
		const nextTimestamp = new Date(nextMessage.timestamp!).getTime();
		pipeline[index].delta_t = nextTimestamp - prevTimestamp;
	}
}

/** "12 msg/s · 4.8 KB/s"; slow topics show msg/min, bytes stay per second. */
function formatTopicRate(rate: TopicRate): string {
	return `${formatMsgRate(rate.messagesPerSecond)} · ${formatRate(rate.bytesPerSecond)}`;
}

function createTreeBranchEntryText(branch: Treebranch, source: string, now: number = Date.now()) {
	const text = branch.original_text;

	// Rates are tracked cumulatively per path prefix, so the topic's own rate is
	// its total minus what its subtopics account for.
	// Children are measured over the same window as the branch so the
	// subtraction lines up.
	const total = topicRate(source, branch.id, now);
	const children = (branch.children ?? []).reduce(
		(sum, c) => {
			const r = topicRate(source, c.id, now, total.window);
			sum.bytesPerSecond += r.bytesPerSecond;
			sum.messagesPerSecond += r.messagesPerSecond;
			return sum;
		},
		{ bytesPerSecond: 0, messagesPerSecond: 0 }
	);
	const ownMessagesPerSecond = total.messagesPerSecond - children.messagesPerSecond;
	const own: TopicRate =
		ownMessagesPerSecond > 1e-6
			? {
					bytesPerSecond: Math.max(0, total.bytesPerSecond - children.bytesPerSecond),
					messagesPerSecond: ownMessagesPerSecond,
					window: total.window
				}
			: { bytesPerSecond: 0, messagesPerSecond: 0, window: total.window };

	const parts: string[] = [];

	if (own.messagesPerSecond > 0) parts.push(formatTopicRate(own));

	if (branch.children?.length) {
		parts.push(`${branch.children.length} subtopic${branch.children.length > 1 ? 's' : ''}`);
		// Cumulative rate of the whole branch (own messages + all subtopics).
		if (children.messagesPerSecond > 0) parts.push(`${formatTopicRate(total)} total`);
	}

	if (!parts.length) return text;

	return `${text} (${parts.join(', ')})`;
}

/** Walk the topic tree and increment number_of_messages by `count` along the path.
 *  Creates nodes as needed. Does NOT add message content. */
function addToTopicBranchMeta(
	source: string,
	topicsplit: string[],
	index: number,
	topicbranch: Treebranch[] | undefined,
	count: number = 1
) {
	const key = topicsplit[index];
	let found = topicbranch?.find((element) => element.original_text === key);

	if (index === topicsplit.length) {
		return;
	}

	if (found) {
		found.children = found.children || [];
		found.number_of_messages += count;
	} else {
		found = {
			id: topicsplit.slice(0, index + 1).join('/'),
			text: key,
			children: [],
			original_text: key,
			number_of_messages: count,
			messages: []
		};
		topicbranch?.push(found);
	}
	addToTopicBranchMeta(source, topicsplit, index + 1, found.children, count);

	if (found.children?.length === 0) {
		found.children = undefined;
	}

	found.text = createTreeBranchEntryText(found, source);

	return topicbranch;
}

function addToTopicTreeMeta(
	source: string,
	topic: string,
	topictree: Treebranch[],
	count: number = 1
): Treebranch[] {
	const branch = topic.split('/');
	return addToTopicBranchMeta(source, branch, 0, topictree, count) || [];
}

/** Walk the topic tree and decrement number_of_messages along the path. */
function decrementTopicTreeCounts(
	source: string,
	topicsplit: string[],
	index: number,
	topicbranch: Treebranch[] | undefined,
	count: number
) {
	if (!topicbranch || index === topicsplit.length) return;
	const key = topicsplit[index];
	const found = topicbranch.find((element) => element.original_text === key);
	if (!found) return;

	found.number_of_messages = Math.max(0, found.number_of_messages - count);

	decrementTopicTreeCounts(source, topicsplit, index + 1, found.children, count);

	// Remove empty leaf nodes
	if (found.number_of_messages === 0 && !found.children?.length) {
		const idx = topicbranch.indexOf(found);
		if (idx !== -1) topicbranch.splice(idx, 1);
	} else {
		found.text = createTreeBranchEntryText(found, source);
	}
}

/** Process a lightweight meta notification about a new MQTT message.
 *  Updates the topic tree counts but does NOT add message content. */
export function processMQTTMessageMeta(message: MQTTMessageMetaParam, app: AppState) {
	const entry = ensureBrokerEntry(app.brokerRepository, message.source);
	entry.connected = true;

	if (!app.selectedBroker) {
		app.selectedBroker = message.source;
	}

	// Update topic tree counts and per-topic rates (record first so the new
	// labels already include this message)
	recordTopicMessage(message.source, message.topic, message.payload_size);
	entry.topics = addToTopicTreeMeta(message.source, message.topic, entry.topics);

	// Update selected topic reference only when this message affects selected broker.
	if (
		app.selectedBroker === message.source &&
		app.brokerRepository[app.selectedBroker]?.selectedTopic
	) {
		const sel = app.brokerRepository[app.selectedBroker].selectedTopic;
		if (sel) {
			app.brokerRepository[app.selectedBroker].selectedTopic =
				findbranchwithid(sel.id.toString(), app.brokerRepository[app.selectedBroker].topics) || sel;
		}
	}

	// Update backend total bytes and rates
	entry.backendTotalBytes = message.total_bytes;
	entry.backendTotalMessages += 1;
	const rates = updateRates(message.source, message.payload_size);
	entry.bytesPerSecond = rates.bytesPerSecond;
	entry.messagesPerSecond = rates.messagesPerSecond;

	// Pipeline tracking
	addToPipeline(message.source, message.topic, message.timestamp, app.brokerRepository);

	return app;
}

/** Keep every pane's locked selection on its pinned message after the broker's
 *  message lists changed. Runs in the same step that mutates the lists, so no
 *  component ever renders a stale index. */
function followLockedMessages(entry: BrokerRepositoryEntry) {
	for (const group of allGroups(entry.layout)) {
		if (!group.activeTopicId) continue;
		const branch = findbranchwithid(group.activeTopicId, entry.topics);
		if (branch) followMessages(group.view, branch.messages);
	}
}

/** Apply one eviction notification; returns the affected broker entry. */
function evictMessages(params: MessagesEvictedParam, app: AppState) {
	const entry = app.brokerRepository[params.source];
	if (!entry) return undefined;
	entry.backendTotalMessages = Math.max(0, entry.backendTotalMessages - params.count);

	// Decrement topic tree counts
	const parts = params.topic.split('/');
	decrementTopicTreeCounts(params.source, parts, 0, entry.topics, params.count);

	// Trim the evicted topic's cached messages (any open tab, not just the active
	// one) so inactive tab caches stay bounded. Messages are newest-first, so the
	// oldest are at the end.
	const evictedLeaf = findLeafBranch(entry.topics, params.topic);
	if (evictedLeaf) {
		for (let i = 0; i < params.count && evictedLeaf.messages.length > 0; i++) {
			evictedLeaf.messages.pop(); // remove oldest (at the end)
		}
	}

	// Update selected topic reference
	if (entry.selectedTopic) {
		entry.selectedTopic =
			findbranchwithid(entry.selectedTopic.id.toString(), entry.topics) || entry.selectedTopic;
	}

	return entry;
}

/** Process eviction notifications from the backend. */
export function processMessagesEvicted(params: MessagesEvictedParam, app: AppState) {
	const entry = evictMessages(params, app);
	if (entry) followLockedMessages(entry);
	return app;
}

/** Recompute the tree labels so per-topic rates keep updating (and drop back
 *  to nothing) while a topic is quiet. Returns true if any label changed. */
export function refreshTopicTreeRates(app: AppState, now: number = Date.now()): boolean {
	let changed = false;
	const walk = (source: string, branches: Treebranch[] | undefined) => {
		if (!branches) return;
		for (const branch of branches) {
			const text = createTreeBranchEntryText(branch, source, now);
			if (text !== branch.text) {
				branch.text = text;
				changed = true;
			}
			walk(source, branch.children);
		}
	};
	for (const [source, entry] of Object.entries(app.brokerRepository)) {
		walk(source, entry.topics);
	}
	return changed;
}

/** Process a batch of meta notifications (sent by the backend every ~100 ms). */
export function processMQTTMessageMetaBatch(messages: MQTTMessageMetaParam[], app: AppState) {
	for (const message of messages) {
		processMQTTMessageMeta(message, app);
	}
	return app;
}

/** Process a batch of eviction notifications (sent by the backend every ~100 ms). */
export function processMessagesEvictedBatch(items: MessagesEvictedParam[], app: AppState) {
	const touched = new Set<BrokerRepositoryEntry>();
	for (const item of items) {
		const entry = evictMessages(item, app);
		if (entry) touched.add(entry);
	}
	touched.forEach(followLockedMessages);
	return app;
}

/** Process full message content for the selected topics (bulk-inserted per group). */
export function processMQTTMessages(messages: MQTTMessageParam[], app: AppState) {
	// Group messages by source+topic for bulk insertion
	const groups = new Map<string, { leaf: Treebranch; entries: Message[] }>();

	for (const message of messages) {
		const entry = app.brokerRepository[message.source];
		if (!entry) continue;

		const leaf = findLeafBranch(entry.topics, message.topic);
		if (!leaf) continue;

		const key = `${message.source}\0${message.topic}`;
		let group = groups.get(key);
		if (!group) {
			group = { leaf, entries: [] };
			groups.set(key, group);
		}
		group.entries.push(
			new Message(
				message.timestamp,
				message.payload,
				null,
				message.retain ?? false,
				message.original_payload_size,
				message.properties ?? null
			)
		);
	}

	// Bulk-insert each group with a single splice (avoids O(n²) per-message unshift)
	for (const { leaf, entries } of groups.values()) {
		leaf.messages = mergeMessagesNewestFirst(leaf.messages, sortMessagesNewestFirst(entries));
	}

	// Locked selections follow their pinned message in the same step.
	for (const source of new Set(messages.map((m) => m.source))) {
		const entry = app.brokerRepository[source];
		if (entry) followLockedMessages(entry);
	}

	// Update selectedTopic reference once per broker (not per message)
	const updatedBrokers = new Set<string>();
	for (const message of messages) {
		if (updatedBrokers.has(message.source)) continue;
		updatedBrokers.add(message.source);
		const entry = app.brokerRepository[message.source];
		if (entry?.selectedTopic) {
			entry.selectedTopic =
				findbranchwithid(entry.selectedTopic.id.toString(), entry.topics) || entry.selectedTopic;
		}
	}

	return app;
}

export function processPipelines(params: PipelineParam[]) {
	return params.map((e, id: number) => ({
		id,
		text: e.name,
		pipeline: e.pipeline
	}));
}

export type SettingsParam = { max_broker_bytes: number; max_message_size: number };

export function processSettings(params: SettingsParam, app: AppState) {
	app.maxBrokerBytes = params.max_broker_bytes;
	return app;
}

export type RateHistorySampleParam = {
	source: string;
	sample: { timestamp: number; bytes_per_second: number; total_bytes: number };
};

/** Maximum age for rate history entries: 7 days in milliseconds. */
const RATE_HISTORY_MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;

export function processRateHistorySample(params: RateHistorySampleParam, app: AppState) {
	const entry = app.brokerRepository[params.source];
	if (!entry) return app;
	const newEntry = {
		timestamp: params.sample.timestamp,
		bytesPerSecond: params.sample.bytes_per_second,
		totalBytes: params.sample.total_bytes
	};
	entry.bytesPerSecond = params.sample.bytes_per_second;
	// Prune entries older than 7 days incrementally to keep per-sample work low.
	const cutoff = Date.now() - RATE_HISTORY_MAX_AGE_MS;
	while (entry.rateHistory.length > 0 && entry.rateHistory[0].timestamp < cutoff) {
		entry.rateHistory.shift();
	}
	entry.rateHistory.push(newEntry);
	return app;
}

export function processSession(params: SessionParam): Session {
	return {
		mode: params.mode === 'claims' ? 'claims' : 'password',
		user: params.user || undefined,
		// Only an explicit `false` takes edit rights away.
		canEdit: params.can_edit !== false
	};
}
