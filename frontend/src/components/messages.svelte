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

	export let selectedTopic: Treebranch | null; // Can't be null.

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

	function selectMessage(index: number) {
		lockedIndex = true;
		selectedIndex = index;
		selectedMessage = selectedTopic?.messages[index];
	}

	let scrollDiv: HTMLDivElement | null = null;
	function handleWheel(event: WheelEvent) {
		if (scrollDiv && event.deltaY !== 0) {
			scrollDiv.scrollLeft += event.deltaY;
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
	<div style="height: 8em;">
		<h4>Selected topic:</h4>
		<CodeSnippet light code={selectedTopic?.id}></CodeSnippet>

		<p>Messages: {selectedTopic?.messages.length || 0}</p>
	</div>

	{#if selectedTopic?.messages.length}
		<Tile light>
			{#if selectedMessage}
				<h5>
					Selected message: {curateDate(selectedMessage.timestamp)}
				</h5>
				<div style="height: 30em; display: flex; flex-direction: column;">
					<Monaco readonly bind:code={selectedMessage.text} />
				</div>
			{/if}

			<Checkbox labelText="Lock message" bind:checked={lockedIndex} />

			<div
				bind:this={scrollDiv}
				style="overflow-x: auto; padding-bottom: 1em;"
				on:wheel={handleWheel}
			>
				<ProgressIndicator
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
			</div>
		</Tile>
	{/if}
{/if}
