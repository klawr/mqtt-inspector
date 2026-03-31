<script lang="ts">
	export let size = 20;
	$: renderSize = Math.round(size * 1.2);
</script>

<img
	src="/favicon.svg"
	alt=""
	aria-hidden="true"
	style={`width:${renderSize}px;height:${renderSize}px;display:block;`}
/>
