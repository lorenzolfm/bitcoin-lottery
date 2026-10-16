<script lang="ts">
	import { page } from '$app/state';
	import { api, errorMessage, type Round } from '#lib/api.ts';
	import RoundBoard from '#lib/RoundBoard.svelte';

	const POLL_MS = 5000;

	const roundId = $derived(Number(page.params.id));
	let round = $state<Round | null>(null);
	let loadError = $state<string | null>(null);

	$effect(() => {
		const id = roundId;
		let active = true;
		async function refresh() {
			try {
				const latest = await api.round(id);
				if (active) round = latest;
				loadError = null;
			} catch (error) {
				if (active) loadError = errorMessage(error);
			}
		}
		refresh();
		const timer = setInterval(() => round?.status !== 'settled' && refresh(), POLL_MS);
		return () => {
			active = false;
			clearInterval(timer);
		};
	});
</script>

<p><a href="/rounds">← All rounds</a></p>

{#if loadError}
	<p class="error">{loadError}</p>
{/if}
{#if round}
	<RoundBoard {round} />
{:else if !loadError}
	<p class="muted">Loading…</p>
{/if}
