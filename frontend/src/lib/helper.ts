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

import type { Treebranch } from './state';

export type TopicSuggestion = {
	id: string;
	text: string;
};

export function findbranchwithid(
	id: string,
	tree: Treebranch[] | undefined
): Treebranch | undefined {
	if (!tree) return;

	for (let i = 0; i < tree.length; i++) {
		if (tree[i].id === id) {
			return tree[i];
		}
		if (tree[i].children) {
			const result = findbranchwithid(id, tree[i].children);
			if (result) {
				return result;
			}
		}
	}
}

export function getAllTopicIds(branch: Treebranch[], topicIds: string[] = []) {
	branch.forEach((topic) => {
		topicIds.push(topic.id);
		if (topic.children) {
			getAllTopicIds(topic.children, topicIds);
		}
	});
	return topicIds;
}

export function getTopicSuggestions(
	topicIds: string[],
	value: string | null | undefined,
	limit: number = 100
): TopicSuggestion[] {
	const needle = (value ?? '').trim().toLowerCase();
	if (limit <= 0) {
		return [];
	}

	if (!needle) {
		return topicIds.slice(0, limit).map((topicId) => ({ id: topicId, text: topicId }));
	}

	const prefixMatches: TopicSuggestion[] = [];
	const substringMatches: TopicSuggestion[] = [];

	for (const topicId of topicIds) {
		const normalizedTopicId = topicId.toLowerCase();
		if (normalizedTopicId.startsWith(needle)) {
			prefixMatches.push({ id: topicId, text: topicId });
			continue;
		}
		if (normalizedTopicId.includes(needle)) {
			substringMatches.push({ id: topicId, text: topicId });
		}
	}

	return [...prefixMatches, ...substringMatches].slice(0, limit);
}

export function formatDuration(ms: number): string {
	if (ms < 10000) return `${ms} ms`;
	const seconds = ms / 1000;
	if (seconds < 600) return `${seconds.toFixed(1)} s`;
	const minutes = seconds / 60;
	if (minutes <= 720) return `${minutes.toFixed(1)} min`;
	const hours = minutes / 60;
	if (hours <= 48) return `${hours.toFixed(1)} h`;
	const days = hours / 24;
	return `${days.toFixed(1)} d`;
}
