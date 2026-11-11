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
	import { shownMessage } from '$lib/message_view';
	import { InlineLoading } from 'carbon-components-svelte';
	import Monaco from './monaco.svelte';
	import MonacoDiff from './monaco_diff.svelte';

	// One editor pane. The tab strip and the shared message toolbar live elsewhere;
	// this component only renders the active topic's selected message. Selection
	// state is changed by $lib/layout (topic switches), $lib/ws_msg_handling
	// (following locked messages) and the toolbar (navigation).
	export let broker: BrokerRepositoryEntry;
	export let group: EditorGroup;
	export let syncing = false;

	$: branch = group.activeTopicId
		? findbranchwithid(group.activeTopicId, broker.topics) ?? null
		: null;
	$: messages = branch?.messages ?? [];

	$: selectedMessage = shownMessage(group.view.main, messages);
	$: selectedMessageCompare = shownMessage(group.view.compare, messages);
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
		<div class="editor-wrap" data-testid="message-editor">
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
