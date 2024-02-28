<script lang="ts">
	import {
		Accordion,
		AccordionItem,
		CodeSnippet,
		ExpandableTile,
		Tile
	} from 'carbon-components-svelte';
	import type { treebranch } from './topic_tree';

	export let broker: {
		topics: treebranch[];
		selectedTopic: treebranch | null;
	};
</script>

{#if broker.selectedTopic}
	<h4>{broker.selectedTopic?.id}</h4>

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
			<h5>History</h5>
			<div style="overflow-y: auto; max-height: 50vh">
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
