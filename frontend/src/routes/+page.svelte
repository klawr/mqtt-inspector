<script lang="ts">
	import { onMount } from 'svelte';
	import TopicTree from '../components/topic_tree.svelte';
	import { addToTopicTree, findbranchwithid, type treebranch } from '../components/topic_tree';
	import {
		Content,
		Grid,
		Header,
		RadioButton,
		RadioButtonGroup,
		SideNav,
		SideNavDivider,
		SideNavItems,
		SideNavLink,
		SideNavMenu,
		SkipToContent,
		Theme
	} from 'carbon-components-svelte';
	import 'carbon-components-svelte/css/all.css';
	import Messages from '../components/messages.svelte';
	import AddBroker from '../components/add_broker.svelte';
	import { Add } from 'carbon-icons-svelte';
	import PublishMessage from '../components/publish_message.svelte';
	import type { CarbonTheme } from 'carbon-components-svelte/src/Theme/Theme.svelte';
	import { page } from '$app/stores';

	let socket: WebSocket;
	let selectedBroker: string;
	let savedCommands: {id: string, text: string, topic: string, payload: string}[] = [];
	const brokerRepository: {
		[key: string]: {
			topics: treebranch[];
			selectedTopic: treebranch | null;
		};
	} = {};

	const decoder = new TextDecoder('utf-8');
	function processMQTTMessage(json: any, decoder: TextDecoder) {
		const source = json.source;
			if (!brokerRepository[source]) {
				brokerRepository[source] = { topics: [], selectedTopic: null };
			}

			const payload = decoder.decode(new Uint8Array(json.payload));
			const timestamp = json.timestamp;
			if (selectedBroker === undefined) {
				selectedBroker = source;
			}

			if (selectedBroker == source) {
				brokerRepository[source].topics = addToTopicTree(
					json.topic,
					brokerRepository[source].topics,
					payload,
					timestamp
				);
			}
			if (selectedTopic) {
				selectedTopic =
					findbranchwithid(selectedTopic?.id.toString(), brokerRepository[source].topics) ||
					selectedTopic;
			}

	}

	function processConfigs(params: any) {
		savedCommands = JSON.parse(params).map((e: any, id: number) => ({
			id: `${id}`,
			text: e.name,
			topic: e.topic,
			payload: e.payload
		}));
	}

	function initializeWebSocket() {
		socket = new WebSocket(`ws://${$page.url.host}/ws`);

		socket.onopen = (event) => {
			console.log('WebSocket connection opened:', event);
		};

		socket.onmessage = (event) => {
			const message = event.data;
			const json = JSON.parse(message);
			switch (json.method) {
				case "mqtt_message":
					processMQTTMessage(json.params, decoder);
					break;
				case "commands":
					processConfigs(json.params);
					break;
				default:
					break;
			}
		};

		socket.onclose = (event) => {
			console.log('WebSocket connection closed:', event);
		};

		socket.onerror = (event) => {
			console.error('WebSocket error:', event);
		};
	}

	onMount(initializeWebSocket);

	let isSideNavOpen = false;
	let selectedTopic: treebranch | null = null;
	let addMqttBrokerModalOpen = false;
	let theme: CarbonTheme = 'g90';
</script>

<Theme bind:theme />

<AddBroker {socket} bind:open={addMqttBrokerModalOpen} />

<Header platformName="MQTT-Inspector" bind:isSideNavOpen>
	<svelte:fragment slot="skip-to-content">
		<SkipToContent />
	</svelte:fragment>
</Header>

<SideNav bind:isOpen={isSideNavOpen}>
	<SideNavItems>
		{#each Object.keys(brokerRepository) as broker}
			<SideNavLink
				text={broker}
				isSelected={selectedBroker === broker}
				on:click={() => {
					selectedTopic = brokerRepository[broker].selectedTopic;
					selectedBroker = broker;
				}}
			/>
		{/each}
		<SideNavLink
			icon={Add}
			text="Add Broker"
			on:click={() => {
				addMqttBrokerModalOpen = true;
			}}
		/>
		<div style="flex: 1" />
		<SideNavDivider />
		<SideNavMenu text="Theme">
			<div style="margin-top: auto; margin-bottom: 0; margin: 1em">
				<RadioButtonGroup orientation="vertical" legendText="Carbon theme" bind:selected={theme}>
					{#each ['white', 'g10', 'g90', 'g100'] as value}
						<RadioButton labelText={value} {value} />
					{/each}
				</RadioButtonGroup>
			</div>
		</SideNavMenu>
	</SideNavItems>
</SideNav>

<Content>
	{#if brokerRepository[selectedBroker]}
		<Grid fullWidth>
			<div style="height: calc(100vh - 8em) !important; display: flex; flex-direction: column">
				<div style="display: flex">
					<div style="flex: 1; margin: 1em; min-width: 30em; max-width: 50em">
						<TopicTree bind:broker={brokerRepository[selectedBroker]} />
					</div>
					<div style="flex: 1; margin: 1em; min-width: 30em">
						<Messages bind:broker={brokerRepository[selectedBroker]} />
					</div>
				</div>
				<div style="flex: 1;" />
				<PublishMessage
					bind:savedCommands
					bind:selectedBroker
					bind:socket
					bind:broker={brokerRepository[selectedBroker]}
				/>
			</div>
		</Grid>
	{/if}
</Content>

<style>
	:global(.bx--side-nav__items) {
		display: flex;
		flex-direction: column;
	}

	:global(.bx--side-nav) {
		border-right: 1px !important;
		border-style: solid !important;
	}
</style>
