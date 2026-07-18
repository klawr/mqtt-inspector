<!-- Copyright (c) 2025 Kai Lawrence -->
<!--
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
-->

<script lang="ts">
	import { onDestroy } from 'svelte';
	import type { BrokerRepositoryEntry, LayoutNode } from '$lib/state';
	import EditorGroup from './editor_group.svelte';
	import Self from './editor_layout.svelte';

	export let broker: BrokerRepositoryEntry;
	// When rendered at the top level `node` is omitted and follows broker.layout
	// reactively; recursion passes an explicit child node.
	export let node: LayoutNode | null = null;
	export let syncingTopics: Set<string>;

	$: current = node ?? broker.layout;

	const MIN_PCT = 8;

	let containerEl: HTMLDivElement;
	let dragging = false;
	let liveSizes: number[] = [];
	// Cleanup for an in-flight resize drag, so listeners never outlive the component.
	let endDrag: (() => void) | null = null;
	// Mirror node.sizes into a live copy for smooth resizing without re-rendering
	// the whole tree (which would thrash Monaco) until the drag ends.
	$: if (!dragging) {
		liveSizes = current.type === 'split' ? [...current.sizes] : [];
	}

	function startResize(event: PointerEvent, index: number) {
		if (current.type !== 'split') return;
		const split = current;
		dragging = true;
		const rect = containerEl.getBoundingClientRect();
		const total = split.direction === 'row' ? rect.width : rect.height;
		const startPos = split.direction === 'row' ? event.clientX : event.clientY;
		const a = liveSizes[index];
		const b = liveSizes[index + 1];

		function move(ev: PointerEvent) {
			const pos = split.direction === 'row' ? ev.clientX : ev.clientY;
			const deltaPct = ((pos - startPos) / total) * 100;
			let na = a + deltaPct;
			let nb = b - deltaPct;
			if (na < MIN_PCT) {
				nb -= MIN_PCT - na;
				na = MIN_PCT;
			}
			if (nb < MIN_PCT) {
				na -= MIN_PCT - nb;
				nb = MIN_PCT;
			}
			liveSizes[index] = na;
			liveSizes[index + 1] = nb;
			liveSizes = [...liveSizes];
		}
		function up() {
			dragging = false;
			endDrag?.();
			split.sizes = [...liveSizes];
			broker = broker;
		}
		endDrag = () => {
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
			endDrag = null;
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
		event.preventDefault();
	}

	onDestroy(() => endDrag?.());
</script>

{#if current.type === 'group'}
	<EditorGroup bind:broker group={current.group} {syncingTopics} />
{:else}
	<div
		class="split"
		class:row={current.direction === 'row'}
		class:column={current.direction === 'column'}
		bind:this={containerEl}
	>
		{#each current.children as child, i (child)}
			<div
				class="split__pane"
				style="flex-basis: {liveSizes[i] ?? 100 / current.children.length}%;"
			>
				<Self bind:broker node={child} {syncingTopics} />
			</div>
			{#if i < current.children.length - 1}
				<div
					class="split__resizer"
					class:row={current.direction === 'row'}
					role="separator"
					on:pointerdown={(event) => startResize(event, i)}
				/>
			{/if}
		{/each}
	</div>
{/if}

<style>
	.split {
		display: flex;
		height: 100%;
		width: 100%;
		min-height: 0;
		min-width: 0;
	}
	.split.row {
		flex-direction: row;
	}
	.split.column {
		flex-direction: column;
	}

	.split__pane {
		flex-grow: 0;
		flex-shrink: 1;
		min-width: 0;
		min-height: 0;
		overflow: hidden;
	}

	/* A thin 1px sash (like VS Code) with a wider invisible grab area via ::after. */
	.split__resizer {
		flex: 0 0 1px;
		position: relative;
		background: var(--cds-border-subtle, #393939);
		z-index: 6;
	}
	.split__resizer::after {
		content: '';
		position: absolute;
		inset: -3px;
		cursor: row-resize;
	}
	.split__resizer.row::after {
		cursor: col-resize;
	}
	.split__resizer:hover {
		background: var(--cds-interactive, #0f62fe);
	}
</style>
