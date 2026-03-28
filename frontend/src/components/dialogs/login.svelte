<!-- Copyright (c) 2024-2026 Kai Lawrence -->
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
	import { Modal, PasswordInput, InlineNotification } from 'carbon-components-svelte';
	import { requestBrokerAuthentication } from '$lib/socket';

	export let open = false;
	export let broker = '';
	export let socket: WebSocket;

	let password = '';
	let error = '';

	function submit() {
		if (!password || !broker || !socket || socket.readyState !== WebSocket.OPEN) return;
		error = '';
		requestBrokerAuthentication(broker, password, socket);
		password = '';
	}

	export function showError(message: string) {
		error = message;
	}

	// Reset error when dialog opens with a new broker
	$: if (open) error = '';
</script>

<Modal
	bind:open
	modalHeading="Authenticate for {broker}"
	primaryButtonText="Authenticate"
	primaryButtonDisabled={!password}
	passiveModal={false}
	on:submit={submit}
	on:close={() => {
		password = '';
		error = '';
	}}
>
	<p style="margin-bottom: 1rem;">This broker requires a password to view its messages.</p>
	{#if error}
		<InlineNotification kind="error" title="Error" subtitle={error} hideCloseButton lowContrast />
	{/if}
	<PasswordInput labelText="Password" placeholder="Enter broker password" bind:value={password} />
</Modal>
