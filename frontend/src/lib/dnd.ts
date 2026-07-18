/*
 * Copyright (c) 2025 Kai Lawrence
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

import type { SplitEdge } from './layout';

/** Custom MIME type identifying an in-app tab drag. */
export const TAB_DND_MIME = 'application/x-mqtt-tab';

export type TabDragPayload = { groupId: string; topicId: string };

export function writeTabDrag(event: DragEvent, payload: TabDragPayload) {
	if (!event.dataTransfer) return;
	event.dataTransfer.setData(TAB_DND_MIME, JSON.stringify(payload));
	event.dataTransfer.effectAllowed = 'move';
}

export function readTabDrag(event: DragEvent): TabDragPayload | null {
	const raw = event.dataTransfer?.getData(TAB_DND_MIME);
	if (!raw) return null;
	try {
		const parsed = JSON.parse(raw);
		if (typeof parsed?.groupId === 'string' && typeof parsed?.topicId === 'string') {
			return parsed;
		}
	} catch {
		/* ignore malformed payloads */
	}
	return null;
}

/**
 * Map a pointer position within a rect to a drop zone: the four ~25% edge bands
 * yield a split edge; the center means "move into this group".
 */
export function edgeFromPoint(
	rect: { left: number; top: number; width: number; height: number },
	x: number,
	y: number,
	band = 0.25
): SplitEdge | 'center' {
	const fx = (x - rect.left) / rect.width;
	const fy = (y - rect.top) / rect.height;
	// Distance into each edge; pick the closest edge if within the band.
	const distances: { edge: SplitEdge; d: number }[] = [
		{ edge: 'left', d: fx },
		{ edge: 'right', d: 1 - fx },
		{ edge: 'up', d: fy },
		{ edge: 'down', d: 1 - fy }
	];
	distances.sort((a, b) => a.d - b.d);
	return distances[0].d <= band ? distances[0].edge : 'center';
}
