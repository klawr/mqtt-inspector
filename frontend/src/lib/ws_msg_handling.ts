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

import { findbranchwithid } from "./helper";
import type { AppState, BrokerRepository, Treebranch } from "./state";

export type Command = { id: string; text: string; topic: string; payload: string; };

export type CommandParam = { id: string; name: string; topic: string; payload: string; };
export type BrokerParam = string[];
export type MqttConnectionStatus = { source: string; connected: boolean; };
type PipelineParamEntry = { topic: string; };
export type PipelineParam = { id: string; name: string; pipeline: PipelineParamEntry[]; };
export type MQTTMessageParam = {
    source: string;
    topic: string;
    payload: ArrayBuffer;
    timestamp: string;
}

export function processConfigs(params: string) {
    const commands = JSON.parse(params) as CommandParam[];
    return commands.map((e, id: number) => ({
        id: `${id}`,
        text: e.name,
        topic: e.topic,
        payload: e.payload
    }));
}

export function processConnectionStatus(params: MqttConnectionStatus, app: AppState) {
    app.brokerRepository[params.source].connected = params.connected;

    return app;
}

export function processBrokers(params: BrokerParam, brokerRepository: BrokerRepository) {
    params.forEach((broker) => {
        if (!brokerRepository[broker]) {
            brokerRepository[broker] = { topics: [], selectedTopic: null, pipeline: [], connected: false };
        }
    });

    return brokerRepository;
}

function addToPipeline(source: string, topic: string, timestamp: string, brokerRepository: BrokerRepository) {
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

    text += " (";

    if (branch.messages.length) {
        text += `${branch.messages.length} message${branch.messages.length > 1 ? "s" : ""}`;
        if (branch.children?.length) {
            text += ", ";
        }
    }

    if (branch.children?.length) {
        const number_of_messages = branch.number_of_messages - branch.messages.length;
        text += `${branch.children.length} subtopic${branch.children.length > 1 ? "s" : ""} with ${number_of_messages} message${number_of_messages > 1 ? "s" : ""}`;

    }

    text += ")";

    return text;
}

function addToTopicBranch(
    topicsplit: string[],
    index: number,
    topicbranch: Treebranch[] | undefined,
    payload: string,
    timestamp: string) {
    const key = topicsplit[index];
    let found = topicbranch?.find((element) => element.original_text === key);

    if (index === topicsplit.length) {
        return;
    }

    if (found) {
        found.children = found.children || [];
        found.number_of_messages += 1;
    }
    else {
        found = {
            id: topicsplit.slice(0, index + 1).join("/"),
            text: key + ' (1 message)',
            children: [],
            original_text: key,
            number_of_messages: 1,
            messages: []
        }
        topicbranch?.push(found);
    }
    addToTopicBranch(topicsplit, index + 1, found.children, payload, timestamp);

    if (found.children?.length === 0) {
        found.children = undefined;
    }

    if (index === topicsplit.length - 1) {
        const ff = found || topicbranch?.find((element) => element.original_text === key);
        const new_entry = { timestamp: timestamp, text: payload, delta_t: 0 }
        if (ff?.messages.length) {
            new_entry.delta_t = new Date(timestamp).getTime() - new Date(ff.messages[0].timestamp).getTime();
        }
        ff?.messages.unshift(new_entry);
    }
    found.text = createTreeBranchEntryText(found);

    return topicbranch;
}

function addToTopicTree(
    topic: string,
    topictree: Treebranch[],
    payload: string,
    timestamp: string): Treebranch[] {
    const branch = topic.split('/');

    return addToTopicBranch(branch, 0, topictree, payload, timestamp) || [];
}


export function processMQTTMessage(
    message: MQTTMessageParam,
    decoder: TextDecoder,
    app: AppState) {

    if (!app.brokerRepository[message.source]) {
        app.brokerRepository[message.source] = { topics: [], selectedTopic: null, pipeline: [], connected: true };
    }
    app.brokerRepository[message.source].connected = true;

    if (app.selectedBroker === undefined) {
        app.selectedBroker = message.source;
    }

    const payload = decoder.decode(new Uint8Array(message.payload));
    app.brokerRepository[message.source].topics = addToTopicTree(
        message.topic,
        app.brokerRepository[message.source].topics,
        payload,
        message.timestamp
    );
    if (app.selectedTopic) {
        app.selectedTopic =
            findbranchwithid(app.selectedTopic?.id.toString(), app.brokerRepository[message.source].topics) ||
            app.selectedTopic;
    }

    addToPipeline(message.source, message.topic, message.timestamp, app.brokerRepository);

    return app;
}

export function processPipelines(params: PipelineParam[]) {
    return params.map((e, id: number) => ({
        id,
        text: e.name,
        pipeline: e.pipeline
    }));
}