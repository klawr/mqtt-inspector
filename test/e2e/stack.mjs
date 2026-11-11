// Test stack for the browser e2e tests: a Mosquitto broker in Docker, the real
// backend serving the built frontend, and a publisher feeding the broker.
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { createServer, connect } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

const REPO_ROOT = resolve(import.meta.dirname, '../..');
/** The backend's HTTP/WebSocket port is fixed. */
export const BACKEND_PORT = 3030;
export const BASE_URL = `http://127.0.0.1:${BACKEND_PORT}/`;

function freePort() {
	return new Promise((ok, fail) => {
		const srv = createServer().once('error', fail);
		srv.listen(0, '127.0.0.1', () => {
			const { port } = srv.address();
			srv.close(() => ok(port));
		});
	});
}

function assertPortFree(port) {
	return new Promise((ok, fail) => {
		const srv = createServer().once('error', () =>
			fail(
				new Error(
					`Port ${port} is already in use. The backend's port is fixed, so stop the ` +
						'other mqtt-inspector instance first (check with: ss -ltnp | grep ' +
						`${port}).`
				)
			)
		);
		srv.listen(port, '0.0.0.0', () => srv.close(ok));
	});
}

async function waitForPort(port, timeoutMs = 30_000) {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		const open = await new Promise((ok) => {
			const sock = connect(port, '127.0.0.1')
				.once('connect', () => (sock.destroy(), ok(true)))
				.once('error', () => ok(false));
		});
		if (open) return;
		await sleep(200);
	}
	throw new Error(`Nothing listening on 127.0.0.1:${port} after ${timeoutMs} ms`);
}

/**
 * Start broker + backend. `maxBrokerMb` is the backend's buffer cap; keep it
 * small so eviction starts within seconds.
 */
export async function startStack({ maxBrokerMb = 1 } = {}) {
	const backendBin = join(REPO_ROOT, 'backend/target/debug/backend');
	const wwwroot = join(REPO_ROOT, 'wwwroot');
	for (const path of [backendBin, join(wwwroot, 'index.html')]) {
		if (!existsSync(path)) throw new Error(`${path} missing, run ./run.sh (it builds both)`);
	}
	await assertPortFree(BACKEND_PORT);

	const dir = mkdtempSync(join(tmpdir(), 'mqtt-inspector-e2e-'));
	const mqttPort = await freePort();
	const broker = `mqtt-inspector-e2e-${mqttPort}`;
	writeFileSync(join(dir, 'mosquitto.conf'), 'listener 1883 0.0.0.0\nallow_anonymous true\n');
	writeFileSync(join(dir, 'brokers.json'), JSON.stringify([{ host: `127.0.0.1:${mqttPort}` }]));

	const procs = [];
	const stop = () => {
		for (const p of procs) p.kill();
		try {
			execFileSync('docker', ['rm', '-f', broker], { stdio: 'ignore' });
		} catch {
			// already gone
		}
		rmSync(dir, { recursive: true, force: true });
	};

	try {
		execFileSync(
			'docker',
			[
				'run',
				'-d',
				'--rm',
				'--name',
				broker,
				'-p',
				`127.0.0.1:${mqttPort}:1883`,
				'-v',
				`${join(dir, 'mosquitto.conf')}:/mosquitto/config/mosquitto.conf:ro`,
				'eclipse-mosquitto:2'
			],
			{ stdio: 'ignore' }
		);
		await waitForPort(mqttPort);

		const backend = spawn(backendBin, [wwwroot, dir], {
			env: { ...process.env, MQTT_INSPECTOR_MAX_BROKER_MB: String(maxBrokerMb) },
			stdio: 'ignore'
		});
		procs.push(backend);
		await waitForPort(BACKEND_PORT);
	} catch (err) {
		stop();
		throw err;
	}

	/**
	 * Publish JSON `{ seq, pad }` messages to `topic` at `rate` msg/s, so tests
	 * can tell from the payload exactly which message is on screen.
	 */
	function publish(topic, { rate = 100, padBytes = 200 } = {}) {
		const pub = spawn('docker', ['exec', '-i', broker, 'mosquitto_pub', '-l', '-t', topic], {
			stdio: ['pipe', 'ignore', 'ignore']
		});
		procs.push(pub);
		pub.stdin.on('error', () => {}); // EPIPE when stopped mid-write
		const pad = 'x'.repeat(padBytes);
		const tickMs = 100;
		let seq = 0;
		const timer = setInterval(() => {
			let lines = '';
			for (let i = 0; i < (rate * tickMs) / 1000; i++) {
				lines += JSON.stringify({ seq: seq++, pad }) + '\n';
			}
			pub.stdin.write(lines);
		}, tickMs);
		pub.once('exit', () => clearInterval(timer));
		return pub;
	}

	return { brokerHost: `127.0.0.1:${mqttPort}`, publish, stop };
}
