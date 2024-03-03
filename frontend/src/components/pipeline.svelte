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
	import {
		Button,
		ComboBox,
		StructuredList,
		StructuredListBody,
		StructuredListCell,
		StructuredListHead,
		StructuredListInput,
		StructuredListRow,
		TextInput
	} from 'carbon-components-svelte';
	import { Add, ArrowDown, ArrowUp, CheckmarkFilled, Save, TrashCan } from 'carbon-icons-svelte';
	import type { BrokerRepositoryEntry, SavedPipeline } from '$lib/state';
	import { findbranchwithid } from '$lib/helper';

	export let pipelines: SavedPipeline[];
	export let broker: BrokerRepositoryEntry;

	export let socket: WebSocket;

	function clicked_row(e: Event) {
		const value = (e.target as any).value;
		if (value === undefined) {
			return;
		}
		const index = value.split('-')[1];
		const selected = broker.pipeline[+index];
		broker.selectedTopic = findbranchwithid(selected.topic, broker.topics) || null;
	}

	function reset() {
		broker.pipeline = broker.pipeline.map((e) => ({
			topic: e.topic,
			timestamp: ''
		}));
	}

	let pipelineName = '';
	function save_pipeline() {
		const new_pipeline = broker.pipeline.map((e) => ({
			topic: e.topic
		}));
		const message = JSON.stringify({
			jsonrpc: '2.0',
			method: 'save_pipeline',
			params: {
				name: pipelineName,
				pipeline: new_pipeline
			}
		});

		socket.send(message);
		pipelineName = '';
	}

	let selectedId: number | undefined;
	function pipelineSelected(e: Event) {
		if (selectedId === undefined) {
			return;
		}
		broker.pipeline = pipelines[selectedId].pipeline.map((e: any) => ({
			topic: e.topic
		}));
	}

	let nextStepText = '';
	function add_to_pipeline() {
		broker.pipeline.push({
			topic: nextStepText
		});
		nextStepText = '';
	}

	let selectedRow = '';
	function removeSelectedRow() {
		if (!selectedRow) {
			return;
		}
		const index = +selectedRow.split('-')[1];
		broker.pipeline = broker.pipeline.filter((e, i) => i !== index);
	}

	function moveSelectedRow(direction: number) {
		reset();
		if (!selectedRow) {
			return;
		}
		const index = +selectedRow.split('-')[1];
		const newIndex = index + direction;
		if (newIndex < 0 || newIndex >= broker.pipeline.length) {
			return;
		}
		const temp = broker.pipeline[index];
		broker.pipeline[index] = broker.pipeline[newIndex];
		broker.pipeline[newIndex] = temp;

		selectedRow = `row-${newIndex}-value`;
	}
</script>

<div style="display: flex">
	<div style="flex: 1">
		<Button size="field" on:click={reset} kind="secondary">Reset</Button>
	</div>
	<div style="flex: 11">
		<ComboBox
			bind:selectedId
			on:select={pipelineSelected}
			bind:items={pipelines}
			placeholder="Select pipeline"
		/>
	</div>
</div>

<div style="margin-bottom: -5em">
	<StructuredList condensed selection bind:selected={selectedRow}>
		<StructuredListHead>
			<StructuredListRow head>
				<StructuredListCell head>Topic</StructuredListCell>
				<StructuredListCell head>
					<div style="text-align: end;">Time</div>
				</StructuredListCell>
				<StructuredListCell head />
			</StructuredListRow>
		</StructuredListHead>
		<StructuredListBody>
			{#each broker.pipeline as item, index}
				<StructuredListRow on:click={clicked_row} label for="row-{index}">
					<StructuredListCell>{item.topic}</StructuredListCell>
					<StructuredListCell>
						<div style="text-align: end;">
							{item.delta_t !== undefined ? `${item.delta_t} ms` : ' - '}
						</div>
					</StructuredListCell>
					<StructuredListCell>
						<div style="width: 0">
							<StructuredListInput id="row-{index}" value="row-{index}-value" />
							{#if (selectedRow === `row-${index}-value` && !broker.selectedTopic?.id) || broker.selectedTopic?.id === item.topic}
								<CheckmarkFilled />
							{/if}
						</div>
					</StructuredListCell>
				</StructuredListRow>
			{/each}
		</StructuredListBody>
	</StructuredList>
</div>
<div style="margin-bottom: -5em">
	<StructuredList>
		<StructuredListRow>
			<StructuredListCell head>Total:</StructuredListCell>
			<StructuredListCell head>
				<div style="text-align: end;">
					{broker.pipeline.reduce((pre, cur) => pre + (cur.delta_t || 0), 0)} ms
				</div>
			</StructuredListCell>
			<StructuredListCell head />
		</StructuredListRow>
	</StructuredList>
</div>
<div style="display: flex">
	<div style="flex: 11;">
		<TextInput bind:value={nextStepText} placeholder="Add topic to pipeline..." />
	</div>
	<div style="flex: 1">
		<Button
			disabled={!nextStepText}
			iconDescription="Add"
			icon={Add}
			on:click={add_to_pipeline}
			size="field"
		/>
	</div>
</div>
<Button
	on:click={() => moveSelectedRow(-1)}
	icon={ArrowUp}
	iconDescription="Move selected row up"
	disabled={!selectedRow ||
		(!!broker.selectedTopic?.id &&
			broker.pipeline[+selectedRow.split('-')[1]]?.topic !== broker.selectedTopic?.id)}
/>
<Button
	on:click={() => moveSelectedRow(1)}
	icon={ArrowDown}
	iconDescription="Move selected row down"
	disabled={!selectedRow ||
		(!!broker.selectedTopic?.id &&
			broker.pipeline[+selectedRow.split('-')[1]]?.topic !== broker.selectedTopic?.id)}
/>
<Button
	on:click={removeSelectedRow}
	icon={TrashCan}
	iconDescription="Remove selected row"
	disabled={!selectedRow ||
		(!!broker.selectedTopic?.id &&
			broker.pipeline[+selectedRow.split('-')[1]]?.topic !== broker.selectedTopic?.id)}
/>

<div style="display: flex">
	<div style="flex: 10">
		<TextInput bind:value={pipelineName} placeholder="Save pipeline as..." />
	</div>

	<div style="flex: 1">
		<Button
			disabled={!pipelineName}
			icon={Save}
			iconDescription="Save current pipeline"
			size="field"
			on:click={save_pipeline}
		/>
	</div>
</div>
