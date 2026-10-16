<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorMessage, type RoundSummary } from '#lib/api.ts';
	import { formatDate, formatNumber } from '#lib/format.ts';

	let rounds = $state<RoundSummary[] | null>(null);
	let loadError = $state<string | null>(null);

	onMount(async () => {
		try {
			rounds = await api.rounds();
		} catch (error) {
			loadError = errorMessage(error);
		}
	});
</script>

<h1>Past rounds</h1>

{#if loadError}
	<p class="error">Could not load rounds: {loadError}</p>
{:else if rounds === null}
	<p class="muted">Loading…</p>
{:else if rounds.length === 0}
	<p class="muted">No rounds yet.</p>
{:else}
	<div class="card table-scroll">
		<table>
			<thead>
				<tr>
					<th>Date</th>
					<th>Round</th>
					<th class="right">Bets</th>
					<th class="right">Block</th>
					<th class="right">Nonce</th>
					<th>Winner</th>
				</tr>
			</thead>
			<tbody>
				{#each rounds as round (round.id)}
					<tr>
						<td class="muted">{formatDate(round.created_at)}</td>
						<td><a href="/rounds/{round.id}">{round.title}</a></td>
						<td class="right num">{round.bet_count}</td>
						<td class="right num">
							{round.target_height === null ? '–' : `#${formatNumber(round.target_height)}`}
						</td>
						<td class="right num">{round.nonce === null ? '–' : formatNumber(round.nonce)}</td>
						<td>
							{#if round.status === 'settled'}
								{round.winner ?? 'no bets'}
							{:else}
								<span class="muted">{round.status === 'open' ? 'betting open' : 'waiting'}</span>
							{/if}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/if}
