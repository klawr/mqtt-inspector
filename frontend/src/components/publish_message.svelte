<script lang="ts">
	import {
		Accordion,
		AccordionItem,
		Button,
		ExpandableTile,
		Row,
		TextArea,
		TextInput,
		Tile
	} from 'carbon-components-svelte';
	import type { treebranch } from './topic_tree';
	import { requestPublishMqttMessage } from '$lib/socket';

	export let socket: WebSocket;
	export let selectedBroker: string;
	export let broker: {
		topics: treebranch[];
		selectedTopic: treebranch | null;
	};

	let topic: string;
	let payload: string;

	function send(e: Event) {
		stopPropagation(e);
		const ip = selectedBroker.split(':')[0];
		const port = selectedBroker.split(':')[1];
		requestPublishMqttMessage(ip, port, topic, payload, socket);
	}

	function setTopicToSelectedTopic() {
		topic = broker.selectedTopic?.id || '';
	}

	function stopPropagation(e: Event) {
		e.stopPropagation();
	}
</script>

<Row>
	<Accordion id="publish_accorrdion">
		<ExpandableTile>
			<div style="height: 2em" slot="above">
				<p>Publish message</p>
			</div>
			<div slot="below">
				<Tile light on:click={stopPropagation}>
					<div style="display: flex">
						<div style="flex: 1">
							<TextInput on:click={stopPropagation} labelText="Topic" bind:value={topic} />
						</div>
						<div style="margin-top: auto; margin-bottom: 0; flex: 1">
							<Button
								on:click={stopPropagation}
								disabled={!broker.selectedTopic?.id}
								on:click={setTopicToSelectedTopic}
								size="sm">Use selected topic</Button
							>
						</div>
					</div>
					<TextArea on:click={stopPropagation} labelText="Payload" bind:value={payload} />
					<Button on:click={send}>Send</Button>
				</Tile>
			</div>
		</ExpandableTile>
	</Accordion>
</Row>
