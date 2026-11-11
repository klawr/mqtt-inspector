// Locking a message must keep it on screen while messages stream in and the
// backend buffer is full (every new message comes with an eviction), including
// when stepping left/right and in the compare (diff) view.
//
// Elements are found by role/visible text, or by data-testid where there is no
// user-facing handle, never by component-library class names.
import { after, before, describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { BASE_URL, startStack } from './stack.mjs';

const TOPIC = 'e2e/fast';
// 200 msg/s of ~250 B: a 1 MB buffer holds ~20 s, so it fills (and a locked
// message gets evicted) within that time, while the tests before the eviction
// test finish well inside it.
const RATE = 200;
const PAD_BYTES = 200;

describe('message lock under a full buffer', () => {
	let stack, browser, page;

	before(async () => {
		stack = await startStack({ maxBrokerMb: 1 });
		stack.publish(TOPIC, { rate: RATE, padBytes: PAD_BYTES });
		browser = await chromium.launch();
		page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
		await page.goto(BASE_URL);
		await page.getByRole('treeitem', { name: /^e2e/ }).waitFor();
		await page.getByRole('button', { name: 'Expand all' }).click();
		await page.getByRole('treeitem', { name: /^fast/ }).click();
		await waitForFullBuffer();
	});

	after(async () => {
		await browser?.close();
		stack?.stop();
	});

	// --- reading the UI ---------------------------------------------------

	/** The `seq` values in the editor: one normally, two (sorted) in diff mode. */
	const seqs = () =>
		page
			.getByTestId('message-editor')
			.innerText()
			.then((text) => {
				const all = [...text.matchAll(/"seq":\s*(\d+)/g)].map((m) => Number(m[1]));
				return [...new Set(all)].sort((a, b) => a - b);
			})
			.catch(() => []);

	/** `seq` of the shown message (single editor). */
	const shown = async () => (await seqs())[0];

	/** `[position, total]` from the "x / N messages" counter; position is '–' while evicted. */
	async function counter() {
		const text = await page.getByTestId('message-counter').innerText();
		const [, position, total] = text.match(/(\S+) \/ (\d+)/);
		return [position === '–' ? position : Number(position), Number(total)];
	}

	/** The buffer is full once the message count stops reaching new highs. */
	async function waitForFullBuffer(timeoutMs = 90_000) {
		const deadline = Date.now() + timeoutMs;
		let max = 0;
		let lastHigh = Date.now();
		while (Date.now() < deadline) {
			const [, total] = await counter().catch(() => [0, 0]);
			if (total > max) [max, lastHigh] = [total, Date.now()];
			if (max > 100 && Date.now() - lastHigh > 2000) return;
			await page.waitForTimeout(250);
		}
		throw new Error(`buffer not full after ${timeoutMs} ms (max ${max} messages)`);
	}

	/**
	 * Sample for `ms`: the shown message(s) must never change and the counter
	 * must stay in range. At RATE msg/s any drift shows within one sample.
	 */
	async function assertStable(ms) {
		const first = await seqs();
		assert.ok(first.length > 0, 'no message shown');
		for (let t = 0; t < ms; t += 200) {
			await page.waitForTimeout(200);
			assert.deepEqual(await seqs(), first, `shown message changed after ${t + 200} ms`);
			const [position, total] = await counter();
			assert.ok(position === '–' || position <= total, `counter: ${position} / ${total}`);
		}
		return first;
	}

	/** Run `action`, then wait for the editor to repaint (Monaco renders a frame later). */
	async function shownAfter(action) {
		const before = await shown();
		await action();
		for (let t = 0; t < 2000; t += 50) {
			const now = await shown();
			if (now !== undefined && now !== before) return now;
			await page.waitForTimeout(50);
		}
		return shown();
	}

	const click = (name) => shownAfter(() => page.getByRole('button', { name }).click());
	const toggle = (label) => page.getByText(label, { exact: true }).click();
	const evictedNotice = () => page.getByText('locked message evicted from buffer');

	// --- tests ------------------------------------------------------------

	it('follows the newest message while unlocked', async () => {
		const before = await shown();
		await page.waitForTimeout(500);
		assert.ok((await shown()) > before);
	});

	it('keeps a locked message on screen', async () => {
		await toggle('Lock message');
		await assertStable(2000);
	});

	it('steps exactly one message older/newer and stays there', async () => {
		const locked = await shown();
		assert.equal(await click('Previous Message'), locked - 1);
		await assertStable(1000);

		assert.equal(await click('Next Message'), locked);
		assert.equal(await click('Next Message'), locked + 1);
		await assertStable(1000);

		await page.getByTestId('message-timeline').hover();
		assert.equal(await shownAfter(() => page.mouse.wheel(0, 100)), locked);
		assert.equal(await shownAfter(() => page.mouse.wheel(0, 100)), locked - 1);
		await assertStable(1000);
	});

	it('keeps both sides of the diff on screen', async () => {
		await toggle('Compare message');
		await toggle('Lock');
		// The diff editor draws a frame after the content changes.
		await page.waitForTimeout(500);
		const [a, b] = await assertStable(3000);
		assert.ok(a !== undefined && b !== undefined, `diff shows ${a} vs ${b}`);

		await toggle('Compare message');
		await page.waitForFunction(
			() =>
				document.querySelector('[data-testid="message-editor"]')?.innerText.match(/"seq"/g)
					?.length === 1
		);
	});

	it('keeps showing a locked message after it is evicted, with a notice', async () => {
		const locked = await shown();
		await evictedNotice().waitFor({ timeout: 60_000 });
		assert.equal(await shown(), locked);
		assert.equal((await counter())[0], '–');
		await assertStable(2000);
	});

	it('steps from an evicted message to the oldest kept one', async () => {
		const evicted = await shown();
		assert.ok((await click('Next Message')) > evicted);
		// That is next in line for eviction, so the notice may come straight back;
		// either way the message must stay on screen.
		await assertStable(1000);
	});
});
