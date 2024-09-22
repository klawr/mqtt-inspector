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
	import { onMount } from 'svelte';
	import TopicTree from '../components/topic_tree.svelte';
	import {
		Button,
		Content,
		Grid,
		Header,
		InlineNotification,
		RadioButton,
		RadioButtonGroup,
		SideNav,
		SideNavDivider,
		SideNavItems,
		SideNavLink,
		SideNavMenu,
		SkipToContent,
		Tab,
		TabContent,
		Tabs,
		Theme
	} from 'carbon-components-svelte';
	import 'carbon-components-svelte/css/all.css';
	import Messages from '../components/messages.svelte';
	import AddBroker from '../components/dialogs/add_broker.svelte';
	import { Add, CircleDash, CircleSolid, Connect, LogoGithub, TrashCan } from 'carbon-icons-svelte';
	import PublishMessage from '../components/publish_message.svelte';
	import type { CarbonTheme } from 'carbon-components-svelte/src/Theme/Theme.svelte';
	import { page } from '$app/stores';
	import Pipeline from '../components/pipeline.svelte';
	import { AppState } from '$lib/state';
	import {
		processBrokerRemoval,
		processBrokers,
		processConfigs,
		processConnectionStatus,
		processMQTTMessage,
		processPipelines
	} from '$lib/ws_msg_handling';
	import RemoveBroker from '../components/dialogs/remove_broker.svelte';
	import { requestMqttBrokerConnection } from '$lib/socket';
	import { selectedTheme, availableThemes } from '../store';

	let socket: WebSocket;
	let app = new AppState();

	const decoder = new TextDecoder('utf-8');

	let socketConnected = false;
	function initializeWebSocket() {
		socket = new WebSocket(`ws://${$page.url.host}/ws`);

		socket.onopen = () => {
			socketConnected = true;
		};

		socket.onmessage = (event) => {
			const message = event.data;
			const json = JSON.parse(message);
			switch (json.method) {
				case 'broker_removal':
					app = processBrokerRemoval(json.params, app);
					break;
				case 'mqtt_connection_status':
					app = processConnectionStatus(json.params, app);
					break;
				case 'mqtt_brokers':
					app.brokerRepository = processBrokers(json.params, decoder, app.brokerRepository);
					break;
				case 'mqtt_message':
					app = processMQTTMessage(json.params, decoder, app);
					break;
				case 'commands':
					app.commands = processConfigs(json.params);
					break;
				case 'pipelines':
					app.pipelines = processPipelines(json.params);
					break;
				default:
					break;
			}
		};

		socket.onclose = () => {
			socketConnected = false;
			console.log('WebSocket connection closed.');
		};

		socket.onerror = (event) => {
			console.error('WebSocket error:', event);
		};
	}

	onMount(initializeWebSocket);

	let isSideNavOpen = false;
	let addMqttBrokerModalOpen = false;
	let removeMqttBrokerModalOpen = false;

	let theme: CarbonTheme;
	selectedTheme.subscribe((t) => {
		theme = t.id;
	});

	function themeChanged(e: Event): void {
		const newId = e?.target as unknown as { value: string };
		if (!newId) {
			return;
		}
		const newTheme = availableThemes.find((t) => t.id == newId.value);
		if (!newTheme) {
			return;
		}
		selectedTheme.set(newTheme);
	}
</script>

<Theme bind:theme />

<AddBroker bind:socket bind:open={addMqttBrokerModalOpen} />
<RemoveBroker bind:app bind:socket bind:open={removeMqttBrokerModalOpen} />

<Header platformName="MQTT-Inspector" bind:isSideNavOpen>
	<svelte:fragment slot="skip-to-content">
		<SkipToContent />
	</svelte:fragment>
	<div style="flex: 1" />

	{#if app.brokerRepository[app.selectedBroker]}
		<Button
			iconDescription="Remove MQTT Broker"
			tooltipPosition="bottom"
			tooltipAlignment="end"
			kind="danger-ghost"
			icon={TrashCan}
			on:click={() => {
				removeMqttBrokerModalOpen = true;
			}}
		/>
	{/if}

	{#if socketConnected}
		<Button
			kind="ghost"
			icon={Connect}
			tooltipPosition="bottom"
			tooltipAlignment="end"
			iconDescription="WebSocket Connected"
		/>
	{:else}
		<Button
			on:click={initializeWebSocket}
			kind="danger-ghost"
			icon={Connect}
			tooltipPosition="bottom"
			tooltipAlignment="end"
			iconDescription="WebSocket Disconnected"
		/>
	{/if}
	<Button
		icon={LogoGithub}
		tooltipPosition="bottom"
		tooltipAlignment="end"
		iconDescription="Fork me on GitHub!"
		href="https://github.com/klawr/mqtt-inspector"
	></Button>
</Header>

<SideNav bind:isOpen={isSideNavOpen}>
	<SideNavItems>
		{#each Object.keys(app.brokerRepository) as broker}
			<SideNavLink
				icon={app.brokerRepository[broker].markedForDeletion
					? TrashCan
					: app.brokerRepository[broker].connected && socketConnected
						? CircleSolid
						: CircleDash}
				text={broker}
				isSelected={app.selectedBroker === broker}
				on:click={() => {
					app.selectedBroker = broker;
				}}
			/>
		{/each}
		<div style={socketConnected ? '' : 'pointer-events: none; opacity: 0.5'}>
			<SideNavLink
				icon={Add}
				text="Add Broker"
				on:click={() => {
					addMqttBrokerModalOpen = true;
				}}
			/>
		</div>
		<div style="flex: 1" />
		<SideNavDivider />
		<SideNavMenu text="Theme">
			<div style="margin: auto 1em 0px 1em">
				<RadioButtonGroup orientation="vertical" legendText="Carbon theme" bind:selected={theme}>
					{#each availableThemes as value}
						<RadioButton labelText={value.id} value={value.id} on:change={themeChanged} />
					{/each}
				</RadioButtonGroup>
			</div>
		</SideNavMenu>
	</SideNavItems>
</SideNav>

<Content style="padding: 0">
	{#if app.brokerRepository[app.selectedBroker]?.markedForDeletion}
		<div style="margin-left: 1em; margin-top: 3em; display: flex">
			<div style="flex: 0"></div>
			<InlineNotification
				hideCloseButton
				title="Marked for deletion"
				subtitle="This connection is marked for deletion. It is not connected anymore and will disappear on refresh."
			/>
			<div style="display: flex; height: 4em; margin-top: 1.2em; margin-left: 1em">
				<Button
					on:click={() => requestMqttBrokerConnection(app.selectedBroker, socket)}
					kind="tertiary"
					size="field">Reconnect!</Button
				>
			</div>
		</div>
	{/if}

	{#if app.brokerRepository[app.selectedBroker]}
		<Grid fullWidth style="height: calc(100vh - 7em); overflow-y: auto;">
			<div style="display:flex; flex-direction: column; height: calc(100vh - 4em)">
				<div style="display:flex; flex-direction: row; flex: 1;">
					<div style="width: 40%; min-width: 30em; max-width: 50em">
						<Tabs autoWidth type="container">
							<Tab label="Treeview" />
							<Tab label="Pipeline" />
							<svelte:fragment slot="content">
							<TabContent>
								<TopicTree bind:broker={app.brokerRepository[app.selectedBroker]} />
							</TabContent>
							<TabContent>
								<Pipeline
									bind:pipelines={app.pipelines}
									bind:broker={app.brokerRepository[app.selectedBroker]}
									bind:socket
								/>
							</TabContent>
						</svelte:fragment>
					</Tabs>
				</div>
				<div style="flex: 1">
					{#if app.brokerRepository[app.selectedBroker].selectedTopic?.messages.length}
						<Messages
							bind:selectedTopic={app.brokerRepository[app.selectedBroker].selectedTopic} />
					{/if}
				</div>
			</div>
		</Grid>
		<PublishMessage
			bind:savedCommands={app.commands}
			bind:selectedBroker={app.selectedBroker}
			bind:socket
			bind:broker={app.brokerRepository[app.selectedBroker]}
		/>
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

	:global(.bx--side-nav__submenu-chevron) {
		transform: scaleY(-1) !important;
	}
</style>
