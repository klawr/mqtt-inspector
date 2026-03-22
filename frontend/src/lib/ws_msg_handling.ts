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
import type { AppState, BrokerRepository, BrokerRepositoryEntry, Treebranch } from './state';

let maxBrokerBytes = 64 * 1024 * 1024; // default, overridden by settings from backend

/** Sliding-window rate tracker: per-broker list of (timestamp_ms, byteCount) samples. */
const rateWindows: Record<string, { t: number; b: number }[]> = {};
const RATE_WINDOW_MS = 3000;

function updateBytesPerSecond(broker: string, bytesReceived: number): number {
	const now = Date.now();
	if (!rateWindows[broker]) rateWindows[broker] = [];
	rateWindows[broker].push({ t: now, b: bytesReceived });
	// prune entries older than the window
	const cutoff = now - RATE_WINDOW_MS;
	while (rateWindows[broker].length > 0 && rateWindows[broker][0].t < cutoff) {
		rateWindows[broker].shift();
	}
	const totalBytes = rateWindows[broker].reduce((sum, e) => sum + e.b, 0);
	return totalBytes / (RATE_WINDOW_MS / 1000);
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

function evictUntilUnderBudget(entry: BrokerRepositoryEntry) {
	while (entry.totalBytes > maxBrokerBytes) {
		if (entry.evictionHead >= entry.evictionOrder.length) break;
		const item = entry.evictionOrder[entry.evictionHead];
		entry.evictionHead++;
		const leaf = findLeafBranch(entry.topics, item.topic);
		if (leaf && leaf.messages.length > 0) {
			leaf.messages.pop();
			entry.totalBytes -= item.payloadLen;
		}
	}
	// Compact the eviction queue when the consumed head is large
	if (entry.evictionHead > 10000) {
		entry.evictionOrder = entry.evictionOrder.slice(entry.evictionHead);
		entry.evictionHead = 0;
	}
}

export type Command = { id: string; text: string; topic: string; payload: string };

export type CommandParam = { id: string; name: string; topic: string; payload: string };
export type BrokerParam = {
	broker: string;
	connected: boolean;
	topics: {
		[key: string]: { payload: ArrayBuffer; timestamp: string }[];
	};
	total_bytes?: number;
	rate_history?: { timestamp: number; bytes_per_second: number; total_bytes: number }[];
}[];
export type MqttConnectionStatus = { source: string; connected: boolean };
type PipelineParamEntry = { topic: string };
export type PipelineParam = { id: string; name: string; pipeline: PipelineParamEntry[] };
export type MQTTMessageParam = {
	source: string;
	topic: string;
	payload: ArrayBuffer;
	timestamp: string;
	total_bytes?: number;
};

function decodePayloadBytes(message: { payload: ArrayBuffer }): Uint8Array {
	return new Uint8Array(message.payload);
}

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

export function processBrokers(
	params: BrokerParam,
	decoder: TextDecoder,
	brokerRepository: BrokerRepository
) {
	params.forEach((param) => {
		if (!brokerRepository[param.broker]) {
			brokerRepository[param.broker] = {
				topics: [],
				selectedTopic: null,
				pipeline: [],
				connected: param.connected,
				totalBytes: 0,
				backendTotalBytes: param.total_bytes ?? 0,
				bytesPerSecond: 0,
				rateHistory: (param.rate_history ?? []).map((e) => ({
					timestamp: e.timestamp,
					bytesPerSecond: e.bytes_per_second,
					totalBytes: e.total_bytes
				})),
				evictionOrder: [],
				evictionHead: 0
			};
		} else {
			brokerRepository[param.broker].connected = param.connected;
			brokerRepository[param.broker].backendTotalBytes =
				param.total_bytes ?? brokerRepository[param.broker].backendTotalBytes;
			// Update rate history from backend (authoritative source)
			if (param.rate_history) {
				brokerRepository[param.broker].rateHistory = param.rate_history.map((e) => ({
					timestamp: e.timestamp,
					bytesPerSecond: e.bytes_per_second,
					totalBytes: e.total_bytes
				}));
			}
		}

		for (const topic of Object.keys(param.topics)) {
			for (const message of param.topics[topic]) {
				const decoded = decoder.decode(decodePayloadBytes(message));
				brokerRepository[param.broker].topics = addToTopicTree(
					topic,
					brokerRepository[param.broker].topics,
					decoded,
					message.timestamp
				);
				brokerRepository[param.broker].totalBytes += decoded.length;
				brokerRepository[param.broker].evictionOrder.push({ topic, payloadLen: decoded.length });
			}
		}

		evictUntilUnderBudget(brokerRepository[param.broker]);
	});

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

	if (!branch.children?.length && !branch.messages.length) {
		return text;
	}

	text += ' (';

	if (branch.messages.length) {
		text += `${branch.messages.length} message${branch.messages.length > 1 ? 's' : ''}`;
		if (branch.children?.length) {
			text += ', ';
		}
	}

	if (branch.children?.length) {
		const number_of_messages = branch.number_of_messages - branch.messages.length;
		text += `${branch.children.length} subtopic${branch.children.length > 1 ? 's' : ''} with ${number_of_messages} message${number_of_messages > 1 ? 's' : ''}`;
	}

	text += ')';

	return text;
}

function addToTopicBranch(
	topicsplit: string[],
	index: number,
	topicbranch: Treebranch[] | undefined,
	payload: string,
	timestamp: string
) {
	const key = topicsplit[index];
	let found = topicbranch?.find((element) => element.original_text === key);

	if (index === topicsplit.length) {
		return;
	}

	if (found) {
		found.children = found.children || [];
		found.number_of_messages += 1;
	} else {
		found = {
			id: topicsplit.slice(0, index + 1).join('/'),
			text: key + ' (1 message)',
			children: [],
			original_text: key,
			number_of_messages: 1,
			messages: []
		};
		topicbranch?.push(found);
	}
	addToTopicBranch(topicsplit, index + 1, found.children, payload, timestamp);

	if (found.children?.length === 0) {
		found.children = undefined;
	}

	if (index === topicsplit.length - 1) {
		const ff = found || topicbranch?.find((element) => element.original_text === key);
		const new_entry = { timestamp: timestamp, text: payload, delta_t: 0 };
		if (ff?.messages.length) {
			ff.messages[0].delta_t =
				new Date(timestamp).getTime() - new Date(ff.messages[0].timestamp).getTime();
		}
		new_entry.delta_t = 0;

		ff?.messages.unshift(new_entry);
	}
	found.text = createTreeBranchEntryText(found);

	return topicbranch;
}

function addToTopicTree(
	topic: string,
	topictree: Treebranch[],
	payload: string,
	timestamp: string
): Treebranch[] {
	const branch = topic.split('/');

	return addToTopicBranch(branch, 0, topictree, payload, timestamp) || [];
}

export function processMQTTMessage(message: MQTTMessageParam, decoder: TextDecoder, app: AppState) {
	if (!app.brokerRepository[message.source]) {
		app.brokerRepository[message.source] = {
			topics: [],
			selectedTopic: null,
			pipeline: [],
			connected: true,
			totalBytes: 0,
			backendTotalBytes: 0,
			bytesPerSecond: 0,
			rateHistory: [],
			evictionOrder: [],
			evictionHead: 0
		};
	}
	app.brokerRepository[message.source].connected = true;

	if (!app.selectedBroker) {
		app.selectedBroker = message.source;
	}

	const payload = decoder.decode(decodePayloadBytes(message));
	app.brokerRepository[message.source].topics = addToTopicTree(
		message.topic,
		app.brokerRepository[message.source].topics,
		payload,
		message.timestamp
	);
	if (app.selectedTopic) {
		app.selectedTopic =
			findbranchwithid(
				app.selectedTopic?.id.toString(),
				app.brokerRepository[message.source].topics
			) || app.selectedTopic;
	}

	addToPipeline(message.source, message.topic, message.timestamp, app.brokerRepository);

	app.brokerRepository[message.source].totalBytes += payload.length;
	app.brokerRepository[message.source].evictionOrder.push({
		topic: message.topic,
		payloadLen: payload.length
	});
	if (message.total_bytes !== undefined) {
		app.brokerRepository[message.source].backendTotalBytes = message.total_bytes;
	}
	app.brokerRepository[message.source].bytesPerSecond = updateBytesPerSecond(
		message.source,
		payload.length
	);
	evictUntilUnderBudget(app.brokerRepository[message.source]);

	return app;
}

export function processMQTTMessages(
	messages: MQTTMessageParam[],
	decoder: TextDecoder,
	app: AppState
) {
	for (const message of messages) {
		processMQTTMessage(message, decoder, app);
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
	maxBrokerBytes = params.max_broker_bytes;
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

export function processSyncComplete(app: AppState) {
	app.syncComplete = true;
	return app;
}
