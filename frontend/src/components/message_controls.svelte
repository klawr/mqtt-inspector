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
	import type { BrokerRepositoryEntry, MessageProperties } from '$lib/state';
	import { formatBytes, formatDuration, findbranchwithid } from '$lib/helper';
	import { focusedGroup } from '$lib/layout';
	import { copyToClipboard } from '$lib/clipboard';
	import {
		isLocked,
		resetToLatest,
		selectMessageAt,
		setLocked,
		shownMessage,
		stepMessage
	} from '$lib/message_view';
	import {
		Button,
		Checkbox,
		InlineLoading,
		ProgressIndicator,
		ProgressStep
	} from 'carbon-components-svelte';
	import {
		ChevronDown,
		ChevronLeft,
		ChevronRight,
		ChevronUp,
		Copy,
		PageFirst
	} from 'carbon-icons-svelte';

	// Single toolbar for the focused editor pane: message meta, navigation, compare,
	// and the progress indicator. Operates on the focused group's shared view state.
	export let broker: BrokerRepositoryEntry;
	export let syncingTopics: Set<string> = new Set();

	const STEP_WIDTH_PX = 112;

	$: group = focusedGroup(broker);
	$: view = group.view;
	$: branch = group.activeTopicId
		? findbranchwithid(group.activeTopicId, broker.topics) ?? null
		: null;
	$: messages = branch?.messages ?? [];
	$: messageCount = messages.length;
	$: syncing = group.activeTopicId ? syncingTopics.has(group.activeTopicId) : false;
	$: selectedMessage = shownMessage(view.main, messages);
	$: propertyRows = selectedMessage ? describeProperties(selectedMessage.properties) : [];
	$: userPropertyCount = selectedMessage?.properties?.user_properties?.length ?? 0;

	// Stays open while navigating between messages.
	let showProperties = false;

	type PropertyRow = { key: string; value: string; user: boolean };

	/** Flattens MQTT 5 properties into table rows: user properties first, then the rest. */
	function describeProperties(props: MessageProperties | null): PropertyRow[] {
		if (!props) return [];
		const rows: PropertyRow[] = (props.user_properties ?? []).map(([key, value]) => ({
			key,
			value,
			user: true
		}));
		const add = (key: string, value: string | number | undefined) => {
			if (value !== undefined) rows.push({ key, value: String(value), user: false });
		};
		add('Content type', props.content_type);
		add('Response topic', props.response_topic);
		add('Correlation data', props.correlation_data);
		add(
			'Message expiry',
			props.message_expiry_interval !== undefined ? `${props.message_expiry_interval} s` : undefined
		);
		add(
			'Payload format',
			props.payload_format_indicator === undefined
				? undefined
				: props.payload_format_indicator === 1
					? 'UTF-8'
					: 'unspecified bytes'
		);
		return rows;
	}

	let containerWidth = 800;
	$: maxVisible = Math.max(3, Math.floor(containerWidth / STEP_WIDTH_PX));
	$: windowStart = Math.max(
		0,
		Math.min(view.main.index - Math.floor(maxVisible / 2), messageCount - maxVisible)
	);
	$: windowEnd = Math.min(messageCount, Math.max(0, windowStart) + maxVisible);
	$: visibleSlice = messages.slice(Math.max(0, windowStart), windowEnd);
	$: windowStartCompare = Math.max(
		0,
		Math.min(view.compare.index - Math.floor(maxVisible / 2), messageCount - maxVisible)
	);
	$: windowEndCompare = Math.min(messageCount, Math.max(0, windowStartCompare) + maxVisible);
	$: visibleSliceCompare = messages.slice(Math.max(0, windowStartCompare), windowEndCompare);

	// All navigation goes through the shared helpers so the index and the pinned
	// (locked) message always change together.
	function selectMessage(index: number) {
		selectMessageAt(view.main, messages, index);
		broker = broker;
	}
	function selectMessageCompare(index: number) {
		selectMessageAt(view.compare, messages, index);
		broker = broker;
	}
	function step(delta: number) {
		stepMessage(view.main, messages, delta);
		broker = broker;
	}
	function goToLatest() {
		resetToLatest(view);
		broker = broker;
	}
	function toggleLock(checked: boolean) {
		setLocked(view.main, messages, checked);
		broker = broker;
	}
	function toggleCompare(checked: boolean) {
		view.compareMessage = checked;
		broker = broker;
	}
	function toggleLockCompare(checked: boolean) {
		setLocked(view.compare, messages, checked);
		broker = broker;
	}

	let navDiv: HTMLDivElement | null = null;
	function handleWheel(event: WheelEvent) {
		if (navDiv && event.deltaY !== 0) {
			step(event.deltaY > 0 ? 1 : -1);
			event.preventDefault();
		}
	}

	function getDeltaLabel(msgs: import('$lib/state').Message[], index: number): string {
		if (index >= msgs.length - 1) return formatDuration(0);
		const curr = new Date(msgs[index].timestamp).getTime();
		const next = new Date(msgs[index + 1].timestamp).getTime();
		return formatDuration(curr - next);
	}

	function curateDate(dateString: string) {
		const date = new Date(dateString);
		return `${date.toLocaleDateString('de-DE')}, ${date.toLocaleTimeString(undefined, {
			hour12: false
		})}.${String(date.getMilliseconds())}`;
	}

	let copyStatusMessage = '';
	let copyStatusTimer: ReturnType<typeof setTimeout> | null = null;
	async function copyMessage() {
		if (!selectedMessage?.text) return;
		const ok = await copyToClipboard(selectedMessage.text);
		copyStatusMessage = ok ? 'Copied' : 'Copy failed';
		if (copyStatusTimer) clearTimeout(copyStatusTimer);
		copyStatusTimer = setTimeout(() => (copyStatusMessage = ''), 1500);
	}

	onDestroy(() => {
		if (copyStatusTimer) clearTimeout(copyStatusTimer);
	});
</script>

{#if branch && messageCount}
	<div class="controls">
		<div class="footer-row">
			<div class="footer-controls">
				<Checkbox
					labelText="Lock message"
					checked={isLocked(view.main)}
					on:check={(e) => toggleLock(e.detail)}
				/>
				<div class="nav-buttons">
					<Button
						kind="secondary"
						iconDescription="First Message"
						tooltipPosition="top"
						icon={PageFirst}
						on:click={goToLatest}
					/>
					<Button
						kind="secondary"
						iconDescription="Next Message"
						tooltipPosition="top"
						icon={ChevronLeft}
						on:click={() => step(-1)}
					/>
					<Button
						kind="secondary"
						iconDescription="Previous Message"
						tooltipPosition="top"
						icon={ChevronRight}
						on:click={() => step(1)}
					/>
				</div>
				<Checkbox
					labelText="Compare message"
					checked={view.compareMessage}
					on:check={(e) => toggleCompare(e.detail)}
				/>
				{#if view.compareMessage}
					<Checkbox
						labelText="Lock"
						checked={isLocked(view.compare)}
						on:check={(e) => toggleLockCompare(e.detail)}
					/>
				{/if}
			</div>
			{#if selectedMessage}
				<div class="message-meta">
					<span style="font-size: 0.875rem; color: var(--cds-text-secondary);">
						Selected message: {curateDate(selectedMessage.timestamp)}
						{#if view.main.evicted}
							<span class="evicted">(locked message evicted from buffer, still shown)</span>
						{/if}
						{#if view.compareMessage && view.compare.evicted}
							<span class="evicted">(compare message evicted from buffer, still shown)</span>
						{/if}
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
						{#if propertyRows.length}
							<Button
								kind="ghost"
								size="small"
								icon={showProperties ? ChevronUp : ChevronDown}
								on:click={() => (showProperties = !showProperties)}
							>
								{userPropertyCount ? `User properties (${userPropertyCount})` : 'Properties'}
							</Button>
						{/if}
						{#if syncing}
							<InlineLoading description="Syncing..." />
						{/if}
						<Button
							kind="ghost"
							size="sm"
							icon={Copy}
							iconDescription="Copy message"
							tooltipPosition="left"
							on:click={copyMessage}
						/>
						{#if copyStatusMessage}
							<span style="font-size: 0.75rem; color: var(--cds-text-secondary);"
								>{copyStatusMessage}</span
							>
						{/if}
						<span style="font-size: 0.875rem; white-space: nowrap;" data-testid="message-counter">
							{view.main.evicted ? '–' : view.main.index + 1} / {messageCount} messages
						</span>
					</div>
				</div>
			{/if}
		</div>

		{#if showProperties && propertyRows.length}
			<div class="properties">
				<table>
					<tbody>
						{#each propertyRows as row}
							<tr class:builtin={!row.user}>
								<th scope="row" title={row.key}>{row.key}</th>
								<td>{row.value}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}

		{#if messageCount > 1}
			<div
				bind:this={navDiv}
				bind:clientWidth={containerWidth}
				on:wheel={handleWheel}
				data-testid="message-timeline"
			>
				<ProgressIndicator
					class={view.compareMessage ? 'green-indicator' : ''}
					currentIndex={view.main.index - Math.max(0, windowStart)}
				>
					{#each visibleSlice as message, i}
						{@const realIndex = Math.max(0, windowStart) + i}
						<ProgressStep
							label={getDeltaLabel(messages, realIndex)}
							on:click={() => selectMessage(realIndex)}
							title={curateDate(message.timestamp)}
						/>
					{/each}
				</ProgressIndicator>
				{#if view.compareMessage}
					<div style="height: 1em" />
					<ProgressIndicator
						class="red-indicator"
						currentIndex={view.compare.index - Math.max(0, windowStartCompare)}
					>
						{#each visibleSliceCompare as message, i}
							{@const realIndex = Math.max(0, windowStartCompare) + i}
							<ProgressStep
								label={getDeltaLabel(messages, realIndex)}
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

<style>
	.controls {
		flex: 0 0 auto;
		padding: 0.5em 1em 0.75em;
		border-top: 1px solid var(--cds-border-subtle, #393939);
		background: var(--cds-layer, #262626);
	}

	.footer-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1em;
		flex-wrap: wrap;
		padding-bottom: 0.5em;
	}

	.footer-controls {
		display: flex;
		align-items: center;
		gap: 0.75em;
		flex-wrap: wrap;
	}

	.nav-buttons {
		display: flex;
		align-items: center;
		scale: 0.75;
		margin: -0.25em -0.5em;
	}

	.message-meta {
		display: flex;
		align-items: center;
		gap: 0.5em;
		flex-wrap: wrap;
		justify-content: flex-end;
	}

	.properties {
		max-height: 12em;
		overflow: auto;
		margin-bottom: 0.5em;
		border: 1px solid var(--cds-border-subtle, #393939);
	}

	.properties table {
		width: 100%;
		border-collapse: collapse;
		font-family: var(--cds-code-01-font-family, monospace);
		font-size: 0.8125rem;
	}

	.properties th,
	.properties td {
		padding: 0.25em 0.75em;
		text-align: left;
		vertical-align: top;
		border-bottom: 1px solid var(--cds-border-subtle, #393939);
	}

	.properties th {
		width: 1%;
		max-width: 20em;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		font-weight: 600;
		color: var(--cds-text-secondary);
	}

	.properties td {
		word-break: break-all;
		user-select: text;
	}

	/* Built-in MQTT 5 properties (content type, ...) follow the user properties. */
	.properties tr.builtin th {
		font-family: var(--cds-body-short-01-font-family, sans-serif);
		font-style: italic;
	}

	.evicted {
		color: var(--cds-support-warning, #f1c21b);
	}

	.footer-controls :global(.bx--form-item),
	.footer-controls :global(.bx--checkbox-wrapper) {
		margin: 0;
	}
</style>
