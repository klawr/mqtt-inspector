<script lang="ts">
	import { Button, ButtonSet, TreeView } from 'carbon-components-svelte';
	import { findbranchwithid, type treebranch } from './topic_tree';
	import type { TreeNode } from 'carbon-components-svelte/src/TreeView/TreeView.svelte';

	export let broker: {
		topics: treebranch[];
		selectedTopic: treebranch | null;
	};
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
	<Button size="sm" on:click={treeview?.expandAll} kind="secondary">Expand all</Button>
	<Button size="sm" on:click={treeview?.collapseAll}>Collapse all</Button>
</ButtonSet>

<TreeView
	bind:this={treeview}
	bind:children={broker.topics}
	bind:activeId
	bind:selectedIds
	on:select={({ detail }) => select(detail)}
	on:toggle={({ detail }) => console.log('toggle', detail)}
	on:focus={({ detail }) => console.log('focus', detail)}
/>
