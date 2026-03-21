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
	import { onMount, onDestroy } from 'svelte';
	import TopicTree from '../components/topic_tree.svelte';
	import {
		Button,
		Content,
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
	import AddBroker from '../components/dialogs/add_broker.svelte';
	import {
		Add,
		CheckmarkFilled,
		CircleDash,
		CircleSolid,
		Connect,
		InformationFilled,
		LogoGithub,
		Renew,
		TrashCan
	} from 'carbon-icons-svelte';
	import PublishMessage from '../components/publish_message.svelte';
	import RateHistoryChart from '../components/rate_history_chart.svelte';
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
		processPipelines,
		processRateHistorySample,
		processSettings,
		processSyncComplete
	} from '$lib/ws_msg_handling';
	import RemoveBroker from '../components/dialogs/remove_broker.svelte';
	import { selectedTheme, availableThemes } from '../store';
	import { goto } from '$app/navigation';
	import { findbranchwithid } from '$lib/helper';

	let socket: WebSocket;
	let app = new AppState();

	const decoder = new TextDecoder('utf-8');

	// Save initial URL params before reactive statements can clear them
	const initialParams = new URLSearchParams(window.location.search);
	let pendingTopic: string | null = initialParams.get('topic');

	let socketConnected = false;
	function initializeWebSocket() {
		if (socket && socket.readyState !== WebSocket.CLOSED) {
			socket.close();
		}
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
				case 'mqtt_brokers': {
					console.log('mqtt_brokers received, rate_history:', json.params?.map?.((p: {broker: string, rate_history?: unknown[]}) => ({ broker: p.broker, rate_history_len: p.rate_history?.length ?? 0 })));
					app.brokerRepository = processBrokers(json.params, decoder, app.brokerRepository);
					const params = new URLSearchParams(window.location.search);
					const broker = params.get('broker');
					if (broker && app.brokerRepository[broker]) {
						app.selectedBroker = broker;
					} else if (!app.selectedBroker) {
						const brokers = Object.keys(app.brokerRepository);
						if (brokers.length > 0) {
							app.selectedBroker = brokers[0];
						}
					}
					const tab = params.get('tab');
					if (tab && !isNaN(Number(tab))) {
						selectedTab = Number(tab);
					}
					break;
				}
				case 'mqtt_message':
					app = processMQTTMessage(json.params, decoder, app);
					break;
				case 'rate_history_sample':
					console.log('rate_history_sample received:', json.params.source, 'history len:', json.params.sample);
					processRateHistorySample(json.params, app);
					// Explicit brokerRepository reassignment to ensure Svelte reactivity
					app.brokerRepository = app.brokerRepository;
					break;
				case 'commands':
					app.commands = processConfigs(json.params);
					break;
				case 'pipelines':
					app.pipelines = processPipelines(json.params);
					break;
				case 'settings':
					app = processSettings(json.params, app);
					break;
				case 'sync_complete':
					app = processSyncComplete(app);
					if (pendingTopic && app.selectedBroker && app.brokerRepository[app.selectedBroker]) {
						const found = findbranchwithid(
							pendingTopic,
							app.brokerRepository[app.selectedBroker].topics
						);
						if (found) {
							app.brokerRepository[app.selectedBroker].selectedTopic = found;
						}
						pendingTopic = null;
					}
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

	let isSideNavOpen = false;
	let addMqttBrokerModalOpen = false;
	let removeMqttBrokerModalOpen = false;

	function formatBytes(bytes: number): string {
		if (bytes < 1024) return bytes + ' B';
		if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
		return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
	}

	function formatRate(bytesPerSecond: number): string {
		if (bytesPerSecond < 1024) return bytesPerSecond.toFixed(0) + ' B/s';
		if (bytesPerSecond < 1024 * 1024) return (bytesPerSecond / 1024).toFixed(1) + ' KB/s';
		return (bytesPerSecond / (1024 * 1024)).toFixed(1) + ' MB/s';
	}

	function formatDuration(ms: number): string {
		const seconds = Math.floor(ms / 1000);
		if (seconds < 60) return `${seconds}s`;
		const minutes = Math.floor(seconds / 60);
		if (minutes < 60) return `${minutes}m`;
		const hours = Math.floor(minutes / 60);
		if (hours < 24) return `${hours}h ${minutes % 60}m`;
		const days = Math.floor(hours / 24);
		return `${days}d ${hours % 24}h`;
	}

	function getHistoryReachMs(entry: import('$lib/state').BrokerRepositoryEntry): number | null {
		const history = entry.rateHistory;
		if (history.length === 0) return null;
		return Date.now() - history[0].timestamp;
	}

	let theme: CarbonTheme;
	$: theme = $selectedTheme.id as CarbonTheme;

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

	let selectedTab = 1;
	$: {
		const params = new URLSearchParams($page.url.search);
		if (app.selectedBroker) {
			params.set('broker', app.selectedBroker);
		}
		if (selectedTab !== 0) {
			params.set('tab', selectedTab.toString());
		}
		const currentTopic =
			app.selectedBroker && app.brokerRepository[app.selectedBroker]?.selectedTopic?.id;
		if (currentTopic) {
			params.set('topic', currentTopic);
		} else if (pendingTopic) {
			params.set('topic', pendingTopic);
		} else {
			params.delete('topic');
		}
		const url = `${$page.url.pathname}?${params.toString()}`;
		const current = `${$page.url.pathname}${$page.url.search}`;
		if (url !== current) {
			goto(url);
		}
	}

	onMount(initializeWebSocket);
	onDestroy(() => {
		if (socket && socket.readyState !== WebSocket.CLOSED) {
			socket.close();
		}
	});
</script>

<Theme bind:theme />

<AddBroker bind:socket bind:open={addMqttBrokerModalOpen} />
<RemoveBroker bind:app bind:socket bind:open={removeMqttBrokerModalOpen} />

<Header platformName="MQTT-Inspector" bind:isSideNavOpen persistentHamburgerMenu={true}>
	<svelte:fragment slot="skip-to-content">
		<SkipToContent />
	</svelte:fragment>

	<Button
		kind={selectedTab === 1 ? 'primary' : 'secondary'}
		on:click={() => {
			selectedTab = 1;
		}}>Treeview</Button
	>

	<Button
		kind={selectedTab === 2 ? 'primary' : 'secondary'}
		on:click={() => {
			selectedTab = 2;
		}}>Pipeline</Button
	>
	<Button
		kind={selectedTab === 3 ? 'primary' : 'secondary'}
		on:click={() => {
			selectedTab = 3;
		}}>Publish</Button
	>
	<Button
		kind={selectedTab === 4 ? 'primary' : 'secondary'}
		on:click={() => {
			selectedTab = 4;
		}}>Throughput</Button
	>

	<div style="flex: 1" />

	{#if app.brokerRepository[app.selectedBroker]}
		{@const entry = app.brokerRepository[app.selectedBroker]}
		<div
			style="font-size: 0.75rem; opacity: 0.7; padding: 0 1em; white-space: nowrap; display: flex; align-items: center; gap: 0.4em;"
		>
			{#if app.syncComplete}
				{formatBytes(entry.totalBytes)}
				{#if entry.backendTotalBytes >= app.maxBrokerBytes}
					<InformationFilled
						size={16}
						title="Storage at maximum ({formatBytes(
							app.maxBrokerBytes
						)}) — oldest messages are being evicted"
					/>
				{:else}
					<CheckmarkFilled
						size={16}
						title="Synced"
						style="color: var(--cds-support-success, #24a148)"
					/>
				{/if}
			{:else}
				{formatBytes(entry.totalBytes)} | {formatBytes(entry.backendTotalBytes)}
				<Renew size={16} title="Syncing..." class="spin-icon" />
			{/if}
			<span style="opacity: 0.8;">| {formatRate(entry.bytesPerSecond || 0)}</span>
			<span style="opacity: 0.8;">| History: {formatDuration(getHistoryReachMs(entry) || 0)}</span>
		</div>
	{/if}

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
				icon={app.brokerRepository[broker].connected && socketConnected ? CircleSolid : CircleDash}
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

<Content style="padding: 1em">
	{#if app.brokerRepository[app.selectedBroker]}
		{#if selectedTab === 1}
			<div class="treeview-flex">
				<div class="treeview-col" style="max-width: 40em; margin-top: -0.27em">
					<TopicTree bind:broker={app.brokerRepository[app.selectedBroker]} />
				</div>
				<div class="treeview-col">
					{#if app.brokerRepository[app.selectedBroker].selectedTopic?.messages.length}
						<Messages bind:selectedTopic={app.brokerRepository[app.selectedBroker].selectedTopic} />
					{/if}
				</div>
			</div>
		{:else if selectedTab === 2}
			<div class="treeview-flex">
				<div class="treeview-col" style="max-width: 40em">
					<Pipeline
						bind:pipelines={app.pipelines}
						bind:broker={app.brokerRepository[app.selectedBroker]}
						bind:socket
					/>
				</div>
				<div class="treeview-col">
					{#if app.brokerRepository[app.selectedBroker].selectedTopic?.messages.length}
						<Messages bind:selectedTopic={app.brokerRepository[app.selectedBroker].selectedTopic} />
					{/if}
				</div>
			</div>
		{:else if selectedTab === 3}
			<PublishMessage
				bind:savedCommands={app.commands}
				bind:selectedBroker={app.selectedBroker}
				bind:socket
				bind:broker={app.brokerRepository[app.selectedBroker]}
			/>
		{:else if selectedTab === 4}
			<RateHistoryChart
				rateHistory={app.brokerRepository[app.selectedBroker].rateHistory}
				brokerName={app.selectedBroker}
				maxBrokerBytes={app.maxBrokerBytes}
			/>
		{/if}
	{/if}
</Content>

<style>
	.treeview-flex {
		display: flex;
		flex-direction: row;
		width: 100%;
		gap: 1rem;
		box-sizing: border-box;
		min-height: 0;
	}

	.treeview-col {
		flex: 1 1 50%;
		min-width: 0;
		overflow: hidden;
		box-sizing: border-box;
		max-height: calc(100vh - 4.8rem);
		min-height: 0;
	}

	@media (max-width: 75em) {
		.treeview-flex {
			flex-direction: column;
		}
		.treeview-col {
			min-width: 100%;
		}
	}

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

	:global(.spin-icon) {
		animation: spin 1.5s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
</style>
