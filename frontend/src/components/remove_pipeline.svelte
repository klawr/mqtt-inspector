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
	import { requestPipelineRemoval } from '$lib/socket';
	import type { SavedPipeline } from '$lib/state';
	import { Modal } from 'carbon-components-svelte';

	export let open = false;
	export let socket: WebSocket;
	export let selectedId: number | undefined;
	export let pipelines: SavedPipeline[];

	function getPipelineName(id: number | undefined) {
		if (id === undefined) {
			return;
		}
		return pipelines.find((e) => e.id === id)?.text || '';
	}

	function submit() {
		const pipelineName = getPipelineName(selectedId);
		if (!pipelineName) {
			open = false;
			return;
		}

		requestPipelineRemoval(pipelineName, socket);
		selectedId = undefined;
		open = false;
	}
</script>

<Modal
	bind:open
	danger
	modalHeading={`Remove pipeline ${getPipelineName(selectedId)}`}
	primaryButtonText="Confirm"
	secondaryButtonText="Cancel"
	on:click:button--secondary={() => (open = false)}
	on:open
	on:close
	on:submit={submit}
>
	<h5>Are you sure you want to remove the pipeline {getPipelineName(selectedId)}?</h5>
	<h5>This will remove it for all peers of this MQTT-Inspector instance.</h5>
</Modal>
