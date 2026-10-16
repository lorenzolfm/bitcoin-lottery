<script lang="ts">
	import { MAX_GUESS, type Bet } from '#lib/api.ts';
	import { formatNumber } from '#lib/format.ts';

	let { bets, nonce }: { bets: Bet[]; nonce: number | null } = $props();

	const guesses = $derived(
		bets.flatMap((bet) => (bet.guess === null ? [] : [{ ...bet, guess: bet.guess }]))
	);
	const label = $derived(
		`${guesses.length} guesses across the nonce range` +
			(nonce === null ? '' : `, nonce at ${formatNumber(nonce)}`)
	);

	function position(value: number): string {
		return `${(value / MAX_GUESS) * 100}%`;
	}
</script>

<figure class="line" role="img" aria-label={label}>
	<div class="track">
		{#each guesses as bet (bet.id)}
			<span
				class="dot"
				class:winner={bet.rank === 1}
				style:left={position(bet.guess)}
				title="{bet.name}: {formatNumber(bet.guess)}"
			></span>
		{/each}
		{#if nonce !== null}
			<span class="nonce" style:left={position(nonce)}>
				<span class="nonce-label">nonce</span>
			</span>
		{/if}
	</div>
	<div class="scale num">
		<span>0</span>
		<span>2.1B</span>
		<span>4.3B</span>
	</div>
</figure>

<style>
	.line {
		margin: 2.75rem 0 1.25rem;
		padding: 0 6px;
	}

	.track {
		position: relative;
		height: 10px;
		border-radius: 999px;
		background: var(--surface-sunken);
		border: 1px solid var(--border);
	}

	.dot {
		position: absolute;
		top: 50%;
		width: 12px;
		height: 12px;
		margin: -6px 0 0 -6px;
		border-radius: 50%;
		background: var(--muted);
		border: 2px solid var(--surface);
		opacity: 0.8;
	}

	.dot.winner {
		width: 16px;
		height: 16px;
		margin: -8px 0 0 -8px;
		background: var(--good);
		opacity: 1;
		z-index: 2;
	}

	.nonce {
		position: absolute;
		top: -14px;
		bottom: -14px;
		width: 3px;
		margin-left: -1.5px;
		background: var(--accent);
		border-radius: 2px;
		z-index: 1;
	}

	.nonce-label {
		position: absolute;
		top: -1.3rem;
		left: 50%;
		transform: translateX(-50%);
		font-size: 0.7rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--accent-strong);
	}

	.scale {
		display: flex;
		justify-content: space-between;
		font-size: 0.75rem;
		color: var(--muted);
		margin-top: 0.4rem;
	}
</style>
