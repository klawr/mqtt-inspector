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
	import AddBroker from '../components/add_broker.svelte';
	import { Add, CircleDash, CircleSolid, Connect, LogoGithub } from 'carbon-icons-svelte';
	import PublishMessage from '../components/publish_message.svelte';
	import type { CarbonTheme } from 'carbon-components-svelte/src/Theme/Theme.svelte';
	import { page } from '$app/stores';
	import Pipeline from '../components/pipeline.svelte';
	import { AppState, type Treebranch } from '$lib/state';
	import {
		processBrokers,
		processConfigs,
		processConnectionStatus,
		processMQTTMessage,
		processPipelines
	} from '$lib/ws_msg_handling';

	let socket: WebSocket;
	let app = new AppState();

	const decoder = new TextDecoder('utf-8');

	let socketConnected = false;
	function initializeWebSocket() {
		socket = new WebSocket(`ws://${$page.url.host}/ws`);

		socket.onopen = (event) => {
			socketConnected = true;
			console.log('WebSocket connection opened:', event);
		};

		socket.onmessage = (event) => {
			const message = event.data;
			const json = JSON.parse(message);
			switch (json.method) {
				case 'mqtt_connection_status':
					app = processConnectionStatus(json.params, app);
				case 'mqtt_brokers':
					app.brokerRepository = processBrokers(json.params, app.brokerRepository);
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

		socket.onclose = (event) => {
			socketConnected = false;
			console.log('WebSocket connection closed:', event);
		};

		socket.onerror = (event) => {
			console.error('WebSocket error:', event);
		};
	}

	onMount(initializeWebSocket);

	let isSideNavOpen = false;
	let selectedTopic: Treebranch | null = null;
	let addMqttBrokerModalOpen = false;
	let theme: CarbonTheme = 'g90';
</script>

<Theme bind:theme />

<AddBroker bind:socket bind:open={addMqttBrokerModalOpen} />

<Header platformName="MQTT-Inspector" bind:isSideNavOpen>
	<svelte:fragment slot="skip-to-content">
		<SkipToContent />
	</svelte:fragment>
	<div style="flex: 1" />

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
				icon={app.brokerRepository[broker].connected ? CircleSolid : CircleDash}
				text={broker}
				isSelected={app.selectedBroker === broker}
				on:click={() => {
					selectedTopic = app.brokerRepository[broker].selectedTopic;
					app.selectedBroker = broker;
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

<Content style="padding: 0">
	{#if app.brokerRepository[app.selectedBroker]}
		<Grid fullWidth>
			<div style="height: calc(100vh - 8em) !important; display: flex; flex-direction: column">
				<div style="display: flex">
					<div style="flex: 1; margin: 1em; min-width: 30em; max-width: 50em">
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
					<div style="flex: 1; margin: 1em; min-width: 30em">
						<Messages bind:broker={app.brokerRepository[app.selectedBroker]} />
					</div>
				</div>
				<div style="flex: 1;" />
				<PublishMessage
					bind:savedCommands={app.commands}
					bind:selectedBroker={app.selectedBroker}
					bind:socket
					bind:broker={app.brokerRepository[app.selectedBroker]}
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
