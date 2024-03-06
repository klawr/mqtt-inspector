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
}

type Message = {
	timestamp: string;
	delta_t?: number;
	text: string;
};

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

export type BrokerRepositoryEntry = {
	markedForDeletion?: boolean;
	topics: Treebranch[];
	selectedTopic: Treebranch | null;
	pipeline: { topic: string; timestamp?: string; delta_t?: number }[];
	connected: boolean;
};

export type BrokerRepository = {
	[key: string]: BrokerRepositoryEntry;
};
