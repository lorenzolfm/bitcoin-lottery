<script lang="ts">
	import { onMount } from 'svelte';
	import { admin, api, ApiError, errorMessage, type Bet, type Round } from '#lib/api.ts';
	import { formatNumber } from '#lib/format.ts';
	import RoundBoard from '#lib/RoundBoard.svelte';
	import * as storage from '#lib/storage.ts';

	const POLL_MS = 5000;
	const TOKEN_KEY = 'lottery:admin-token';

	/** Only set once the server has accepted it. */
	let token = $state<string | null>(null);
	let tokenInput = $state('');
	let round = $state<Round | null>(null);
	let title = $state(defaultTitle());
	let busy = $state(false);
	let actionError = $state<string | null>(null);

	const running = $derived(round !== null && round.status !== 'settled');

	onMount(() => {
		const saved = storage.load(TOKEN_KEY);
		if (saved) signIn(saved);
	});

	$effect(() => {
		if (!token) return;
		refresh();
		const timer = setInterval(refresh, POLL_MS);
		return () => clearInterval(timer);
	});

	async function signIn(candidate: string) {
		busy = true;
		actionError = null;
		try {
			await admin.checkToken(candidate);
			token = candidate;
			storage.save(TOKEN_KEY, candidate);
		} catch (error) {
			storage.save(TOKEN_KEY, null);
			actionError =
				error instanceof ApiError && error.status === 401
					? 'That token was not accepted.'
					: errorMessage(error);
		} finally {
			busy = false;
		}
	}

	function signOut() {
		token = null;
		tokenInput = '';
		storage.save(TOKEN_KEY, null);
	}

	async function refresh() {
		try {
			round = await api.currentRound();
		} catch (error) {
			actionError = errorMessage(error);
		}
	}

	async function act(action: (token: string) => Promise<unknown>) {
		if (!token) return;
		busy = true;
		actionError = null;
		try {
			await action(token);
			await refresh();
		} catch (error) {
			if (error instanceof ApiError && error.status === 401) signOut();
			actionError = errorMessage(error);
		} finally {
			busy = false;
		}
	}

	function createRound(event: SubmitEvent) {
		event.preventDefault();
		act((token) => admin.createRound(token, title.trim()));
	}

	function closeBetting(target: Round) {
		const count = target.bets.length;
		const message = `Close betting with ${count} ${count === 1 ? 'bet' : 'bets'}? Nobody can bet after this, and the next block mined decides the round.`;
		if (confirm(message)) act((token) => admin.closeRound(token, target.id));
	}

	function removeBet(bet: Bet) {
		if (confirm(`Remove the bet from "${bet.name}"?`))
			act((token) => admin.deleteBet(token, bet.id));
	}

	function defaultTitle(): string {
		const month = new Date().toLocaleDateString('en-US', { month: 'long', year: 'numeric' });
		return `BitDevs · ${month}`;
	}
</script>

<h1>Host controls</h1>

{#if actionError}
	<p class="error">{actionError}</p>
{/if}

{#if !token}
	<form
		class="card"
		onsubmit={(event) => {
			event.preventDefault();
			signIn(tokenInput.trim());
		}}
	>
		<label>
			Admin token
			<input type="password" bind:value={tokenInput} autocomplete="current-password" required />
		</label>
		<p class="muted">The value of <code>ADMIN_TOKEN</code> the server was started with.</p>
		<button type="submit" disabled={busy || !tokenInput.trim()}>Sign in</button>
	</form>
{:else}
	{#if !running}
		<form class="card" onsubmit={createRound}>
			<h2>Open a new round</h2>
			<label>
				Title
				<input bind:value={title} maxlength="80" required />
			</label>
			<button type="submit" disabled={busy || !title.trim()}>Open betting</button>
		</form>
	{:else if round}
		<section class="card controls">
			{#if round.status === 'open'}
				<div>
					<h2>Betting is open</h2>
					<p class="muted">
						{round.bets.length}
						{round.bets.length === 1 ? 'bet' : 'bets'} so far. Put the Play page on the projector and
						close betting when everyone is in.
					</p>
				</div>
				<button disabled={busy} onclick={() => round && closeBetting(round)}>Close betting</button>
			{:else}
				<div>
					<h2>Waiting for block #{formatNumber(round.target_height ?? 0)}</h2>
					<p class="muted">The round settles on its own once the block is mined.</p>
				</div>
			{/if}
		</section>

		{#if round.bets.length > 0}
			<section class="card">
				<h2>Bets</h2>
				<p class="muted">
					Remove a bet before the round settles, for example a name that should not be on screen.
				</p>
				<ul class="bets">
					{#each round.bets as bet (bet.id)}
						<li>
							<span>{bet.name}</span>
							<button class="danger" disabled={busy} onclick={() => removeBet(bet)}>Remove</button>
						</li>
					{/each}
				</ul>
			</section>
		{/if}
	{/if}

	{#if round}
		<RoundBoard {round} />
	{/if}

	<p><button class="secondary" onclick={signOut}>Sign out</button></p>
{/if}

<style>
	.controls {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		flex-wrap: wrap;
	}

	.controls p {
		margin: 0;
	}

	.bets {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.bets li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.45rem 0;
		border-bottom: 1px solid var(--border);
		overflow-wrap: anywhere;
	}

	code {
		font-family: var(--mono);
		font-size: 0.9em;
	}
</style>
