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

export function requestCommandAddition(
	save_command_name: string,
	topic: string,
	payload: string,
	socket: WebSocket
) {
	const message = JSON.stringify({
		jsonrpc: '2.0',
		method: 'save_command',
		params: { name: save_command_name, topic, payload }
	});
	socket.send(message);
}

export function requestPipelineAddition(
	pipelineName: string,
	newPipeline: { topic: string }[],
	socket: WebSocket
) {
	const message = JSON.stringify({
		jsonrpc: '2.0',
		method: 'save_pipeline',
		params: {
			name: pipelineName,
			pipeline: newPipeline
		}
	});

	socket.send(message);
}

export function requestMqttBrokerRemoval(hostname: string, socket: WebSocket) {
	const message = JSON.stringify({
		jsonrpc: '2.0',
		method: 'remove',
		params: { hostname }
	});
	socket.send(message);
}

export function requestPipelineRemoval(pipeline: string, socket: WebSocket) {
	const message = JSON.stringify({
		jsonrpc: '2.0',
		method: 'remove_pipeline',
		params: { name: pipeline }
	});
	socket.send(message);
}

export function requestMqttBrokerConnection(
	hostname: string,
	socket: WebSocket,
	options?: { use_tls?: boolean; username?: string; password?: string }
) {
	const params: Record<string, unknown> = { hostname };
	if (options?.use_tls) params.use_tls = true;
	if (options?.username) params.username = options.username;
	if (options?.password) params.password = options.password;
	const message = JSON.stringify({
		jsonrpc: '2.0',
		method: 'connect',
		params
	});
	socket.send(message);
}

export function requestPublishMqttMessage(
	host: string,
	topic: string,
	payload: string,
	retain: boolean,
	socket: WebSocket
) {
	const message = JSON.stringify({
		jsonrpc: '2.0',
		method: 'publish',
		params: { host, topic, payload, retain }
	});

	socket.send(message);
}

export function requestTopicSelection(
	broker: string | null,
	topic: string | null,
	socket: WebSocket
) {
	const message = JSON.stringify({
		jsonrpc: '2.0',
		method: 'select_topic',
		params: { broker, topic }
	});

	socket.send(message);
}

export function requestBrokerAuthentication(hostname: string, password: string, socket: WebSocket) {
	const message = JSON.stringify({
		jsonrpc: '2.0',
		method: 'authenticate_broker',
		params: { hostname, password }
	});

	socket.send(message);
}
