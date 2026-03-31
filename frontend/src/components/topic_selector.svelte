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
	import { createEventDispatcher } from 'svelte';
	import { TextInput } from 'carbon-components-svelte';
	import { getTopicSuggestions, type TopicSuggestion } from '$lib/helper';

	export let value = '';
	export let topicIds: string[] = [];
	export let labelText = 'Topic';
	export let placeholder = 'Search for a topic...';
	export let helperText = '';
	export let maxSuggestions = 100;
	export let allowCustomValue = true;
	export let emptyMessage = 'No matching topics';
	export let id = '';

	const dispatch = createEventDispatcher<{
		select: { value: string };
		submit: { value: string };
	}>();

	let rootElement: HTMLDivElement;
	let isOpen = false;
	let highlightedIndex = -1;

	function getNormalizedValue(): string {
		return typeof value === 'string' ? value : '';
	}

	$: suggestions = getTopicSuggestions(topicIds, getNormalizedValue(), maxSuggestions);
	$: if (!isOpen) {
		highlightedIndex = -1;
	} else if (suggestions.length === 0) {
		highlightedIndex = -1;
	} else if (highlightedIndex >= suggestions.length) {
		highlightedIndex = suggestions.length - 1;
	}

	function openSuggestions() {
		isOpen = true;
	}

	function closeSuggestions() {
		isOpen = false;
	}

	function chooseSuggestion(suggestion: TopicSuggestion) {
		value = suggestion.id;
		closeSuggestions();
		dispatch('select', { value });
	}

	function submitCurrentValue() {
		const trimmedValue = getNormalizedValue().trim();
		if (!trimmedValue) {
			closeSuggestions();
			return;
		}
		closeSuggestions();
		dispatch('submit', { value: trimmedValue });
	}

	function canSubmitValue(): boolean {
		const trimmedValue = getNormalizedValue().trim();
		return (
			!!trimmedValue &&
			(allowCustomValue || suggestions.some((suggestion) => suggestion.id === trimmedValue))
		);
	}

	function handleInput() {
		openSuggestions();
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			openSuggestions();
			if (suggestions.length > 0) {
				highlightedIndex = Math.min(highlightedIndex + 1, suggestions.length - 1);
			}
			return;
		}

		if (event.key === 'ArrowUp') {
			event.preventDefault();
			if (suggestions.length > 0) {
				highlightedIndex = Math.max(highlightedIndex - 1, 0);
			}
			return;
		}

		if (event.key === 'Escape') {
			closeSuggestions();
			return;
		}

		if (event.key === 'Enter') {
			event.preventDefault();
			if (highlightedIndex >= 0 && highlightedIndex < suggestions.length) {
				chooseSuggestion(suggestions[highlightedIndex]);
				return;
			}
			if (canSubmitValue()) {
				submitCurrentValue();
			}
		}
	}

	function handleWindowMouseDown(event: MouseEvent) {
		if (!rootElement?.contains(event.target as Node)) {
			closeSuggestions();
		}
	}
</script>

<svelte:window on:mousedown={handleWindowMouseDown} />

<div class="topic-selector" bind:this={rootElement}>
	<TextInput
		{id}
		bind:value
		{labelText}
		{placeholder}
		on:focus={openSuggestions}
		on:input={handleInput}
		on:keydown={handleKeydown}
	/>
	{#if helperText}
		<p class="topic-selector__helper">{helperText}</p>
	{/if}
	{#if isOpen}
		<div class="topic-selector__panel">
			{#if suggestions.length > 0}
				<div class="topic-selector__status">
					Showing {suggestions.length} of {topicIds.length} topics
				</div>
				<div class="topic-selector__list" role="listbox" aria-label={labelText}>
					{#each suggestions as suggestion, index (suggestion.id)}
						<button
							type="button"
							class:selected={index === highlightedIndex}
							class="topic-selector__item"
							on:mousedown|preventDefault={() => chooseSuggestion(suggestion)}
						>
							{suggestion.text}
						</button>
					{/each}
				</div>
			{:else if getNormalizedValue().trim()}
				<div class="topic-selector__status">
					{emptyMessage}
					{#if allowCustomValue}
						. Press Enter to use "{getNormalizedValue().trim()}"
					{/if}
				</div>
			{:else}
				<div class="topic-selector__status">No topics available yet</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.topic-selector {
		position: relative;
	}

	.topic-selector__helper {
		margin: 0.25rem 0 0;
		font-size: 0.75rem;
		color: var(--cds-text-secondary, #6f6f6f);
	}

	.topic-selector__panel {
		position: absolute;
		top: calc(100% + 0.25rem);
		left: 0;
		right: 0;
		z-index: 10;
		border: 1px solid var(--cds-border-subtle, #c6c6c6);
		background: var(--cds-layer, #262626);
		box-shadow: 0 0.5rem 1rem rgb(0 0 0 / 0.2);
	}

	.topic-selector__status {
		padding: 0.5rem 0.75rem;
		font-size: 0.75rem;
		color: var(--cds-text-secondary, #c6c6c6);
	}

	.topic-selector__list {
		max-height: 18rem;
		overflow: auto;
	}

	.topic-selector__item {
		display: block;
		width: 100%;
		padding: 0.625rem 0.75rem;
		border: 0;
		background: transparent;
		color: inherit;
		text-align: left;
		cursor: pointer;
		font: inherit;
	}

	.topic-selector__item:hover,
	.topic-selector__item.selected {
		background: var(--cds-layer-hover, rgb(255 255 255 / 0.08));
	}
</style>
