<script lang="ts">
	import { onDestroy } from 'svelte';
	import { AreaChart } from '@carbon/charts-svelte';
	import { ScaleTypes } from '@carbon/charts';
	import type { RateHistoryEntry } from '$lib/state';
	import '@carbon/charts-svelte/styles.css';

	export let rateHistory: RateHistoryEntry[];
	export let brokerName: string;
	export let maxBrokerBytes: number;

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

	// Memoize all chart data — only recompute when rateHistory actually
	// gains new entries, NOT on every parent re-render from mqtt_message.
	let prevLen = -1;
	let rateChartData: { group: string; date: Date; value: number }[] = [];
	let storedChartData: { group: string; date: Date; value: number }[] = [];
	let rateOptions: Record<string, unknown> = {};
	let storedOptions: Record<string, unknown> = {};
	let showCharts = false;

	$: {
		const len = rateHistory.length;
		if (len !== prevLen) {
			prevLen = len;
			showCharts = len > 0;

			const maxRate = rateHistory.reduce((max, e) => Math.max(max, e.bytesPerSecond), 0);
			const rateUnit = getUnit(maxRate);

			const maxStoredVal = Math.max(
				maxBrokerBytes,
				rateHistory.reduce((max, e) => Math.max(max, e.totalBytes), 0)
			);
			const storedUnit = getUnit(maxStoredVal);

			const maxReachedEntry = rateHistory.find((e) => e.totalBytes >= maxBrokerBytes);
			const maxReachedDate = maxReachedEntry ? new Date(maxReachedEntry.timestamp) : null;

			rateChartData = rateHistory.map((entry) => ({
				group: brokerName,
				date: new Date(entry.timestamp),
				value: Math.round((entry.bytesPerSecond / rateUnit.divisor) * 100) / 100
			}));

			storedChartData = rateHistory.map((entry) => ({
				group: 'Stored',
				date: new Date(entry.timestamp),
				value: Math.round((entry.totalBytes / storedUnit.divisor) * 100) / 100
			}));

			rateOptions = {
				title: `Throughput (${rateUnit.label}/s)`,
				axes: {
					bottom: { mapsTo: 'date', scaleType: ScaleTypes.TIME },
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
						...(maxReachedDate
							? {
									thresholds: [
										{
											value: maxReachedDate,
											label: 'Max storage reached',
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
		<p style="opacity: 0.5; font-size: 0.8rem; text-align: right; margin: 0;">
			Next sample in {countdown}s
		</p>
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
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 400px;
		opacity: 0.7;
	}
</style>
