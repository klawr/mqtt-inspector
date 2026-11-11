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
	import { session } from '../../store';
	import type { MqttProtocol } from '$lib/state';
	import {
		Modal,
		TextInput,
		Toggle,
		PasswordInput,
		Select,
		SelectItem
	} from 'carbon-components-svelte';

	export let open = false;
	export let socket: WebSocket;

	let ip: string | undefined = undefined;
	const ip_default = '127.0.0.1';
	let port: string | undefined = undefined;
	const port_default = '1883';
	let use_tls = false;
	let username: string | undefined = undefined;
	let password: string | undefined = undefined;
	let ca_cert: string | undefined = undefined;
	let client_cert: string | undefined = undefined;
	let client_key: string | undefined = undefined;
	let client_key_password: string | undefined = undefined;
	let access_password: string | undefined = undefined;
	let allowed_roles: string | undefined = undefined;
	let protocol: MqttProtocol | 'auto' = 'auto';

	$: if (use_tls && (port === undefined || port === '1883')) {
		port = '8883';
	} else if (!use_tls && port === '8883') {
		port = '1883';
	}

	// Mirrors the backend's check; invalid addresses are never saved.
	$: portInvalid = port !== undefined && port.trim() !== '' && !isValidPort(port.trim());
	$: ipInvalid = ip !== undefined && ip.includes(':');

	function isValidPort(value: string): boolean {
		if (!/^\d+$/.test(value)) return false;
		const n = Number(value);
		return n >= 1 && n <= 65535;
	}

	function submit() {
		if (portInvalid || ipInvalid) return;
		const hostname = `${ip?.trim() || ip_default}:${port?.trim() || port_default}`;
		requestMqttBrokerConnection(hostname, socket, {
			use_tls,
			...(protocol !== 'auto' && { protocol }),
			username: username?.trim(),
			password,
			...($session.mode === 'claims'
				? {
						allowed_roles: (allowed_roles ?? '')
							.split(',')
							.map((r) => r.trim())
							.filter((r) => r)
					}
				: { access_password }),
			...(use_tls && {
				ca_cert: ca_cert?.trim(),
				client_cert: client_cert?.trim(),
				client_key: client_key?.trim(),
				client_key_password
			})
		});
		open = false;
	}
</script>

<Modal
	bind:open
	modalHeading="Add MQTT Broker"
	primaryButtonText="Confirm"
	secondaryButtonText="Cancel"
	primaryButtonDisabled={portInvalid || ipInvalid}
	on:click:button--secondary={() => (open = false)}
	on:open
	on:close
	on:submit={submit}
>
	<TextInput
		labelText="Ip"
		placeholder={ip_default}
		bind:value={ip}
		invalid={ipInvalid}
		invalidText="Enter the host without a port (IPv6 addresses are not supported)"
	/>
	<TextInput
		labelText="Port"
		placeholder={port_default}
		bind:value={port}
		invalid={portInvalid}
		invalidText="Port must be a number from 1 to 65535"
	/>
	<div style="margin-top: 1rem;">
		<Select
			labelText="MQTT version"
			bind:selected={protocol}
			helperText={protocol === 'auto'
				? 'Uses MQTT 5 (with user properties) and falls back to 3.1.1 for older brokers.'
				: ''}
		>
			<SelectItem value="auto" text="Auto (MQTT 5, fall back to 3.1.1)" />
			<SelectItem value="5" text="MQTT 5" />
			<SelectItem value="3.1.1" text="MQTT 3.1.1" />
		</Select>
	</div>
	<div style="margin-top: 1rem;">
		<Toggle labelText="Use TLS (mqtts)" bind:toggled={use_tls} labelA="Off" labelB="On" />
	</div>
	{#if use_tls}
		<div style="margin-top: 1rem;">
			<TextInput
				labelText="CA certificate (PEM)"
				placeholder="optional, e.g. certs/ca.pem (default: system CAs)"
				bind:value={ca_cert}
			/>
		</div>
		<div style="margin-top: 0.5rem;">
			<TextInput
				labelText="Client certificate (PEM)"
				placeholder="optional, e.g. certs/client.crt"
				bind:value={client_cert}
			/>
		</div>
		<div style="margin-top: 0.5rem;">
			<TextInput
				labelText="Client key (PEM)"
				placeholder="optional, e.g. certs/client.key"
				bind:value={client_key}
			/>
		</div>
		<div style="margin-top: 0.5rem;">
			<PasswordInput
				labelText="Client key password"
				placeholder="optional, only for encrypted keys"
				bind:value={client_key_password}
			/>
		</div>
		<p style="margin-top: 0.5rem; font-size: 0.75rem; color: var(--cds-text-helper);">
			Paths on the backend, relative to the config directory (e.g. /srv/config) or absolute.
		</p>
	{/if}
	<div style="margin-top: 1rem;">
		<TextInput labelText="Username" placeholder="optional" bind:value={username} />
	</div>
	<div style="margin-top: 0.5rem;">
		<PasswordInput labelText="Password" placeholder="optional" bind:value={password} />
	</div>
	{#if $session.mode === 'claims'}
		<div style="margin-top: 1rem;">
			<TextInput
				labelText="Allowed roles"
				placeholder="optional, comma-separated, e.g. plant-a, operators"
				bind:value={allowed_roles}
			/>
		</div>
		<p style="margin-top: 1rem; font-size: 0.75rem; color: var(--cds-text-helper);">
			Users with any of these roles can see this broker; leave empty for every logged-in user.
			Credentials are stored in plain text in the backend config directory.
		</p>
	{:else}
		<div style="margin-top: 1rem;">
			<PasswordInput
				labelText="Access password"
				placeholder="optional, defaults to the MQTT password"
				bind:value={access_password}
			/>
		</div>
		<p style="margin-top: 1rem; font-size: 0.75rem; color: var(--cds-text-helper);">
			Viewers must enter the access password (or, if unset, the MQTT password) to see this broker.
			Credentials are stored in plain text in the backend config directory.
		</p>
	{/if}
</Modal>
