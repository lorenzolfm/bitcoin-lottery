<script lang="ts">
	import type { Round } from '#lib/api.ts';
	import { blockUrl, formatNumber, formatTime, shortHash } from '#lib/format.ts';
	import NumberLine from '#lib/NumberLine.svelte';

	let { round, highlightBetId = null }: { round: Round; highlightBetId?: number | null } = $props();

	const winner = $derived(round.status === 'settled' ? round.bets[0] : undefined);
	const blocksToGo = $derived(
		round.target_height !== null && round.tip_height !== null
			? round.target_height - round.tip_height
			: null
	);
	// While waiting, sorting by guess shows who is boxed in by whom.
	const rows = $derived(
		round.status === 'closed'
			? round.bets.toSorted((a, b) => (a.guess ?? 0) - (b.guess ?? 0))
			: round.bets
	);
	const statusLabel = $derived(
		{ open: 'Betting open', closed: 'Waiting for block', settled: 'Settled' }[round.status]
	);
</script>

<section class="card board">
	<header class="board-head">
		<h2>{round.title}</h2>
		<span class="pill {round.status}">{statusLabel}</span>
	</header>

	{#if round.status === 'open'}
		<p class="muted">
			{round.bets.length}
			{round.bets.length === 1 ? 'bet' : 'bets'} so far. Guesses stay sealed until the host closes betting.
		</p>
	{:else if round.status === 'closed' && round.target_height !== null}
		<div class="waiting">
			<span class="pulse" aria-hidden="true"></span>
			<div>
				<p class="headline">
					Waiting for block <span class="num">#{formatNumber(round.target_height)}</span>
				</p>
				<p class="muted">
					{#if round.closed_at}Betting closed at {formatTime(round.closed_at)}.{/if}
					The first block mined after that decides the round.
					{#if round.tip_height !== null && blocksToGo !== null}
						<br />Chain tip <span class="num">#{formatNumber(round.tip_height)}</span> ·
						{blocksToGo <= 0
							? 'block found, settling…'
							: `${blocksToGo} ${blocksToGo === 1 ? 'block' : 'blocks'} to go`}
					{/if}
				</p>
			</div>
		</div>
	{:else if round.block}
		<div class="result">
			{#if winner && winner.guess !== null && winner.distance !== null}
				<p class="eyebrow">Winner</p>
				<p class="winner-name">🏆 {winner.name}</p>
				<dl class="facts">
					<div>
						<dt>Nonce</dt>
						<dd class="num">{formatNumber(round.block.nonce)}</dd>
					</div>
					<div>
						<dt>Guess</dt>
						<dd class="num">{formatNumber(winner.guess)}</dd>
					</div>
					<div>
						<dt>Off by</dt>
						<dd class="num">{formatNumber(winner.distance)}</dd>
					</div>
				</dl>
			{:else}
				<p class="headline">No bets this round.</p>
			{/if}
			<p class="muted block-ref">
				Block <a href={blockUrl(round.block.hash)} target="_blank" rel="noreferrer"
					>#{formatNumber(round.block.height)}</a
				>
				· nonce <span class="num">{formatNumber(round.block.nonce)}</span> ·
				<span class="num" title={round.block.hash}>{shortHash(round.block.hash)}</span>
			</p>
		</div>
	{/if}

	{#if round.status !== 'open' && round.bets.length > 0}
		<NumberLine bets={round.bets} nonce={round.block?.nonce ?? null} />
	{/if}

	{#if rows.length > 0}
		<div class="table-scroll">
			<table>
				<thead>
					<tr>
						{#if round.status === 'settled'}<th>#</th>{/if}
						<th>Name</th>
						<th class="right">Guess</th>
						{#if round.status === 'settled'}<th class="right">Off by</th>{/if}
						{#if round.status === 'open'}<th class="right">Placed</th>{/if}
					</tr>
				</thead>
				<tbody>
					{#each rows as bet (bet.id)}
						<tr class:you={bet.id === highlightBetId} class:first={bet.rank === 1}>
							{#if round.status === 'settled'}<td class="num">{bet.rank}</td>{/if}
							<td>
								{bet.name}
								{#if bet.id === highlightBetId}<span class="tag">you</span>{/if}
							</td>
							<td class="right num">
								{bet.guess === null ? '🔒' : formatNumber(bet.guess)}
							</td>
							{#if round.status === 'settled'}
								<td class="right num">{bet.distance === null ? '' : formatNumber(bet.distance)}</td>
							{/if}
							{#if round.status === 'open'}
								<td class="right muted">{formatTime(bet.created_at)}</td>
							{/if}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</section>

<style>
	.board-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		flex-wrap: wrap;
		margin-bottom: 0.75rem;
	}

	.board-head h2 {
		margin: 0;
		font-size: 1.25rem;
	}

	.pill {
		font-size: 0.8rem;
		font-weight: 700;
		padding: 0.25rem 0.7rem;
		border-radius: 999px;
		background: var(--surface-sunken);
		color: var(--muted);
	}

	.pill.open {
		background: var(--good-soft);
		color: var(--good);
	}

	.pill.closed {
		background: var(--accent-soft);
		color: var(--accent-strong);
	}

	.headline {
		font-size: 1.2rem;
		font-weight: 700;
		margin-bottom: 0.25rem;
	}

	.waiting {
		display: flex;
		gap: 1rem;
		align-items: flex-start;
		padding: 1rem;
		border-radius: 12px;
		background: var(--accent-soft);
	}

	.waiting p:last-child {
		margin-bottom: 0;
	}

	.pulse {
		flex: none;
		width: 14px;
		height: 14px;
		margin-top: 0.45rem;
		border-radius: 50%;
		background: var(--accent);
		animation: pulse 1.6s ease-out infinite;
	}

	@keyframes pulse {
		0% {
			box-shadow: 0 0 0 0 rgb(247 147 26 / 0.6);
		}
		100% {
			box-shadow: 0 0 0 14px rgb(247 147 26 / 0);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.pulse {
			animation: none;
		}
	}

	.result {
		text-align: center;
		padding: 0.5rem 0 0;
	}

	.eyebrow {
		font-size: 0.8rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--muted);
		margin-bottom: 0.25rem;
	}

	.winner-name {
		font-size: clamp(2rem, 7vw, 3.25rem);
		font-weight: 800;
		line-height: 1.1;
		margin-bottom: 1rem;
		overflow-wrap: anywhere;
	}

	.facts {
		display: flex;
		justify-content: center;
		flex-wrap: wrap;
		gap: 0.75rem 2rem;
		margin: 0 0 1rem;
	}

	.facts dt {
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--muted);
	}

	.facts dd {
		margin: 0;
		font-size: 1.15rem;
		font-weight: 700;
	}

	.block-ref {
		font-size: 0.9rem;
	}

	tr.first td {
		font-weight: 700;
		color: var(--good);
	}

	tr.you td {
		background: var(--accent-soft);
	}

	.tag {
		font-size: 0.7rem;
		font-weight: 700;
		text-transform: uppercase;
		background: var(--accent);
		color: var(--on-accent);
		border-radius: 4px;
		padding: 0.05rem 0.35rem;
		margin-left: 0.35rem;
	}
</style>
