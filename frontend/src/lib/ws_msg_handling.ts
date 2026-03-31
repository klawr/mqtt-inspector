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

import { findbranchwithid } from './helper';
import { Message } from './state';
import type { AppState, BrokerRepository, BrokerRepositoryEntry, Treebranch } from './state';

/** Sliding-window rate tracker: per-broker list of (timestamp_ms, byteCount) samples. */
const rateWindows: Record<string, { t: number; b: number }[]> = {};
const RATE_WINDOW_MS = 3000;

function updateRates(
	broker: string,
	bytesReceived: number
): { bytesPerSecond: number; messagesPerSecond: number } {
	const now = Date.now();
	if (!rateWindows[broker]) rateWindows[broker] = [];
	rateWindows[broker].push({ t: now, b: bytesReceived });
	// prune entries older than the window
	const cutoff = now - RATE_WINDOW_MS;
	const idx = rateWindows[broker].findIndex((e) => e.t >= cutoff);
	if (idx > 0) {
		rateWindows[broker].splice(0, idx);
	} else if (idx === -1) {
		rateWindows[broker].length = 0;
	}
	const totalBytes = rateWindows[broker].reduce((sum, e) => sum + e.b, 0);
	const seconds = RATE_WINDOW_MS / 1000;
	return {
		bytesPerSecond: totalBytes / seconds,
		messagesPerSecond: rateWindows[broker].length / seconds
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

function getTimestampMs(timestamp: string): number {
	return new Date(timestamp).getTime();
}

function sortMessagesNewestFirst<T extends { timestamp: string }>(messages: T[]): T[] {
	return messages.sort(
		(left, right) => getTimestampMs(right.timestamp) - getTimestampMs(left.timestamp)
	);
}

function mergeMessagesNewestFirst(existing: Message[], incoming: Message[]): Message[] {
	const merged: Message[] = [];
	let incomingIndex = 0;
	let existingIndex = 0;

	while (incomingIndex < incoming.length && existingIndex < existing.length) {
		if (
			getTimestampMs(incoming[incomingIndex].timestamp) >=
			getTimestampMs(existing[existingIndex].timestamp)
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
}[];
export type MqttConnectionStatus = { source: string; connected: boolean };
export type PipelineParam = { id: string; name: string; pipeline: { topic: string }[] };
export type MQTTMessageParam = {
	source: string;
	topic: string;
	payload: ArrayBuffer;
	timestamp: string;
	total_bytes?: number;
	retain?: boolean;
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
	}

	return app;
}

export function processConnectionStatus(params: MqttConnectionStatus, app: AppState) {
	if (!app.brokerRepository[params.source]) {
		return app;
	}
	app.brokerRepository[params.source].connected = params.connected;

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
	params.forEach((param) => {
		const requiresAuth = param.requires_auth ?? false;
		if (!brokerRepository[param.broker]) {
			brokerRepository[param.broker] = {
				topics: [],
				selectedTopic: null,
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
				authenticated: false
			};
		} else {
			brokerRepository[param.broker].connected = param.connected;
			brokerRepository[param.broker].requiresAuth = requiresAuth;
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
		entry.topics = addToTopicTreeMeta(topic, entry.topics, info.count);
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

function createTreeBranchEntryText(branch: Treebranch) {
	let text = branch.original_text;

	if (!branch.children?.length && !branch.number_of_messages) {
		return text;
	}

	text += ' (';

	const leafMessages =
		branch.number_of_messages -
		(branch.children ?? []).reduce((sum, c) => sum + c.number_of_messages, 0);

	if (leafMessages > 0) {
		text += `${leafMessages} message${leafMessages > 1 ? 's' : ''}`;
		if (branch.children?.length) {
			text += ', ';
		}
	}

	if (branch.children?.length) {
		const childMessages = branch.number_of_messages - leafMessages;
		text += `${branch.children.length} subtopic${branch.children.length > 1 ? 's' : ''} with ${childMessages} message${childMessages > 1 ? 's' : ''}`;
	}

	text += ')';

	return text;
}

/** Walk the topic tree and increment number_of_messages by `count` along the path.
 *  Creates nodes as needed. Does NOT add message content. */
function addToTopicBranchMeta(
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
			text: key + ` (${count} message${count > 1 ? 's' : ''})`,
			children: [],
			original_text: key,
			number_of_messages: count,
			messages: []
		};
		topicbranch?.push(found);
	}
	addToTopicBranchMeta(topicsplit, index + 1, found.children, count);

	if (found.children?.length === 0) {
		found.children = undefined;
	}

	found.text = createTreeBranchEntryText(found);

	return topicbranch;
}

function addToTopicTreeMeta(
	topic: string,
	topictree: Treebranch[],
	count: number = 1
): Treebranch[] {
	const branch = topic.split('/');
	return addToTopicBranchMeta(branch, 0, topictree, count) || [];
}

/** Walk the topic tree and decrement number_of_messages along the path. */
function decrementTopicTreeCounts(
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

	decrementTopicTreeCounts(topicsplit, index + 1, found.children, count);

	// Remove empty leaf nodes
	if (found.number_of_messages === 0 && !found.children?.length) {
		const idx = topicbranch.indexOf(found);
		if (idx !== -1) topicbranch.splice(idx, 1);
	} else {
		found.text = createTreeBranchEntryText(found);
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

	// Update topic tree counts
	entry.topics = addToTopicTreeMeta(message.topic, entry.topics);

	// Update the selected topic reference if it belongs to this broker
	if (app.brokerRepository[app.selectedBroker]?.selectedTopic) {
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

/** Process eviction notifications from the backend. */
export function processMessagesEvicted(params: MessagesEvictedParam, app: AppState) {
	const entry = app.brokerRepository[params.source];
	if (!entry) return app;
	entry.backendTotalMessages = Math.max(0, entry.backendTotalMessages - params.count);

	// Decrement topic tree counts
	const parts = params.topic.split('/');
	decrementTopicTreeCounts(parts, 0, entry.topics, params.count);

	// If the evicted topic is the currently selected one, remove oldest messages
	const selectedTopic = entry.selectedTopic;
	if (selectedTopic && selectedTopic.id === params.topic) {
		for (let i = 0; i < params.count && selectedTopic.messages.length > 0; i++) {
			selectedTopic.messages.pop(); // remove oldest (at the end)
		}
	}

	// Update selected topic reference
	if (entry.selectedTopic) {
		entry.selectedTopic =
			findbranchwithid(entry.selectedTopic.id.toString(), entry.topics) || entry.selectedTopic;
	}

	return app;
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
	for (const item of items) {
		processMessagesEvicted(item, app);
	}
	return app;
}

/** Process full message content for the selected topic.
 *  Does NOT update tree counts (meta handles that). Only adds message content. */
export function processMQTTMessage(message: MQTTMessageParam, app: AppState) {
	if (!app.brokerRepository[message.source]) {
		return app;
	}

	const entry = app.brokerRepository[message.source];

	// Find the leaf node and add message content
	const leaf = findLeafBranch(entry.topics, message.topic);
	if (leaf) {
		const new_entry = new Message(
			message.timestamp,
			message.payload,
			null,
			message.retain ?? false
		);
		leaf.messages = mergeMessagesNewestFirst(sortMessagesNewestFirst([...leaf.messages]), [
			new_entry
		]);
	}

	// Update the selectedTopic reference
	if (entry.selectedTopic) {
		entry.selectedTopic =
			findbranchwithid(entry.selectedTopic.id.toString(), entry.topics) || entry.selectedTopic;
	}

	return app;
}

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
			new Message(message.timestamp, message.payload, null, message.retain ?? false)
		);
	}

	// Bulk-insert each group with a single splice (avoids O(n²) per-message unshift)
	for (const { leaf, entries } of groups.values()) {
		leaf.messages = mergeMessagesNewestFirst(
			sortMessagesNewestFirst([...leaf.messages]),
			sortMessagesNewestFirst(entries)
		);
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

/** Clear all message content from the selected topic's tree (before re-sync). */
export function processTopicMessagesClear(app: AppState) {
	// Clear messages from all leaf nodes for the current broker
	if (app.selectedBroker && app.brokerRepository[app.selectedBroker]) {
		const entry = app.brokerRepository[app.selectedBroker];
		if (entry.selectedTopic) {
			clearMessages(entry.selectedTopic);
		}
	}
	return app;
}

function clearMessages(branch: Treebranch) {
	branch.messages = [];
	if (branch.children) {
		for (const child of branch.children) {
			clearMessages(child);
		}
	}
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
	// Prune entries older than 7 days and create a new array for Svelte reactivity
	const cutoff = Date.now() - RATE_HISTORY_MAX_AGE_MS;
	entry.rateHistory = [...entry.rateHistory.filter((e) => e.timestamp >= cutoff), newEntry];
	return app;
}
