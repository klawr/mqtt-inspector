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
	import { Button, ComboBox, TextInput, Tile, Toggle } from 'carbon-components-svelte';
	import { requestCommandAddition, requestPublishMqttMessage } from '$lib/socket';
	import { session } from '../store';
	import { Add, TrashCan } from 'carbon-icons-svelte';
	import type { BrokerRepositoryEntry, Command } from '$lib/state';
	import { getAllTopicIds } from '$lib/helper';
	import OverwriteCommand from './dialogs/overwrite_command.svelte';
	import Monaco from './monaco.svelte';
	import TopicSelector from './topic_selector.svelte';

	export let savedCommands: Command[];
	export let socket: WebSocket;
	export let selectedBroker: string;
	export let broker: BrokerRepositoryEntry;
	export let topic = '';
	export let payload = '';
	export let retain = false;
	/** MQTT 5 user properties as [key, value] rows. */
	export let userProperties: [string, string][] = [];
	export let save_command_name = '';
	export let selectedCommandId = '';

	$: topicIds = getAllTopicIds(broker.topics);

	function send(e: Event) {
		stopPropagation(e);
		requestPublishMqttMessage(selectedBroker, topic, payload, retain, socket, userProperties);
	}

	function addUserProperty() {
		userProperties = [...userProperties, ['', '']];
	}

	function removeUserProperty(index: number) {
		userProperties = userProperties.filter((_, i) => i !== index);
	}

	function setTopicToSelectedTopic() {
		topic = broker.selectedTopic?.id || '';
	}

	function stopPropagation(e: Event) {
		e.stopPropagation();
	}

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
</script>

<OverwriteCommand
	bind:open={overwriteCommandOpen}
	bind:socket
	bind:save_command_name
	bind:topic
	bind:payload
/>

<Tile
	light
	on:click={stopPropagation}
	style="height: calc(100vh - 5em); display: flex; flex-direction: column;"
>
	<div style="display: flex; width: 100%">
		<div style="flex: 4">
			<TopicSelector
				bind:value={topic}
				{topicIds}
				labelText="Topic"
				placeholder="Select or type a topic"
			/>
		</div>
		<div style="margin-top: auto; margin-bottom: 0; flex: 3">
			<Button disabled={!broker.selectedTopic?.id} on:click={setTopicToSelectedTopic} size="field"
				>Use selected</Button
			>
		</div>
		<div style="margin-top: auto; margin-bottom: auto; flex: 2">
			<TextInput labelText="Save command" bind:value={save_command_name} placeholder="Add name" />
		</div>
		<div style="margin-top: auto; margin-bottom: 0; flex: 0">
			<Button
				tooltipAlignment="end"
				tooltipPosition="bottom"
				iconDescription="Save command"
				icon={Add}
				disabled={!topic || !save_command_name || !$session.canEdit}
				on:click={save_message}
				size="field"
			/>
		</div>
		<div style="margin-top: auto; margin-bottom: 0; flex: 2">
			<ComboBox
				bind:selectedId={selectedCommandId}
				on:clear={() => {
					selectedCommandId = '';
				}}
				on:select={saved_message_selected}
				titleText="Saved commands"
				placeholder="Search..."
				items={savedCommands}
			/>
		</div>
		<div style="margin-top: auto; margin-bottom: 0; flex: 0">
			<Button
				disabled={!selectedCommandId || !$session.canEdit}
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
	<div style="flex: 1; min-height: 0; margin-top: 0.5em;">
		<Monaco bind:code={payload} />
	</div>

	{#if userProperties.length}
		<div class="user-properties">
			{#if broker.protocol === '3.1.1'}
				<p class="hint">
					This broker is connected via MQTT 3.1.1, so user properties will not be sent.
				</p>
			{/if}
			{#each userProperties as property, i}
				<div class="user-property">
					<TextInput
						size="sm"
						hideLabel
						labelText="Key"
						placeholder="Key"
						bind:value={property[0]}
					/>
					<TextInput
						size="sm"
						hideLabel
						labelText="Value"
						placeholder="Value"
						bind:value={property[1]}
					/>
					<Button
						kind="ghost"
						size="small"
						icon={TrashCan}
						iconDescription="Remove user property"
						tooltipPosition="left"
						on:click={() => removeUserProperty(i)}
					/>
				</div>
			{/each}
		</div>
	{/if}

	<div style="display: flex; width: 100%; align-items: center; gap: 0.5em; margin-top: 0.5em;">
		<Toggle size="sm" bind:toggled={retain} labelText="Retain message" />
		<Button kind="ghost" size="small" icon={Add} on:click={addUserProperty}>
			Add user property
		</Button>
		<Button on:click={send} style="flex: 1">Send</Button>
	</div>
</Tile>

<style>
	.user-properties {
		max-height: 12em;
		overflow: auto;
		margin-top: 0.5em;
		display: flex;
		flex-direction: column;
		gap: 0.25em;
	}

	.user-property {
		display: grid;
		grid-template-columns: 1fr 2fr auto;
		gap: 0.5em;
		align-items: center;
	}

	.hint {
		font-size: 0.75rem;
		color: var(--cds-text-helper);
	}
</style>
