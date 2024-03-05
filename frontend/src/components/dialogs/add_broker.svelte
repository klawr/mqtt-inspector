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
	import { requestMqttBrokerConnection } from '$lib/socket';
	import { Modal, TextInput } from 'carbon-components-svelte';

	export let open = false;
	export let socket: WebSocket;

	let ip: string | undefined = undefined;
	const ip_default = '127.0.0.1';
	let port: string | undefined = undefined;
	const port_default = '1883';

	function submit() {
		const hostname = `${ip?.trim() || ip_default}:${port?.trim() || port_default}`;
		requestMqttBrokerConnection(hostname, socket);
		open = false;
	}
</script>

<Modal
	bind:open
	modalHeading="Add MQTT Broker"
	primaryButtonText="Confirm"
	secondaryButtonText="Cancel"
	on:click:button--secondary={() => (open = false)}
	on:open
	on:close
	on:submit={submit}
>
	<TextInput labelText="Ip" placeholder={ip_default} bind:value={ip} />
	<TextInput labelText="Port" placeholder={port_default} bind:value={port} />
</Modal>
