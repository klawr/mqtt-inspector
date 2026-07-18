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
	import { findbranchwithid } from '$lib/helper';
	import { getTopicSwitchResetState } from './messages';
	import { InlineLoading } from 'carbon-components-svelte';
	import Monaco from './monaco.svelte';
	import MonacoDiff from './monaco_diff.svelte';

	// One editor pane. The tab strip and the shared message toolbar live elsewhere;
	// this component only renders the active topic's selected message and keeps the
	// group's view state (selected index) in sync as messages arrive.
	export let broker: BrokerRepositoryEntry;
	export let group: EditorGroup;
	export let syncing = false;

	$: branch = group.activeTopicId
		? findbranchwithid(group.activeTopicId, broker.topics) ?? null
		: null;
	$: messages = branch?.messages ?? [];
	$: messageCount = messages.length;

	let prevTopicId: string | null = branch?.id ?? null;
	let prevCount = messageCount;

	// Reset to the latest message on topic switch; keep a locked index pointing at
	// the same message as newer ones are prepended.
	function maintain() {
		const view = group.view;
		let changed = false;
		const topicId = branch?.id ?? null;
		if (topicId !== prevTopicId) {
			const reset = getTopicSwitchResetState(messageCount);
			view.selectedIndex = reset.selectedIndex;
			view.lockedIndex = reset.lockedIndex;
			view.compareMessage = reset.compareMessage;
			view.selectedIndexCompare = reset.selectedIndexCompare;
			view.lockedIndexCompare = reset.lockedIndexCompare;
			prevTopicId = topicId;
			prevCount = reset.prevMessageCount;
			changed = true;
		} else {
			const diff = messageCount - prevCount;
			if (diff > 0) {
				if (view.lockedIndex && view.selectedIndex >= 0) {
					view.selectedIndex += diff;
					changed = true;
				}
				if (view.lockedIndexCompare && view.selectedIndexCompare >= 0) {
					view.selectedIndexCompare += diff;
					changed = true;
				}
				prevCount = messageCount;
			}
		}
		if (changed) broker = broker;
	}
	$: {
		// Re-run whenever the topic or message count changes.
		void messageCount;
		void (branch?.id ?? null);
		maintain();
	}

	$: selectedMessage = messages[group.view.selectedIndex];
	$: selectedMessageCompare = messages[group.view.selectedIndexCompare];
</script>

<div class="pane">
	{#if !branch}
		<div class="pane-center">
			<span class="pane-empty">Select a topic to view its messages</span>
		</div>
	{:else if syncing && !messages.length}
		<div class="pane-center">
			<InlineLoading description="Loading messages..." />
		</div>
	{:else if messages.length && selectedMessage}
		<div class="editor-wrap">
			{#if group.view.compareMessage && selectedMessageCompare}
				<MonacoDiff
					bind:code={selectedMessage.text}
					bind:codeCompare={selectedMessageCompare.text}
				/>
			{:else}
				<Monaco readonly bind:code={selectedMessage.text} />
			{/if}
		</div>
	{:else}
		<div class="pane-center">
			<span class="pane-empty">No messages for this topic yet</span>
		</div>
	{/if}
</div>

<style>
	.pane {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.editor-wrap {
		flex: 1 1 auto;
		min-height: 0;
	}

	.pane-center {
		flex: 1 1 auto;
		min-height: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.pane-empty {
		font-size: 0.875rem;
		color: var(--cds-text-secondary, #8d8d8d);
	}
</style>
