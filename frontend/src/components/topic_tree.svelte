<script lang="ts">
	import { Button, ButtonSet, TreeView } from 'carbon-components-svelte';
	import type { TreeNode } from 'carbon-components-svelte/src/TreeView/TreeView.svelte';
	import type { BrokerRepositoryEntry } from '$lib/state';
	import { findbranchwithid } from '$lib/helper';

	export let broker: BrokerRepositoryEntry;
	let activeId = broker.selectedTopic?.id || '';
	let selectedIds: string[] = [];

	function select(
		detail: TreeNode & {
			expanded: boolean;
			leaf: boolean;
		}
	) {
		broker.selectedTopic = findbranchwithid(detail.id.toString(), broker.topics) || null;
	}

	let treeview: TreeView;
</script>

<ButtonSet>
	<Button size="small" on:click={treeview?.expandAll} kind="secondary">Expand all</Button>
	<Button size="small" on:click={treeview?.collapseAll}>Collapse all</Button>
</ButtonSet>

<TreeView
	bind:this={treeview}
	bind:children={broker.topics}
	bind:activeId
	bind:selectedIds
	on:select={({ detail }) => select(detail)}
/>
