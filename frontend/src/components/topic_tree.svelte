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
	import { onMount, tick } from 'svelte';
	import { Button, ButtonSet, TreeView } from 'carbon-components-svelte';
	import type { TreeNode } from 'carbon-components-svelte/src/TreeView/TreeView.svelte';
	import type { BrokerRepositoryEntry, Treebranch } from '$lib/state';
	import { findbranchwithid, getAllTopicIds } from '$lib/helper';
	import TopicSelector from './topic_selector.svelte';

	export let broker: BrokerRepositoryEntry;
	let activeId = broker.selectedTopic?.id || '';
	let lastShownId = '';

	async function revealActiveTopic() {
		if (!treeview || !activeId) {
			return;
		}
		await tick();
		treeview?.showNode(activeId);
		lastShownId = activeId;
	}

	// Keep activeId in sync and expand tree when selectedTopic changes externally (e.g. URL restore)
	$: if (broker.selectedTopic?.id && broker.selectedTopic.id !== activeId) {
		activeId = broker.selectedTopic.id;
	}

	// When returning to the tree tab, the component is remounted and the tree is collapsed by default.
	// Re-reveal the active node so the selected topic is visible.
	$: if (treeview && activeId && activeId !== lastShownId) {
		revealActiveTopic();
	}

	function select(
		detail: TreeNode & {
			expanded: boolean;
			leaf: boolean;
		}
	) {
		broker.selectedTopic = findbranchwithid(detail.id.toString(), broker.topics) || null;
	}

	let treeview: TreeView;

	let searchValue = '';
	function revealTopic(topicId: string) {
		if (!topicId) {
			return;
		}
		treeview?.showNode(topicId);
		broker.selectedTopic = findbranchwithid(topicId, broker.topics) || null;
	}

	$: topicIds = getAllTopicIds(broker.topics);

	function sanitizeTree(nodes: Treebranch[]): TreeNode[] {
		return nodes.map((node) => {
			const sanitized: TreeNode = {
				id: node.id,
				text: node.text
			};
			if (node.children) {
				sanitized.children = sanitizeTree(node.children);
			}

			return sanitized;
		});
	}

	let sanitizedTopics: TreeNode[] = [];
	$: sanitizedTopics = sanitizeTree(broker.topics);

	onMount(() => {
		revealActiveTopic();
	});
</script>

<ButtonSet>
	<Button size="small" on:click={treeview?.expandAll} kind="secondary">Expand all</Button>
	<Button size="small" on:click={treeview?.collapseAll}>Collapse all</Button>
</ButtonSet>

<TopicSelector
	bind:value={searchValue}
	{topicIds}
	placeholder="Search for a topic..."
	labelText="Topic search"
	allowCustomValue={false}
	on:select={(event) => revealTopic(event.detail.value)}
	on:submit={(event) => revealTopic(event.detail.value)}
/>

<div class="overflow-auto treeview-col">
	<TreeView
		bind:this={treeview}
		bind:children={sanitizedTopics}
		bind:activeId
		on:select={({ detail }) => select(detail)}
	/>
</div>

<style>
	.overflow-auto {
		overflow: auto;
		height: calc(100vh - 9.35rem);
	}
</style>
