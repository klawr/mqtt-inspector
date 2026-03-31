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
		CircleDash,
		CircleSolid,
		Connect,
		InformationFilled,
		Locked,
		LogoGithub,
		TrashCan,
		Unlocked
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
		processMQTTMessages,
		processMQTTMessageMetaBatch,
		processMessagesEvictedBatch,
		processTopicSummaries,
		processTopicMessagesClear,
		processPipelines,
		parseMqttWebSocketMessage,
		processRateHistorySample,
		processSettings
	} from '$lib/ws_msg_handling';
	import RemoveBroker from '../components/dialogs/remove_broker.svelte';
	import Login from '../components/dialogs/login.svelte';
	import { selectedTheme, availableThemes } from '../store';
	import { goto } from '$app/navigation';
	import AppNavIcon from '../components/app_nav_icon.svelte';
	import { findbranchwithid } from '$lib/helper';
	import { requestTopicSelection } from '$lib/socket';

	let socket: WebSocket;
	let app = new AppState();
	let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	let mqttFlushTimer: ReturnType<typeof setTimeout> | null = null;
	let reconnectAttempts = 0;
	let shouldReconnect = true;
	let pendingMqttMessages: import('$lib/ws_msg_handling').MQTTMessageParam[] = [];
	let topicSyncing = false;
	let syncTimeoutTimer: ReturnType<typeof setTimeout> | null = null;

	const MQTT_BATCH_FLUSH_MS = 16;
	const SYNC_TIMEOUT_MS = 5000;

	// Save initial URL params before reactive statements can clear them
	const initialParams = new URLSearchParams(window.location.search);
	let pendingTopic: string | null = initialParams.get('topic');

	let socketConnected = false;
	let loginOpen = false;
	let loginBroker = '';
	let loginRef: Login | undefined;

	function scheduleReconnect() {
		if (!shouldReconnect || reconnectTimer) {
			return;
		}
		const delay = Math.min(1000 * 2 ** reconnectAttempts, 10000);
		reconnectTimer = setTimeout(() => {
			reconnectTimer = null;
			initializeWebSocket();
		}, delay);
		reconnectAttempts += 1;
	}

	function flushPendingMqttMessages() {
		if (mqttFlushTimer) {
			clearTimeout(mqttFlushTimer);
			mqttFlushTimer = null;
		}
		if (pendingMqttMessages.length === 0) {
			return;
		}
		const batch = pendingMqttMessages;
		pendingMqttMessages = [];
		app = processMQTTMessages(batch, app);
	}

	function scheduleMqttFlush() {
		if (topicSyncing || mqttFlushTimer) {
			return;
		}
		mqttFlushTimer = setTimeout(() => {
			mqttFlushTimer = null;
			flushPendingMqttMessages();
		}, MQTT_BATCH_FLUSH_MS);
	}

	function initializeWebSocket() {
		if (reconnectTimer) {
			clearTimeout(reconnectTimer);
			reconnectTimer = null;
		}
		flushPendingMqttMessages();
		if (socket && socket.readyState !== WebSocket.CLOSED) {
			socket.onclose = null;
			socket.close();
		}
		pendingMqttMessages = [];
		app = new AppState();
		socketConnected = false;
		const wsProto = $page.url.protocol === 'https:' ? 'wss:' : 'ws:';
		const wsUrl = `${wsProto}//${$page.url.host}/ws`;
		const currentSocket = new WebSocket(wsUrl);
		currentSocket.binaryType = 'arraybuffer';
		socket = currentSocket;

		currentSocket.onopen = () => {
			if (socket !== currentSocket) {
				return;
			}
			socketConnected = true;
			reconnectAttempts = 0;
		};

		currentSocket.onmessage = (event) => {
			if (socket !== currentSocket) {
				return;
			}
			const message = event.data;
			const json =
				typeof message === 'string' ? JSON.parse(message) : parseMqttWebSocketMessage(message);
			if (!json) {
				return;
			}
			if (
				json.method !== 'mqtt_message' &&
				json.method !== 'mqtt_message_meta_batch' &&
				json.method !== 'messages_evicted_batch'
			) {
				flushPendingMqttMessages();
			}
			switch (json.method) {
				case 'broker_removal':
					app = processBrokerRemoval(json.params, app);
					break;
				case 'mqtt_connection_status':
					app = processConnectionStatus(json.params, app);
					break;
				case 'mqtt_brokers': {
					app.brokerRepository = processBrokers(json.params, app.brokerRepository);
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
				case 'topic_summaries':
					app.brokerRepository = processTopicSummaries(json.params, app.brokerRepository);
					if (pendingTopic && app.selectedBroker && app.brokerRepository[app.selectedBroker]) {
						const found = findbranchwithid(
							pendingTopic,
							app.brokerRepository[app.selectedBroker].topics
						);
						if (found) {
							app.brokerRepository[app.selectedBroker].selectedTopic = found;
							requestTopicSelection(app.selectedBroker, found.id, socket);
						}
						pendingTopic = null;
					}
					break;
				case 'mqtt_message_meta_batch':
					app = processMQTTMessageMetaBatch(json.params, app);
					break;
				case 'mqtt_message':
					pendingMqttMessages.push(json.params);
					scheduleMqttFlush();
					break;
				case 'messages_evicted_batch':
					app = processMessagesEvictedBatch(json.params, app);
					break;
				case 'topic_messages_clear':
					flushPendingMqttMessages();
					app = processTopicMessagesClear(app);
					topicSyncing = true;
					if (syncTimeoutTimer) clearTimeout(syncTimeoutTimer);
					syncTimeoutTimer = setTimeout(() => {
						if (topicSyncing) {
							topicSyncing = false;
							flushPendingMqttMessages();
							app.brokerRepository = app.brokerRepository;
						}
						syncTimeoutTimer = null;
					}, SYNC_TIMEOUT_MS);
					break;
				case 'topic_sync_complete':
					topicSyncing = false;
					if (syncTimeoutTimer) {
						clearTimeout(syncTimeoutTimer);
						syncTimeoutTimer = null;
					}
					flushPendingMqttMessages();
					app.brokerRepository = app.brokerRepository;
					break;
				case 'rate_history_sample':
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
				case 'broker_auth_result': {
					const { broker, success } = json.params as { broker: string; success: boolean };
					if (success) {
						if (app.brokerRepository[broker]) {
							app.brokerRepository[broker].authenticated = true;
						}
						loginOpen = false;
						// Re-send topic selection now that peer is authenticated
						if (socket && socket.readyState === WebSocket.OPEN) {
							const entry = app.brokerRepository[broker];
							requestTopicSelection(broker, entry?.selectedTopic?.id ?? null, socket);
						}
					} else {
						loginRef?.showError('Invalid password');
					}
					break;
				}
				default:
					break;
			}
		};

		currentSocket.onclose = () => {
			if (socket !== currentSocket) {
				return;
			}
			socketConnected = false;
			scheduleReconnect();
		};

		currentSocket.onerror = () => {
			if (socket !== currentSocket) {
				return;
			}
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

	function formatMsgCount(count: number): string {
		if (count < 1000) return count + ' msg';
		if (count < 1_000_000) return (count / 1000).toFixed(1) + 'k msg';
		return (count / 1_000_000).toFixed(1) + 'M msg';
	}

	function formatMsgRate(rate: number): string {
		if (rate < 1000) return rate.toFixed(0) + ' msg/s';
		return (rate / 1000).toFixed(1) + 'k msg/s';
	}

	function formatDurationShort(ms: number): string {
		const seconds = Math.floor(ms / 1000);
		if (seconds < 60) return `${seconds}s`;
		const minutes = Math.floor(seconds / 60);
		if (minutes < 60) return `${minutes}m`;
		const hours = Math.floor(minutes / 60);
		if (hours < 24) return `${hours}h ${minutes % 60}m`;
		const days = Math.floor(hours / 24);
		return `${days}d ${hours % 24}h`;
	}

	type HistoryReachCacheEntry = {
		historyLength: number;
		lastTimestamp: number;
		maxBrokerBytes: number;
		reachMs: number;
	};
	const historyReachCache = new Map<string, HistoryReachCacheEntry>();

	function getHistoryReachMs(
		broker: string,
		entry: import('$lib/state').BrokerRepositoryEntry
	): number {
		const rateHistory = entry.rateHistory;
		if (rateHistory.length === 0) {
			return 0;
		}

		const historyLength = rateHistory.length;
		const lastTimestamp = rateHistory[historyLength - 1].timestamp;
		const cached = historyReachCache.get(broker);
		if (
			cached &&
			cached.historyLength === historyLength &&
			cached.lastTimestamp === lastTimestamp &&
			cached.maxBrokerBytes === app.maxBrokerBytes
		) {
			return cached.reachMs;
		}

		const maxBrokerBytes = app.maxBrokerBytes;
		let oldestAvailableDate: Date = new Date(rateHistory[0].timestamp);
		if (rateHistory.length > 1) {
			let cumulated = 0;
			let thresholdIndex = -1;
			for (let i = rateHistory.length - 1; i > 0; i--) {
				const dt = (rateHistory[i].timestamp - rateHistory[i - 1].timestamp) / 1000;
				const bytes = rateHistory[i].bytesPerSecond * dt;
				cumulated += bytes;
				if (cumulated >= maxBrokerBytes) {
					thresholdIndex = i - 1;
					break;
				}
			}
			if (thresholdIndex !== -1) {
				oldestAvailableDate = new Date(rateHistory[thresholdIndex].timestamp);
			}
		}

		const reachMs = Date.now() - oldestAvailableDate.getTime();
		historyReachCache.set(broker, {
			historyLength,
			lastTimestamp,
			maxBrokerBytes,
			reachMs
		});
		return reachMs;
	}

	let theme: CarbonTheme;
	$: theme = $selectedTheme.id as CarbonTheme;

	function themeChanged(e: Event): void {
		const value = (e.target as HTMLInputElement)?.value;
		if (!value) return;
		const newTheme = availableThemes.find((t) => t.id === value);
		if (newTheme) selectedTheme.set(newTheme);
	}

	let selectedTab = 1;
	let publishTopic = '';
	let publishPayload = '';
	let publishRetain = false;
	let publishCommandName = '';
	let publishSelectedCommandId = '';

	$: brokerNeedsAuth = app.selectedBroker
		? app.brokerRepository[app.selectedBroker]?.requiresAuth &&
			!app.brokerRepository[app.selectedBroker]?.authenticated
		: false;

	// Track topic selection and notify backend when it changes
	let lastSelectedBroker: string | null = null;
	let lastSelectedTopicId: string | null = null;
	let lastUrlSyncState = '';
	$: {
		const currentBroker = app.selectedBroker || null;
		const currentTopicId = currentBroker
			? app.brokerRepository[currentBroker]?.selectedTopic?.id ?? null
			: null;
		if (currentBroker !== lastSelectedBroker || currentTopicId !== lastSelectedTopicId) {
			lastSelectedBroker = currentBroker;
			lastSelectedTopicId = currentTopicId;
			if (socket && socket.readyState === WebSocket.OPEN) {
				const entry = currentBroker ? app.brokerRepository[currentBroker] : null;
				if (entry && entry.requiresAuth && !entry.authenticated) {
					loginBroker = currentBroker!;
					loginOpen = true;
				} else {
					requestTopicSelection(currentBroker, currentTopicId, socket);
				}
			}
		}
	}
	$: {
		const currentTopic =
			app.selectedBroker && app.brokerRepository[app.selectedBroker]?.selectedTopic?.id;
		const routeState = `${app.selectedBroker}|${selectedTab}|${currentTopic ?? pendingTopic ?? ''}`;
		if (routeState !== lastUrlSyncState) {
			lastUrlSyncState = routeState;

			const params = new URLSearchParams($page.url.search);
			if (app.selectedBroker) {
				params.set('broker', app.selectedBroker);
			}
			if (selectedTab !== 0) {
				params.set('tab', selectedTab.toString());
			}
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
	}

	onMount(initializeWebSocket);
	onDestroy(() => {
		shouldReconnect = false;
		flushPendingMqttMessages();
		if (reconnectTimer) {
			clearTimeout(reconnectTimer);
			reconnectTimer = null;
		}
		if (mqttFlushTimer) {
			clearTimeout(mqttFlushTimer);
			mqttFlushTimer = null;
		}
		if (socket && socket.readyState !== WebSocket.CLOSED) {
			socket.onclose = null;
			socket.close();
		}
	});
</script>

<Theme bind:theme />

<AddBroker bind:socket bind:open={addMqttBrokerModalOpen} />
<RemoveBroker bind:app bind:socket bind:open={removeMqttBrokerModalOpen} />
<Login bind:this={loginRef} bind:open={loginOpen} broker={loginBroker} {socket} />

<Header
	platformName="mqtt-inspector"
	bind:isSideNavOpen
	persistentHamburgerMenu={true}
	iconMenu={AppNavIcon}
	iconClose={AppNavIcon}
>
	<svelte:fragment slot="skip-to-content">
		<SkipToContent />
	</svelte:fragment>

	<Button
		kind={selectedTab === 1 ? 'primary' : 'secondary'}
		disabled={brokerNeedsAuth}
		on:click={() => {
			selectedTab = 1;
		}}>Treeview</Button
	>

	<Button
		kind={selectedTab === 2 ? 'primary' : 'secondary'}
		disabled={brokerNeedsAuth}
		on:click={() => {
			selectedTab = 2;
		}}>Pipeline</Button
	>
	<Button
		kind={selectedTab === 3 ? 'primary' : 'secondary'}
		disabled={brokerNeedsAuth}
		on:click={() => {
			selectedTab = 3;
		}}>Publish</Button
	>
	<Button
		kind={selectedTab === 4 ? 'primary' : 'secondary'}
		disabled={brokerNeedsAuth}
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
			{formatBytes(entry.backendTotalBytes)}
			{#if entry.backendTotalBytes >= app.maxBrokerBytes}
				<InformationFilled
					size={16}
					title="Storage at maximum ({formatBytes(
						app.maxBrokerBytes
					)}) — oldest messages are being evicted"
				/>
			{/if}
			<span>| {formatRate(entry.bytesPerSecond || 0)}</span>
			<span>| {formatMsgCount(entry.backendTotalMessages)}</span>
			<span>| {formatMsgRate(entry.messagesPerSecond || 0)}</span>
			<span
				>| History: {formatDurationShort(getHistoryReachMs(app.selectedBroker, entry) || 0)}</span
			>
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
			{@const entry = app.brokerRepository[broker]}
			<SideNavLink
				icon={entry.requiresAuth
					? entry.authenticated
						? Unlocked
						: Locked
					: entry.connected && socketConnected
						? CircleSolid
						: CircleDash}
				text={broker}
				isSelected={app.selectedBroker === broker}
				on:click={() => {
					if (app.selectedBroker === broker && entry.requiresAuth && !entry.authenticated) {
						loginBroker = broker;
						loginOpen = true;
					} else {
						app.selectedBroker = broker;
					}
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
					{#if app.brokerRepository[app.selectedBroker].selectedTopic?.messages.length || topicSyncing}
						<Messages
							bind:selectedTopic={app.brokerRepository[app.selectedBroker].selectedTopic}
							{topicSyncing}
						/>
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
					{#if app.brokerRepository[app.selectedBroker].selectedTopic?.messages.length || topicSyncing}
						<Messages
							bind:selectedTopic={app.brokerRepository[app.selectedBroker].selectedTopic}
							{topicSyncing}
						/>
					{/if}
				</div>
			</div>
		{:else if selectedTab === 3}
			<PublishMessage
				bind:savedCommands={app.commands}
				bind:selectedBroker={app.selectedBroker}
				bind:socket
				bind:broker={app.brokerRepository[app.selectedBroker]}
				bind:topic={publishTopic}
				bind:payload={publishPayload}
				bind:retain={publishRetain}
				bind:save_command_name={publishCommandName}
				bind:selectedCommandId={publishSelectedCommandId}
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
</style>
