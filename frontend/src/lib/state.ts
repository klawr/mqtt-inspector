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

export class AppState {
	selectedBroker: string = '';
	selectedTopic: Treebranch | null = null;
	brokerRepository: BrokerRepository = {};
	pipelines: SavedPipeline[] = [];
	commands: Command[] = [];
	maxBrokerBytes: number = 64 * 1024 * 1024;
}

const decoder = new TextDecoder();

export class Message {
	timestamp: string;
	retain: boolean;
	private _payload: ArrayBuffer | null;
	private _text: string | null;

	constructor(
		timestamp: string,
		payload: ArrayBuffer | null,
		text: string | null,
		retain: boolean = false
	) {
		this.timestamp = timestamp;
		this.retain = retain;
		this._payload = payload;
		this._text = text;
	}

	get text(): string {
		if (this._text === null) {
			this._text = this._payload ? decoder.decode(new Uint8Array(this._payload)) : '';
			this._payload = null; // free the raw buffer
		}
		return this._text;
	}

	set text(value: string) {
		this._text = value;
		this._payload = null;
	}
}

export type Treebranch = {
	id: string;
	text: string;
	children?: Treebranch[];
	original_text: string;
	number_of_messages: number;
	messages: Message[];
};

type PipelineEntry = {
	topic: string;
};
export type SavedPipeline = {
	id: number;
	text: string;
	pipeline: PipelineEntry[];
};

export type Command = {
	id: string;
	text: string;
	topic: string;
	payload: string;
};

export type RateHistoryEntry = {
	timestamp: number;
	bytesPerSecond: number;
	totalBytes: number;
};

export type BrokerRepositoryEntry = {
	topics: Treebranch[];
	selectedTopic: Treebranch | null;
	pipeline: { topic: string; timestamp?: string; delta_t?: number }[];
	connected: boolean;
	backendTotalBytes: number;
	backendTotalMessages: number;
	bytesPerSecond: number;
	messagesPerSecond: number;
	rateHistory: RateHistoryEntry[];
	requiresAuth: boolean;
	authenticated: boolean;
};

export type BrokerRepository = {
	[key: string]: BrokerRepositoryEntry;
};
