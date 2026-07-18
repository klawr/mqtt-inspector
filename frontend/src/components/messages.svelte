<!-- Copyright (c) 2024-2025 Kai Lawrence -->
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
	import type { BrokerRepositoryEntry, Treebranch } from '$lib/state';
	import { formatDuration } from '$lib/helper';
	import { getTopicSwitchResetState } from './messages';
	import {
		Button,
		Checkbox,
		InlineLoading,
		ProgressIndicator,
		ProgressStep
	} from 'carbon-components-svelte';
	import Monaco from './monaco.svelte';
	import MonacoDiff from './monaco_diff.svelte';
	import TopicTabs from './topic_tabs.svelte';
	import { ChevronLeft, ChevronRight, Copy, PageFirst } from 'carbon-icons-svelte';

	export let broker: BrokerRepositoryEntry;
	export let selectedTopic: Treebranch | null; // Can't be null.
	export let topicSyncing = false;

	const STEP_WIDTH_PX = 112; // width of one ProgressStep including gap

	let compareMessage = false;
	let lockedIndex = false;
	let lockedIndexCompare = false;
	let selectedIndex = 0;
	let selectedMessage = selectedTopic?.messages[selectedIndex];
	let selectedIndexCompare = 1;
	let selectedMessageCompare = selectedTopic?.messages[selectedIndexCompare];
	let previousTopicId = selectedTopic?.id ?? null;

	$: messageCount = selectedTopic?.messages.length ?? 0;

	// Switching topics should always focus the latest message of that topic.
	$: {
		const currentTopicId = selectedTopic?.id ?? null;
		if (currentTopicId !== previousTopicId) {
			const reset = getTopicSwitchResetState(selectedTopic?.messages.length ?? 0);
			selectedIndex = reset.selectedIndex;
			selectedMessage = selectedTopic?.messages[reset.selectedIndex];
			lockedIndex = reset.lockedIndex;
			compareMessage = reset.compareMessage;
			selectedIndexCompare = reset.selectedIndexCompare;
			selectedMessageCompare = selectedTopic?.messages[reset.selectedIndexCompare];
			lockedIndexCompare = reset.lockedIndexCompare;
			prevMessageCount = reset.prevMessageCount;
			previousTopicId = currentTopicId;
		}
	}

	// When locked and new messages arrive (prepended at index 0), keep pointing
	// at the same message by shifting selectedIndex.
	let prevMessageCount = messageCount;
	$: {
		const diff = messageCount - prevMessageCount;
		if (diff > 0 && lockedIndex && selectedIndex >= 0) {
			selectedIndex += diff;
		}
		if (diff > 0 && lockedIndexCompare && selectedIndexCompare >= 0) {
			selectedIndexCompare += diff;
		}
		prevMessageCount = messageCount;
	}

	// Dynamic window size based on container width
	let containerWidth = 800;
	$: maxVisible = Math.max(3, Math.floor(containerWidth / STEP_WIDTH_PX));

	// Sliding window around selectedIndex for the main indicator
	$: windowStart = Math.max(
		0,
		Math.min(selectedIndex - Math.floor(maxVisible / 2), messageCount - maxVisible)
	);
	$: windowEnd = Math.min(messageCount, (windowStart < 0 ? 0 : windowStart) + maxVisible);
	$: visibleSlice = selectedTopic?.messages.slice(Math.max(0, windowStart), windowEnd) ?? [];

	// Sliding window around selectedIndexCompare for the compare indicator
	$: windowStartCompare = Math.max(
		0,
		Math.min(selectedIndexCompare - Math.floor(maxVisible / 2), messageCount - maxVisible)
	);
	$: windowEndCompare = Math.min(
		messageCount,
		(windowStartCompare < 0 ? 0 : windowStartCompare) + maxVisible
	);
	$: visibleSliceCompare =
		selectedTopic?.messages.slice(Math.max(0, windowStartCompare), windowEndCompare) ?? [];

	$: if (selectedTopic) {
		if (!lockedIndex) {
			selectedMessage = selectedTopic.messages[selectedIndex];
		}
	}

	function selectMessage(index: number) {
		if (index < 0) {
			index = 0;
		} else if (index >= (selectedTopic?.messages.length || 0)) {
			index = (selectedTopic?.messages.length || 1) - 1;
		}
		lockedIndex = true;
		selectedIndex = index;
		selectedMessage = selectedTopic?.messages[index];
	}

	function selectMessageCompare(index: number) {
		lockedIndexCompare = true;
		selectedIndexCompare = index;
		selectedMessageCompare = selectedTopic?.messages[index];
	}

	let navDiv: HTMLDivElement | null = null;
	function handleWheel(event: WheelEvent) {
		if (navDiv && event.deltaY !== 0) {
			// Scroll wheel on the indicator navigates messages
			const direction = event.deltaY > 0 ? 1 : -1;
			selectMessage(selectedIndex + direction);
			event.preventDefault();
		}
	}

	function getDeltaLabel(messages: import('$lib/state').Message[], index: number): string {
		if (index >= messages.length - 1) return formatDuration(0);
		const curr = new Date(messages[index].timestamp).getTime();
		const next = new Date(messages[index + 1].timestamp).getTime();
		return formatDuration(curr - next);
	}

	function curateDate(dateString: string) {
		const date = new Date(dateString);
		return `${date.toLocaleDateString('de-DE')}, ${date.toLocaleTimeString(undefined, {
			hour12: false
		})}.${String(date.getMilliseconds())}`;
	}

	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
		return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
	}

	let copyStatusMessage = '';
	let messageCopyStatusTimer: ReturnType<typeof setTimeout> | null = null;

	function fallbackCopyText(text: string): boolean {
		try {
			const textArea = document.createElement('textarea');
			textArea.value = text;
			textArea.setAttribute('readonly', '');
			textArea.style.position = 'fixed';
			textArea.style.opacity = '0';
			document.body.appendChild(textArea);
			textArea.focus();
			textArea.select();
			const copied = document.execCommand('copy');
			document.body.removeChild(textArea);
			return copied;
		} catch {
			return false;
		}
	}

	async function copyText(target: 'topic' | 'message') {
		const text = target === 'topic' ? selectedTopic?.id : selectedMessage?.text;
		if (!text) return;

		let copied = false;
		try {
			if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
				await navigator.clipboard.writeText(text);
				copied = true;
			}
		} catch {
			copied = false;
		}

		if (!copied) {
			copied = fallbackCopyText(text);
		}

		if (target === 'message') {
			copyStatusMessage = copied ? 'Copied' : 'Copy failed';
			if (messageCopyStatusTimer) clearTimeout(messageCopyStatusTimer);
			messageCopyStatusTimer = setTimeout(() => {
				copyStatusMessage = '';
			}, 1500);
		}
	}
</script>

{#if broker.openTabs.length || selectedTopic}
	<div class="editor-panel">
		<TopicTabs bind:broker />

		{#if topicSyncing && !selectedTopic?.messages.length}
			<div class="panel-body panel-body--center">
				<InlineLoading description="Loading messages..." />
			</div>
		{:else if selectedTopic?.messages.length}
			<!-- Editor is flush under the topic tabs (VS Code-style); the message meta
			     info and navigation controls live below it. -->
			{#if selectedMessage}
				<div class="editor-wrap">
					{#if compareMessage && selectedMessageCompare}
						<MonacoDiff
							bind:code={selectedMessage.text}
							bind:codeCompare={selectedMessageCompare.text}
						/>
					{:else}
						<Monaco readonly bind:code={selectedMessage.text} />
					{/if}
				</div>
			{/if}

			<div class="panel-footer">
				{#if selectedMessage}
					<div class="message-meta">
						<span style="font-size: 0.875rem; color: var(--cds-text-secondary);">
							Selected message: {curateDate(selectedMessage.timestamp)}
							{#if selectedMessage.retain}
								{' '}(retained)
							{/if}
							{#if selectedMessage.isTruncated}
								{' '}(truncated: showing {formatBytes(selectedMessage.displayedPayloadSize)} of {formatBytes(
									selectedMessage.originalPayloadSize
								)})
							{/if}
						</span>
						<div style="display: flex; align-items: center; gap: 0.5em;">
							{#if topicSyncing}
								<InlineLoading description="Syncing..." />
							{/if}
							<Button
								kind="ghost"
								size="sm"
								icon={Copy}
								iconDescription="Copy message"
								tooltipPosition="left"
								on:click={() => copyText('message')}
							/>
							{#if copyStatusMessage}
								<span style="font-size: 0.75rem; color: var(--cds-text-secondary);"
									>{copyStatusMessage}</span
								>
							{/if}
							<span style="font-size: 0.875rem; white-space: nowrap;">
								{selectedIndex + 1} / {messageCount} messages
							</span>
						</div>
					</div>
				{/if}

				<div style="display: flex; align-items: center; gap: 0.5em; flex-wrap: wrap;">
					<div style="align-self: center; margin-right: 1em;">
						<Checkbox labelText="Lock message" bind:checked={lockedIndex} />
					</div>

					<div style="align-self: center; scale: 0.75; margin: -0.25em">
						<Button
							kind="secondary"
							iconDescription="First Message"
							tooltipPosition="top"
							icon={PageFirst}
							on:click={() => {
								selectMessage(0);
								lockedIndex = false;
								lockedIndexCompare = false;
							}}
						/>
						<Button
							kind="secondary"
							iconDescription="Next Message"
							tooltipPosition="top"
							icon={ChevronLeft}
							on:click={() => {
								selectMessage(selectedIndex - 1);
							}}
						/>
						<Button
							kind="secondary"
							iconDescription="Previous Message"
							tooltipPosition="top"
							icon={ChevronRight}
							on:click={() => selectMessage(selectedIndex + 1)}
						/>
					</div>

					<div style="align-self: center; margin-right: 1em;">
						<Checkbox labelText="Compare message" bind:checked={compareMessage} />
					</div>

					{#if compareMessage}
						<div style="align-self: center; margin-right: 1em;">
							<Checkbox labelText="Lock" bind:checked={lockedIndexCompare} />
						</div>
					{/if}
				</div>

				{#if messageCount > 1}
					<div bind:this={navDiv} bind:clientWidth={containerWidth} on:wheel={handleWheel}>
						<ProgressIndicator
							class={compareMessage ? 'green-indicator' : ''}
							currentIndex={selectedIndex - Math.max(0, windowStart)}
						>
							{#each visibleSlice as message, i}
								{@const realIndex = Math.max(0, windowStart) + i}
								<ProgressStep
									label={getDeltaLabel(selectedTopic?.messages ?? [], realIndex)}
									on:click={() => selectMessage(realIndex)}
									title={curateDate(message.timestamp)}
								/>
							{/each}
						</ProgressIndicator>
						{#if compareMessage}
							<div style="height: 1em" />
							<ProgressIndicator
								class="red-indicator"
								currentIndex={selectedIndexCompare - Math.max(0, windowStartCompare)}
							>
								{#each visibleSliceCompare as message, i}
									{@const realIndex = Math.max(0, windowStartCompare) + i}
									<ProgressStep
										label={getDeltaLabel(selectedTopic?.messages ?? [], realIndex)}
										on:click={() => selectMessageCompare(realIndex)}
										title={curateDate(message.timestamp)}
									/>
								{/each}
							</ProgressIndicator>
						{/if}
						<style>
							.green-indicator {
								--cds-interactive-04: #4ec9b0;
							}
							.red-indicator {
								--cds-interactive-04: #f44747;
							}
						</style>
					</div>
				{/if}
			</div>
		{/if}
	</div>
{/if}

<style>
	.editor-panel {
		display: flex;
		flex-direction: column;
		height: calc(100vh - 8em);
		min-height: 0;
		background: var(--cds-layer, #262626);
		border: 1px solid var(--cds-border-subtle, #393939);
	}

	/* Monaco fills all remaining space, flush against the tab strip above it. */
	.editor-wrap {
		flex: 1 1 auto;
		min-height: 0;
	}

	.panel-body {
		flex: 1 1 auto;
		min-height: 0;
	}

	.panel-body--center {
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.panel-footer {
		flex: 0 0 auto;
		padding: 0.5em 1em 1em;
	}

	.message-meta {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 0.5em;
		padding-bottom: 0.5em;
	}
</style>
