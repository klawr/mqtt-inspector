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
	import { Button, ButtonSet, ComboBox, TreeView } from 'carbon-components-svelte';
	import type { TreeNode } from 'carbon-components-svelte/src/TreeView/TreeView.svelte';
	import type { BrokerRepositoryEntry, Treebranch } from '$lib/state';
	import { findbranchwithid, getAllTopics, shouldFilterItem } from '$lib/helper';

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

	let searchTopic: Treebranch | undefined;
	function selectSearchTopic(e: CustomEvent) {
		searchTopic = e.detail.selectedItem;
		if (!searchTopic) {
			return;
		}
		treeview.showNode(searchTopic.id);
		broker.selectedTopic = findbranchwithid(searchTopic.id, broker.topics) || null;
		searchTopic = undefined;
	}

	let searchTopics: { text: string; id: string }[] = [];
	$: {
		searchTopics = getAllTopics(broker.topics).map((topic) => {
			return { text: topic.id, id: topic.id };
		});
	}
</script>

<ButtonSet>
	<Button size="small" on:click={treeview?.expandAll} kind="secondary">Expand all</Button>
	<Button size="small" on:click={treeview?.collapseAll}>Collapse all</Button>
</ButtonSet>

<ComboBox
	placeholder="Search for a topic..."
	selectedId={searchTopic?.id}
	items={searchTopics}
	value={searchTopic?.text}
	on:select={selectSearchTopic}
	{shouldFilterItem}
/>

<div class="overflow-auto treeview-col">
	<TreeView
		bind:this={treeview}
		bind:children={broker.topics}
		bind:activeId
		bind:selectedIds
		on:select={({ detail }) => select(detail)}
	/>
</div>

<style>
	.overflow-auto {
		overflow: auto;
		height: calc(100vh - 9rem);
	}
</style>
