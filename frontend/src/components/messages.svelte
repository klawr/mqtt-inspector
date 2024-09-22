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
	import { CodeSnippet, RadioTile, Tile, TileGroup } from 'carbon-components-svelte';
	import Monaco from './monaco.svelte';

	export let selectedTopic: Treebranch | null; // Can't be null.

	let selectedMessage = selectedTopic?.messages[0]!;
	function selectMessage(message: CustomEvent) {
		selectedMessage = selectedTopic?.messages
			.find(msg => msg.timestamp == message.detail)!;
	}
</script>

{#if selectedTopic}
	<div style="height: 8em;">
		<h4>Selected topic:</h4>
		<CodeSnippet light code={selectedTopic?.id}></CodeSnippet>
	</div>

	{#if selectedTopic?.messages.length}
		<Tile light>
			<h5>
				Selected message: {new Date(selectedMessage.timestamp).toLocaleString()}
			</h5>
			<div style="height: 30em;">
				<Monaco readonly bind:code={selectedMessage.text} />
			</div>
		</Tile>
	{/if}

	{#if selectedTopic?.messages.length > 1}
		<Tile light>
			<h5>History ({selectedTopic.messages.length})</h5>
			<TileGroup on:select={selectMessage} selected={selectedMessage?.timestamp}>
				{#each selectedTopic?.messages as message}
					<RadioTile value={message.timestamp} style="height: 1em">
						{new Date(message.timestamp).toLocaleString() +
								(message.delta_t ? ' (' + message.delta_t + ' ms)' : '')}
					</RadioTile>
				{/each}
			</TileGroup>
		</Tile>
	{/if}
{/if}
