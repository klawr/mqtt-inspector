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
	import { onDestroy, onMount } from 'svelte';
	import { prettyPrint } from './messages';
	import jsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';

	export let readonly = false;
	export let code: string = '';
	export let result: string = '';

	let editorElement: HTMLDivElement;

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	let editor: any;

	onMount(async () => {
		const monaco = await import('monaco-editor');

		self.MonacoEnvironment = {
			// eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/no-unused-vars
			getWorker: function (_: any, label: string) {
				return new jsonWorker();
			}
		};

		monaco.languages.typescript.typescriptDefaults.setEagerModelSync(true);
		editor = monaco.editor.create(editorElement, {
			readOnly: readonly,
			automaticLayout: true,
			theme: 'vs-dark',
			language: 'json'
		});

		editor.onDidChangeModelContent(() => {
			result = editor.getValue();
		});
	});

	onDestroy(async () => {
		editor?.dispose();
	});

	$: if (editor) {
		editor.setValue(code ? prettyPrint(code) : '');
		result = editor.getValue();
	}
</script>

<div style="height: 100%" bind:this={editorElement} />
