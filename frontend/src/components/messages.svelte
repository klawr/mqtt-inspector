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

	// NOTE: Not sure why but the Accordion is just always open.
	// NOTE: Just remove this and the "meneedfix" div if that is fixed.
	function accordionFix(e: Event) {
		const target = e.target as HTMLElement;
		let content = target.parentElement?.children[1].children[0] as HTMLElement;
		if (!content || !content.classList.contains('meneedfix')) {
			content = target.parentElement?.parentElement?.children[1].children[0] as HTMLElement;
		}

		if (content?.classList.contains('meneedfix')) {
			if (content.style.display === 'none') {
				content.style.display = 'block';
			} else {
				content.style.display = 'none';
			}
		}
	}
</script>

{#if broker.selectedTopic}
	<h4>{broker.selectedTopic?.id}</h4>

	{#if broker.selectedTopic?.messages.length}
		<Tile light>
			<h5>Latest message: {broker.selectedTopic?.messages[0].timestamp}</h5>
			<CodeSnippet wrapText type="multi" code={broker.selectedTopic?.messages[0].text} />
		</Tile>
	{/if}

	{#if broker.selectedTopic?.messages.length > 1}
		<Tile light>
			<h5>History</h5>
			<div style="overflow-y: auto; max-height: 50vh">
				<Accordion>
					{#each broker.selectedTopic?.messages as message}
						<AccordionItem on:click={accordionFix} title={message.timestamp}>
							<!-- svelte-ignore a11y-click-events-have-key-events -->
							<!-- svelte-ignore a11y-no-static-element-interactions -->
							<div class="meneedfix" style="display: none">
								<CodeSnippet wrapText type="multi" code={message.text} />
							</div>
						</AccordionItem>
					{/each}
				</Accordion>
			</div>
		</Tile>
	{/if}
{/if}
