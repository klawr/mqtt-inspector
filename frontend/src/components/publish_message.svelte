<script lang="ts">
	import {
		Button,
		ComboBox,
		ExpandableTile,
		Row,
		TextArea,
		TextInput,
		Tile
	} from 'carbon-components-svelte';
	import { requestPublishMqttMessage } from '$lib/socket';
	import { Add } from 'carbon-icons-svelte';
	import type { BrokerRepositoryEntry, Command } from '$lib/state';

	export let savedCommands: Command[];
	export let socket: WebSocket;
	export let selectedBroker: string;
	export let broker: BrokerRepositoryEntry;

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

	function saved_message_selected(e: any) {
		const item = e.detail.selectedItem;
		topic = item.topic;
		payload = item.payload;
	}

	function save_message() {
		const message = JSON.stringify({
			jsonrpc: '2.0',
			method: 'save_publish',
			params: { name: save_command_name, topic, payload }
		});
		socket.send(message);
		save_command_name = '';
	}

	let save_command_name = '';
</script>

<Row>
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
							disabled={!broker.selectedTopic?.id}
							on:click={setTopicToSelectedTopic}
							size="field">Use selected topic</Button
						>
					</div>
					<div style="display: flex; flex: 1">
						<div style="flex: 1">
							<TextInput
								labelText="Save command name"
								bind:value={save_command_name}
								placeholder="Add name"
							/>
						</div>
						<div style="margin-top: auto; margin-bottom: 0; flex: 1">
							<Button
								icon={Add}
								disabled={!topic || !save_command_name}
								on:click={save_message}
								size="field"
							/>
						</div>
						<div style="flex: 1">
							<ComboBox
								on:select={saved_message_selected}
								titleText="Saved commands"
								placeholder="Search..."
								items={savedCommands}
							/>
						</div>
					</div>
				</div>
				<TextArea on:click={stopPropagation} labelText="Payload" bind:value={payload} />
				<Button on:click={send}>Send</Button>
			</Tile>
		</div>
	</ExpandableTile>
</Row>
