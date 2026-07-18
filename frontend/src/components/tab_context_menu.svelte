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
	import { createEventDispatcher } from 'svelte';
	import type { BrokerRepositoryEntry } from '$lib/state';
	import { closeOtherTabs, closeTab, splitGroup, type SplitEdge } from '$lib/layout';
	import { copyToClipboard } from '$lib/clipboard';

	export let broker: BrokerRepositoryEntry;
	export let groupId: string;
	export let topicId: string;
	export let x = 0;
	export let y = 0;

	const dispatch = createEventDispatcher<{ close: void; changed: void }>();

	function done() {
		broker = broker;
		dispatch('changed');
		dispatch('close');
	}

	function split(edge: SplitEdge) {
		splitGroup(broker, groupId, edge, topicId);
		done();
	}

	function copyPath() {
		copyToClipboard(topicId);
		dispatch('close');
	}

	function close() {
		closeTab(broker, groupId, topicId);
		done();
	}

	function closeOthers() {
		closeOtherTabs(broker, groupId, topicId);
		done();
	}

	function onWindowMouseDown(event: MouseEvent) {
		if (!(event.target as HTMLElement)?.closest?.('.tab-menu')) {
			dispatch('close');
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') dispatch('close');
	}
</script>

<svelte:window
	on:mousedown={onWindowMouseDown}
	on:keydown={onKeydown}
	on:resize={() => dispatch('close')}
/>

<div class="tab-menu" style="left: {x}px; top: {y}px;" role="menu">
	<div class="tab-menu__path" title={topicId}>{topicId}</div>
	<button type="button" class="tab-menu__item" on:click={() => split('up')}>Split Up</button>
	<button type="button" class="tab-menu__item" on:click={() => split('down')}>Split Down</button>
	<button type="button" class="tab-menu__item" on:click={() => split('left')}>Split Left</button>
	<button type="button" class="tab-menu__item" on:click={() => split('right')}>Split Right</button>
	<div class="tab-menu__sep" />
	<button type="button" class="tab-menu__item" on:click={copyPath}>Copy Path</button>
	<div class="tab-menu__sep" />
	<button type="button" class="tab-menu__item" on:click={close}>Close</button>
	<button type="button" class="tab-menu__item" on:click={closeOthers}>Close Others</button>
</div>

<style>
	.tab-menu {
		position: fixed;
		z-index: 1000;
		min-width: 12rem;
		padding: 0.25rem 0;
		background: var(--cds-layer, #262626);
		border: 1px solid var(--cds-border-subtle, #393939);
		box-shadow: 0 0.5rem 1rem rgb(0 0 0 / 0.3);
		font-size: 0.875rem;
	}

	.tab-menu__path {
		padding: 0.25rem 0.75rem;
		color: var(--cds-text-secondary, #8d8d8d);
		font-size: 0.75rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 20rem;
	}

	.tab-menu__item {
		display: block;
		width: 100%;
		padding: 0.375rem 0.75rem;
		border: 0;
		background: transparent;
		color: var(--cds-text-primary, #f4f4f4);
		text-align: left;
		cursor: pointer;
		font: inherit;
	}

	.tab-menu__item:hover {
		background: var(--cds-layer-hover, rgb(255 255 255 / 0.08));
	}

	.tab-menu__sep {
		height: 1px;
		margin: 0.25rem 0;
		background: var(--cds-border-subtle, #393939);
	}
</style>
