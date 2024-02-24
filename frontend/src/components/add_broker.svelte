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
		requestMqttBrokerConnection(ip || ip_default, port || port_default, socket);
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
