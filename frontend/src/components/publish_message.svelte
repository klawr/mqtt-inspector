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
	import { Button, ComboBox, TextInput, Tile } from 'carbon-components-svelte';
	import { requestCommandAddition, requestPublishMqttMessage } from '$lib/socket';
	import { Add, TrashCan } from 'carbon-icons-svelte';
	import type { BrokerRepositoryEntry, Command } from '$lib/state';
	import OverwriteCommand from './dialogs/overwrite_command.svelte';
	import Monaco from './monaco.svelte';
	import Accordion from 'carbon-components-svelte/src/Accordion/Accordion.svelte';
	import AccordionItem from 'carbon-components-svelte/src/Accordion/AccordionItem.svelte';

	export let savedCommands: Command[];
	export let socket: WebSocket;
	export let selectedBroker: string;
	export let broker: BrokerRepositoryEntry;

	let topic: string;
	let payload: string;

	function send(e: Event) {
		stopPropagation(e);
		requestPublishMqttMessage(selectedBroker, topic, payload, socket);
	}

	function setTopicToSelectedTopic() {
		topic = broker.selectedTopic?.id || '';
	}

	function stopPropagation(e: Event) {
		e.stopPropagation();
	}

	let selectedCommandId: string;
	function saved_message_selected(e: CustomEvent) {
		const item = e.detail.selectedItem;
		if (!item) {
			return;
		}
		selectedCommandId = item.id;
		topic = item.topic;
		payload = item.payload;
	}

	function remove_command() {
		const selectedCommand = savedCommands.find((c) => c.id === selectedCommandId)?.text;

		selectedCommandId = '';
		if (!selectedCommand) {
			return;
		}

		const message = JSON.stringify({
			jsonrpc: '2.0',
			method: 'remove_command',
			params: { name: selectedCommand }
		});
		socket.send(message);
	}

	let overwriteCommandOpen = false;
	function save_message() {
		if (savedCommands.find((c) => c.text === save_command_name)) {
			overwriteCommandOpen = true;
			return;
		}
		requestCommandAddition(save_command_name, topic, payload, socket);
		save_command_name = '';
	}

	let save_command_name = '';

	let open = false;
	function clicked() {
		setTimeout(() => {
			if (open) {
				scrollBy({ top: 1000, behavior: 'smooth' });
			}
		}, 0);
	}
</script>

<OverwriteCommand
	bind:open={overwriteCommandOpen}
	bind:socket
	bind:save_command_name
	bind:topic
	bind:payload
/>

<Accordion>
	<AccordionItem title="Publish message" on:click={clicked} bind:open>
		<Tile light on:click={stopPropagation}>
			<div style="display: flex; width: 100%; bottom: 0">
				<div style="flex: 4">
					<TextInput on:click={stopPropagation} labelText="Topic" bind:value={topic} />
				</div>
				<div style="margin-top: auto; margin-bottom: 0; flex: 3">
					<Button
						disabled={!broker.selectedTopic?.id}
						on:click={setTopicToSelectedTopic}
						size="field">Use selected</Button
					>
				</div>
				<div style="margin-top: auto; margin-bottom: auto; flex: 2">
					<TextInput
						labelText="Save command"
						bind:value={save_command_name}
						placeholder="Add name"
					/>
				</div>
				<div style="margin-top: auto; margin-bottom: 0; flex: 0">
					<Button
						tooltipAlignment="end"
						tooltipPosition="bottom"
						iconDescription="Save command"
						icon={Add}
						disabled={!topic || !save_command_name}
						on:click={save_message}
						size="field"
					/>
				</div>
				<div style="margin-top: auto; margin-bottom: 0; flex: 2">
					<ComboBox
						bind:selectedId={selectedCommandId}
						on:clear={() => (selectedCommandId = '')}
						on:select={saved_message_selected}
						titleText="Saved commands"
						placeholder="Search..."
						items={savedCommands}
					/>
				</div>
				<div style="margin-top: auto; margin-bottom: 0; flex: 0">
					<Button
						disabled={!selectedCommandId}
						tooltipAlignment="end"
						tooltipPosition="bottom"
						iconDescription="Delete selected command"
						kind="danger-ghost"
						size="field"
						icon={TrashCan}
						on:click={remove_command}
					/>
				</div>
			</div>
			<div style="height: 30em">
				<Monaco bind:result={payload} bind:code={payload} />
			</div>

			<Button on:click={send}>Send</Button>
		</Tile>
	</AccordionItem>
</Accordion>
