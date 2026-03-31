<script lang="ts">
	import { onDestroy } from 'svelte';
	import { AreaChart } from '@carbon/charts-svelte';
	import { ScaleTypes } from '@carbon/charts';
	import type { RateHistoryEntry } from '$lib/state';
	import '@carbon/charts-svelte/styles.css';

	export let rateHistory: RateHistoryEntry[];
	export let brokerName: string;
	export let maxBrokerBytes: number;

	const timeRanges = [
		{ id: 'history', label: 'Since history', ms: null },
		{ id: 'all', label: 'All samples', ms: null },
		{ id: '15m', label: 'Last 15 min', ms: 15 * 60 * 1000 },
		{ id: '1h', label: 'Last 1 hour', ms: 60 * 60 * 1000 },
		{ id: '6h', label: 'Last 6 hours', ms: 6 * 60 * 60 * 1000 },
		{ id: '24h', label: 'Last 24 hours', ms: 24 * 60 * 60 * 1000 },
		{ id: '7d', label: 'Last 7 days', ms: 7 * 24 * 60 * 60 * 1000 }
	];
	let selectedRangeId = 'history';
	const MAX_CHART_POINTS = 1500;

	// Countdown timer: seconds until next sample
	let countdown = 10;
	const timer = setInterval(() => {
		if (rateHistory.length === 0) {
			countdown = 10;
			return;
		}
		const lastTs = rateHistory[rateHistory.length - 1].timestamp;
		const elapsed = (Date.now() - lastTs) / 1000;
		countdown = Math.max(0, Math.ceil(10 - elapsed));
	}, 1000);
	onDestroy(() => clearInterval(timer));

	// Determine appropriate unit and divisor based on max value
	function getUnit(maxVal: number): { label: string; divisor: number } {
		if (maxVal < 1024) return { label: 'B', divisor: 1 };
		if (maxVal < 1024 * 1024) return { label: 'KB', divisor: 1024 };
		return { label: 'MB', divisor: 1024 * 1024 };
	}

	function downsampleHistory(entries: RateHistoryEntry[]): RateHistoryEntry[] {
		if (entries.length <= MAX_CHART_POINTS) return entries;
		const result: RateHistoryEntry[] = [];
		const step = (entries.length - 1) / (MAX_CHART_POINTS - 1);
		for (let i = 0; i < MAX_CHART_POINTS; i++) {
			result.push(entries[Math.round(i * step)]);
		}
		return result;
	}

	// Memoize all chart data — only recompute when rateHistory actually
	// gains new entries, NOT on every parent re-render from mqtt_message.
	let previousChartKey = '';
	let rateChartData: { group: string; date: Date; value: number }[] = [];
	let storedChartData: { group: string; date: Date; value: number }[] = [];
	let rateOptions: Record<string, unknown> = {};
	let storedOptions: Record<string, unknown> = {};
	let showCharts = false;
	$: selectedRange = timeRanges.find((range) => range.id === selectedRangeId) ?? timeRanges[0];

	// Compute the oldest timestamp within the broker's retained history by
	// accumulating bytesPerSecond * dt backwards until we exceed maxBrokerBytes.
	$: historyStartTimestamp = (() => {
		if (rateHistory.length <= 1) return null;
		let cumulated = 0;
		for (let i = rateHistory.length - 1; i > 0; i--) {
			const dt = (rateHistory[i].timestamp - rateHistory[i - 1].timestamp) / 1000;
			cumulated += rateHistory[i].bytesPerSecond * dt;
			if (cumulated >= maxBrokerBytes) {
				return rateHistory[i - 1].timestamp;
			}
		}
		return null; // all data fits within broker capacity
	})();

	$: filteredRateHistory = (() => {
		if (selectedRangeId === 'history') {
			return historyStartTimestamp !== null
				? rateHistory.filter((entry) => entry.timestamp >= historyStartTimestamp!)
				: rateHistory;
		}
		if (selectedRange.ms === null) return rateHistory;
		return rateHistory.filter((entry) => entry.timestamp >= Date.now() - selectedRange.ms!);
	})();

	$: chartRateHistory = downsampleHistory(filteredRateHistory);

	$: {
		const chartKey = `${selectedRangeId}:${chartRateHistory.length}:${chartRateHistory.at(0)?.timestamp ?? 0}:${chartRateHistory.at(-1)?.timestamp ?? 0}`;
		if (chartKey !== previousChartKey) {
			previousChartKey = chartKey;
			showCharts = chartRateHistory.length > 0;

			const maxRate = chartRateHistory.reduce((max, e) => Math.max(max, e.bytesPerSecond), 0);
			const rateUnit = getUnit(maxRate);

			const maxStoredVal = Math.max(
				maxBrokerBytes,
				chartRateHistory.reduce((max, e) => Math.max(max, e.totalBytes), 0)
			);
			const storedUnit = getUnit(maxStoredVal);

			// Calculate indicator using bytesPerSecond and timestamps if totalBytes is capped
			let oldestAvailableDate: Date | null = null;
			if (chartRateHistory.length > 1) {
				let cumulated = 0;
				let thresholdIndex = -1;
				for (let i = chartRateHistory.length - 1; i > 0; i--) {
					const dt = (chartRateHistory[i].timestamp - chartRateHistory[i - 1].timestamp) / 1000;
					const bytes = chartRateHistory[i].bytesPerSecond * dt;
					cumulated += bytes;
					if (cumulated >= maxBrokerBytes) {
						thresholdIndex = i - 1;
						break;
					}
				}
				if (thresholdIndex !== -1) {
					oldestAvailableDate = new Date(chartRateHistory[thresholdIndex].timestamp);
				}
			}

			rateChartData = chartRateHistory.map((entry) => ({
				group: brokerName,
				date: new Date(entry.timestamp),
				value: Math.round((entry.bytesPerSecond / rateUnit.divisor) * 100) / 100
			}));

			storedChartData = chartRateHistory.map((entry) => ({
				group: 'Stored',
				date: new Date(entry.timestamp),
				value: Math.round((entry.totalBytes / storedUnit.divisor) * 100) / 100
			}));

			rateOptions = {
				title: `Throughput (${rateUnit.label}/s)`,
				axes: {
					bottom: {
						mapsTo: 'date',
						scaleType: ScaleTypes.TIME,
						...(oldestAvailableDate
							? {
									thresholds: [
										{
											value: oldestAvailableDate,
											label: 'Oldest available message',
											fillColor: '#da1e28'
										}
									]
								}
							: {})
					},
					left: {
						mapsTo: 'value',
						title: `${rateUnit.label}/s`,
						scaleType: ScaleTypes.LINEAR
					}
				},
				curve: 'curveMonotoneX' as const,
				height: '350px',
				theme: 'g90' as const,
				color: { scale: { [brokerName]: '#0f62fe' } },
				points: { enabled: false },
				toolbar: { enabled: true },
				legend: { enabled: false }
			};

			storedOptions = {
				title: `Stored bytes (${storedUnit.label})`,
				axes: {
					bottom: {
						mapsTo: 'date',
						scaleType: ScaleTypes.TIME,
						...(oldestAvailableDate
							? {
									thresholds: [
										{
											value: oldestAvailableDate,
											label: 'Oldest available message',
											fillColor: '#da1e28'
										}
									]
								}
							: {})
					},
					left: {
						mapsTo: 'value',
						title: storedUnit.label,
						scaleType: ScaleTypes.LINEAR
					}
				},
				curve: 'curveMonotoneX' as const,
				height: '350px',
				theme: 'g90' as const,
				color: { scale: { Stored: '#6929c4' } },
				points: { enabled: false },
				toolbar: { enabled: true },
				legend: { enabled: false }
			};
		}
	}
</script>

{#if showCharts}
	<div class="charts-container">
		<div class="chart-toolbar">
			<label>
				<span>Timerange</span>
				<select bind:value={selectedRangeId}>
					{#each timeRanges as range}
						<option value={range.id}>{range.label}</option>
					{/each}
				</select>
			</label>
			<p>
				Showing {chartRateHistory.length} of {rateHistory.length}
				samples. Next sample in {countdown}s
			</p>
		</div>
		<AreaChart data={rateChartData} options={rateOptions} />
		<AreaChart data={storedChartData} options={storedOptions} />
	</div>
{:else}
	<div class="empty-state">
		<p>No throughput data recorded yet. Data will appear as messages are received.</p>
		<p style="opacity: 0.6; font-size: 0.85rem;">Next sample in {countdown}s</p>
	</div>
{/if}

<style>
	.charts-container {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.chart-toolbar {
		display: flex;
		justify-content: space-between;
		align-items: end;
		gap: 1rem;
		flex-wrap: wrap;
	}

	.chart-toolbar label {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		font-size: 0.85rem;
	}

	.chart-toolbar select {
		min-width: 12rem;
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--cds-border-subtle, #c6c6c6);
		background: var(--cds-field, #262626);
		color: inherit;
		font: inherit;
	}

	.chart-toolbar p {
		margin: 0;
		opacity: 0.65;
		font-size: 0.8rem;
	}

	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 400px;
		opacity: 0.7;
	}
</style>
