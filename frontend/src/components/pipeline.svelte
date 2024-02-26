<script lang="ts">
	import {
		Button,
		ButtonSet,
		ComboBox,
		StructuredList,
		StructuredListBody,
		StructuredListCell,
		StructuredListHead,
		StructuredListInput,
		StructuredListRow,
		TextInput
	} from 'carbon-components-svelte';
	import { findbranchwithid, type treebranch } from './topic_tree';
	import { Add, CheckmarkFilled, Save } from 'carbon-icons-svelte';

	export let pipelines: { id: number; text: string; pipeline: { topic: string }[] }[];
	export let broker: {
		topics: treebranch[];
		selectedTopic: treebranch | null;
		pipeline: { topic: string; timestamp?: string }[];
	};

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

	let selectedId;
	function pipelineSelected(e: Event) {
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

<StructuredList condensed selection selected="row-1-value">
	<StructuredListHead>
		<StructuredListRow head>
			<StructuredListCell head>Topic</StructuredListCell>
			<StructuredListCell head>Timestamp</StructuredListCell>
			<StructuredListCell head>{''}</StructuredListCell>
		</StructuredListRow>
	</StructuredListHead>
	<StructuredListBody>
		{#each broker.pipeline as item, index}
			<StructuredListRow on:click={clicked_row} label for="row-{index}">
				<StructuredListCell>{item.topic}</StructuredListCell>
				<StructuredListCell>{item.timestamp || ''}</StructuredListCell>
				<StructuredListInput id="row-{index}" value="row-{index}-value" />
				<StructuredListCell>
					<CheckmarkFilled
						class="bx--structured-list-svg"
						aria-label="select an option"
						title="select an option"
					/>
				</StructuredListCell>
			</StructuredListRow>
		{/each}
	</StructuredListBody>
</StructuredList>

<div style="display: flex">
	<div style="flex: 10">
		<TextInput bind:value={nextStepText} placeholder="Add topic to pipeline" />
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

<div style="display: flex">
	<div style="flex: 10">
		<TextInput bind:value={pipelineName} placeholder="Save pipeline as" />
	</div>

	<div style="flex: 1">
		<Button
			disabled={!pipelineName}
			icon={Save}
			iconDescription="Save current pipeline"
			size="field"
			on:click={save_pipeline}
			kind="secondary"
		/>
	</div>
</div>
