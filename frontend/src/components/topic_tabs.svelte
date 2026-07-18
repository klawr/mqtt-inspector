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
	import { Close } from 'carbon-icons-svelte';
	import type { BrokerRepositoryEntry, EditorGroup } from '$lib/state';
	import { activateTab, closeTab, moveTab, pinTab } from '$lib/layout';
	import { TAB_DND_MIME, readTabDrag, writeTabDrag } from '$lib/dnd';

	export let broker: BrokerRepositoryEntry;
	export let group: EditorGroup;

	const dispatch = createEventDispatcher<{
		contextmenu: { x: number; y: number; topicId: string };
	}>();

	$: activeId = group.activeTopicId;
	let dragOver = false;

	// Mutations happen in layout.ts, which Svelte can't instrument; the
	// self-assignment triggers reactivity and propagates via bind:broker.
	function onActivate(id: string) {
		activateTab(broker, group.id, id);
		broker = broker;
	}

	function onPin(id: string) {
		pinTab(broker, group.id, id);
		broker = broker;
	}

	function onClose(event: MouseEvent, id: string) {
		event.stopPropagation();
		closeTab(broker, group.id, id);
		broker = broker;
	}

	function onAuxClick(event: MouseEvent, id: string) {
		if (event.button === 1) {
			event.preventDefault();
			closeTab(broker, group.id, id);
			broker = broker;
		}
	}

	function onContextMenu(event: MouseEvent, id: string) {
		event.preventDefault();
		dispatch('contextmenu', { x: event.clientX, y: event.clientY, topicId: id });
	}

	function onDragStart(event: DragEvent, id: string) {
		writeTabDrag(event, { groupId: group.id, topicId: id });
	}

	// Dropping a tab onto this strip moves it into this group.
	function onStripDrop(event: DragEvent) {
		const payload = readTabDrag(event);
		dragOver = false;
		if (!payload) return;
		event.preventDefault();
		moveTab(broker, payload.groupId, group.id, payload.topicId);
		broker = broker;
	}

	function onStripDragOver(event: DragEvent) {
		if (event.dataTransfer?.types.includes(TAB_DND_MIME)) {
			event.preventDefault();
			dragOver = true;
		}
	}
</script>

<!-- svelte-ignore a11y-no-static-element-interactions -->
<div
	class="topic-tabs"
	class:drag-over={dragOver}
	on:dragover={onStripDragOver}
	on:dragleave={() => (dragOver = false)}
	on:drop={onStripDrop}
>
	{#each group.tabs as tab (tab.id)}
		<div
			class="topic-tabs__tab"
			class:active={tab.id === activeId}
			class:preview={tab.preview}
			role="tab"
			tabindex="0"
			draggable="true"
			aria-selected={tab.id === activeId}
			title={`${tab.id}\n(right-click for options)`}
			on:click={() => onActivate(tab.id)}
			on:dblclick={() => onPin(tab.id)}
			on:contextmenu={(event) => onContextMenu(event, tab.id)}
			on:auxclick={(event) => onAuxClick(event, tab.id)}
			on:dragstart={(event) => onDragStart(event, tab.id)}
			on:keydown={(event) => {
				if (event.key === 'Enter' || event.key === ' ') {
					event.preventDefault();
					onActivate(tab.id);
				}
			}}
		>
			<span class="topic-tabs__label">{tab.id}</span>
			<button
				type="button"
				class="topic-tabs__close"
				title="Close tab"
				aria-label="Close {tab.id}"
				on:click={(event) => onClose(event, tab.id)}
			>
				<Close size={16} />
			</button>
		</div>
	{/each}
</div>

<style>
	.topic-tabs {
		display: flex;
		flex-direction: row;
		align-items: stretch;
		overflow-x: auto;
		scrollbar-width: thin;
		min-height: 2.25rem;
		border-bottom: 1px solid var(--cds-border-subtle, #393939);
		background: var(--cds-layer, #262626);
	}

	.topic-tabs.drag-over {
		background: var(--cds-layer-hover, rgb(255 255 255 / 0.08));
	}

	.topic-tabs__tab {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.375rem 0.5rem 0.375rem 0.75rem;
		max-width: 16rem;
		white-space: nowrap;
		cursor: pointer;
		border-right: 1px solid var(--cds-border-subtle, #393939);
		border-top: 2px solid transparent;
		color: var(--cds-text-secondary, #c6c6c6);
		user-select: none;
	}

	.topic-tabs__tab:hover {
		background: var(--cds-layer-hover, rgb(255 255 255 / 0.08));
	}

	.topic-tabs__tab.active {
		background: var(--cds-layer-selected, rgb(255 255 255 / 0.12));
		color: var(--cds-text-primary, #f4f4f4);
		border-top-color: var(--cds-interactive, #0f62fe);
	}

	.topic-tabs__tab.preview .topic-tabs__label {
		font-style: italic;
	}

	.topic-tabs__label {
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.topic-tabs__close {
		display: flex;
		align-items: center;
		justify-content: center;
		border: 0;
		padding: 0.125rem;
		background: transparent;
		color: inherit;
		cursor: pointer;
		border-radius: 2px;
		opacity: 0.6;
	}

	.topic-tabs__close:hover {
		opacity: 1;
		background: var(--cds-layer-hover, rgb(255 255 255 / 0.16));
	}
</style>
