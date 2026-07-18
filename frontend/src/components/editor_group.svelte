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
	import type { BrokerRepositoryEntry, EditorGroup } from '$lib/state';
	import { focusGroup, moveTab, moveTabToNewSplit } from '$lib/layout';
	import { edgeFromPoint, readTabDrag, TAB_DND_MIME, type TabDragPayload } from '$lib/dnd';
	import TopicTabs from './topic_tabs.svelte';
	import GroupEditor from './group_editor.svelte';
	import TabContextMenu from './tab_context_menu.svelte';

	export let broker: BrokerRepositoryEntry;
	export let group: EditorGroup;
	export let syncingTopics: Set<string>;

	$: syncing = group.activeTopicId ? syncingTopics.has(group.activeTopicId) : false;
	$: focused = broker.activeGroupId === group.id;

	let bodyEl: HTMLDivElement;
	let dropEdge: ReturnType<typeof edgeFromPoint> | null = null;

	let menu: { x: number; y: number; topicId: string } | null = null;

	function focus() {
		if (broker.activeGroupId !== group.id) {
			focusGroup(broker, group.id);
			broker = broker;
		}
	}

	function onBodyDragOver(event: DragEvent) {
		if (!event.dataTransfer?.types.includes(TAB_DND_MIME)) return;
		event.preventDefault();
		const rect = bodyEl.getBoundingClientRect();
		dropEdge = edgeFromPoint(rect, event.clientX, event.clientY);
	}

	function onBodyDrop(event: DragEvent) {
		const payload: TabDragPayload | null = readTabDrag(event);
		const edge = dropEdge;
		dropEdge = null;
		if (!payload) return;
		event.preventDefault();
		if (edge && edge !== 'center') {
			moveTabToNewSplit(broker, group.id, edge, payload.groupId, payload.topicId);
		} else {
			moveTab(broker, payload.groupId, group.id, payload.topicId);
		}
		broker = broker;
	}
</script>

<!-- svelte-ignore a11y-no-static-element-interactions -->
<div class="editor-group" class:focused on:mousedown|capture={focus}>
	<TopicTabs bind:broker {group} on:contextmenu={(event) => (menu = event.detail)} />

	<div
		class="group-body"
		bind:this={bodyEl}
		on:dragover={onBodyDragOver}
		on:dragleave={() => (dropEdge = null)}
		on:drop={onBodyDrop}
	>
		<GroupEditor bind:broker {group} {syncing} />

		{#if dropEdge}
			<div class="drop-overlay drop-overlay--{dropEdge}" />
		{/if}
	</div>
</div>

{#if menu}
	<TabContextMenu
		bind:broker
		groupId={group.id}
		topicId={menu.topicId}
		x={menu.x}
		y={menu.y}
		on:close={() => (menu = null)}
	/>
{/if}

<style>
	.editor-group {
		position: relative;
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
		min-width: 0;
		background: var(--cds-layer, #262626);
		box-sizing: border-box;
	}

	/* Focus ring drawn on an overlay above the pane content so it stays visible on
	   every edge (an inset box-shadow would be painted over by the editor/tabs) and
	   adds no layout gap between panes. */
	.editor-group.focused::after {
		content: '';
		position: absolute;
		inset: 0;
		pointer-events: none;
		box-shadow: inset 0 0 0 1px var(--cds-interactive, #0f62fe);
		z-index: 7;
	}

	.group-body {
		position: relative;
		flex: 1 1 auto;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}

	.drop-overlay {
		position: absolute;
		z-index: 5;
		pointer-events: none;
		background: color-mix(in srgb, var(--cds-interactive, #0f62fe) 30%, transparent);
		border: 1px solid var(--cds-interactive, #0f62fe);
	}

	.drop-overlay--center {
		inset: 0;
	}
	.drop-overlay--left {
		left: 0;
		top: 0;
		bottom: 0;
		width: 50%;
	}
	.drop-overlay--right {
		right: 0;
		top: 0;
		bottom: 0;
		width: 50%;
	}
	.drop-overlay--up {
		left: 0;
		right: 0;
		top: 0;
		height: 50%;
	}
	.drop-overlay--down {
		left: 0;
		right: 0;
		bottom: 0;
		height: 50%;
	}
</style>
