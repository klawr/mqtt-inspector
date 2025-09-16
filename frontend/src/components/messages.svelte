<!-- Copyright (c) 2024 Kai Lawrence -->
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
	import type { Treebranch } from '$lib/state';
	import {
		Checkbox,
		CodeSnippet,
		ProgressIndicator,
		ProgressStep,
		Tile
	} from 'carbon-components-svelte';
	import Monaco from './monaco.svelte';
	import MonacoDiff from './monaco_diff.svelte';

	export let selectedTopic: Treebranch | null; // Can't be null.

	let compareMessage = false;
	let lockedIndex = false;
	$: if (selectedTopic) {
		if (!lockedIndex) {
			selectedMessage = selectedTopic.messages[selectedIndex];
		} else {
			selectedIndex = 0;
		}
	}

	let selectedIndex = 0;
	let selectedMessage = selectedTopic?.messages[selectedIndex];

	let selectedIndexCompare = 0;
	let selectedMessageCompare = selectedTopic?.messages[selectedIndexCompare];

	function selectMessage(index: number) {
		lockedIndex = true;
		selectedIndex = index;
		selectedMessage = selectedTopic?.messages[index];
	}

	function selectMessageCompare(index: number) {
		selectedIndexCompare = index;
		selectedMessageCompare = selectedTopic?.messages[index];
	}

	let scrollDiv: HTMLDivElement | null = null;
	function handleWheel(event: WheelEvent) {
		if (scrollDiv && event.deltaY !== 0) {
			scrollDiv.scrollLeft -= event.deltaY;
			event.preventDefault();
		}
	}

	function curateDate(dateString: string) {
		const date = new Date(dateString);
		return `${date.toLocaleDateString('de-DE')}, ${date.toLocaleTimeString(undefined, {
			hour12: false
		})}.${String(date.getMilliseconds())}`;
	}
</script>

{#if selectedTopic}
	<div style="height: 6em;">
		<h4>Selected topic:</h4>
		<CodeSnippet light code={selectedTopic?.id}></CodeSnippet>
	</div>

	{#if selectedTopic?.messages.length}
		<Tile light style="height: calc(100vh - 15em)">
			{#if selectedMessage}
				<h5>
					Selected message: {curateDate(selectedMessage.timestamp)}
				</h5>
				<div style="height: calc(100% - 9em">
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

			<div style="display: flex;">
				<div style="margin-right: 1em;">
					<Checkbox labelText="Lock message" bind:checked={lockedIndex} />
				</div>
				<div style="margin-right: 1em;">
					<Checkbox labelText="Compare message" bind:checked={compareMessage} />
				</div>
				<div style="align-self: center;">
					Cached Messages: {selectedTopic?.messages.length || 0}
				</div>
			</div>

			<div
				bind:this={scrollDiv}
				style="overflow-x: auto; padding-bottom: 1em;"
				on:wheel={handleWheel}
			>
				<ProgressIndicator
					class={compareMessage ? 'green-indicator' : ''}
					currentIndex={lockedIndex
						? selectedTopic?.messages.findIndex((e) => e.timestamp == selectedMessage?.timestamp)
						: selectedIndex}
				>
					{#each selectedTopic?.messages as message, index}
						<ProgressStep
							label={`${message.delta_t ? message.delta_t : 0} ms`}
							on:click={() => selectMessage(index)}
							title={curateDate(message.timestamp)}
						/>
					{/each}
				</ProgressIndicator>
				{#if compareMessage}
					<div style="height: 1em" />
					<ProgressIndicator
						class="red-indicator"
						currentIndex={selectedTopic?.messages.findIndex(
							(e) => e.timestamp == selectedMessageCompare?.timestamp
						)}
					>
						{#each selectedTopic?.messages as compareMessage, compareIndex}
							<ProgressStep
								label={`${compareMessage.delta_t ? compareMessage.delta_t : 0} ms`}
								on:click={() => selectMessageCompare(compareIndex)}
								title={curateDate(compareMessage.timestamp)}
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
		</Tile>
	{/if}
{/if}
