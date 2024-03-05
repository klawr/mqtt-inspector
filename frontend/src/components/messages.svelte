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
	import type { BrokerRepositoryEntry } from '$lib/state';
	import { Accordion, AccordionItem, CodeSnippet, Tile } from 'carbon-components-svelte';

	export let broker: BrokerRepositoryEntry;
</script>

{#if broker.selectedTopic}
	<div style="height: 8em;">
		<h4>Selected topic:</h4>
		<CodeSnippet light code={broker.selectedTopic?.id}></CodeSnippet>
	</div>

	{#if broker.selectedTopic?.messages.length}
		<Tile light>
			<h5>
				Latest message: {new Date(broker.selectedTopic?.messages[0].timestamp).toLocaleString()}
			</h5>
			<CodeSnippet wrapText type="multi" code={broker.selectedTopic?.messages[0].text} />
		</Tile>
	{/if}

	{#if broker.selectedTopic?.messages.length > 1}
		<Tile light>
			<h5>History ({broker.selectedTopic.messages.length})</h5>
			<div style="overflow-y: auto; max-height: 50vh;">
				<Accordion>
					{#each broker.selectedTopic?.messages as message}
						<AccordionItem
							title={new Date(message.timestamp).toLocaleString() +
								(message.delta_t ? ' (' + message.delta_t + ' ms)' : '')}
						>
							<CodeSnippet wrapText type="multi" code={message.text} />
						</AccordionItem>
					{/each}
				</Accordion>
			</div>
		</Tile>
	{/if}
{/if}
