<script lang="ts">
	import { api, errorMessage, MAX_GUESS, NAME_MAX_CHARS, type Round } from '#lib/api.ts';
	import { formatNumber, parseGuess } from '#lib/format.ts';
	import RoundBoard from '#lib/RoundBoard.svelte';
	import * as storage from '#lib/storage.ts';

	type MyBet = { id: number; name: string; guess: number };

	const POLL_MS = 5000;
	const NAME_KEY = 'lottery:name';
	const BETS_KEY = 'lottery:bets';

	let round = $state<Round | null>(null);
	let loaded = $state(false);
	let loadError = $state<string | null>(null);

	let name = $state(storage.load(NAME_KEY) ?? '');
	let guessText = $state('');
	let submitting = $state(false);
	let submitError = $state<string | null>(null);
	/** Bets placed from this device, by round id. */
	let myBets = $state<Record<number, MyBet>>(readMyBets());

	const myBet = $derived(round ? (myBets[round.id] ?? null) : null);
	const guess = $derived(parseGuess(guessText));
	const guessValid = $derived(guess !== null && guess <= MAX_GUESS);
	const nameLength = $derived([...name.trim()].length);
	const nameValid = $derived(nameLength > 0 && nameLength <= NAME_MAX_CHARS);

	$effect(() => {
		refresh();
		const timer = setInterval(refresh, POLL_MS);
		return () => clearInterval(timer);
	});

	async function refresh() {
		try {
			round = await api.currentRound();
			loadError = null;
		} catch (error) {
			loadError = errorMessage(error);
		} finally {
			loaded = true;
		}
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!round || guess === null || !guessValid || !nameValid) return;
		submitting = true;
		submitError = null;
		try {
			const bet = await api.placeBet(round.id, name.trim(), guess);
			myBets = { ...myBets, [round.id]: { id: bet.id, name: bet.name, guess } };
			storage.save(BETS_KEY, JSON.stringify(myBets));
			storage.save(NAME_KEY, bet.name);
			guessText = '';
			await refresh();
		} catch (error) {
			submitError = errorMessage(error);
		} finally {
			submitting = false;
		}
	}

	function randomGuess() {
		guessText = formatNumber(crypto.getRandomValues(new Uint32Array(1))[0]);
	}

	function readMyBets(): Record<number, MyBet> {
		try {
			return JSON.parse(storage.load(BETS_KEY) ?? '{}');
		} catch {
			return {};
		}
	}
</script>

{#if !loaded}
	<p class="muted">Loading…</p>
{:else if !round}
	{#if loadError}
		<p class="error">Could not reach the lottery: {loadError}</p>
	{:else}
		<section class="card intro">
			<h1>No round yet</h1>
			<p class="muted">
				The host opens a round at the start of the meetup. Keep this page open; it updates on its
				own.
			</p>
		</section>
	{/if}
{:else}
	{#if loadError}
		<p class="error">Lost contact with the server; retrying. ({loadError})</p>
	{/if}

	{#if myBet}
		<section class="card mine">
			<p class="eyebrow">Your bet</p>
			<p>
				<strong>{myBet.name}</strong> guessed
				<strong class="num">{formatNumber(myBet.guess)}</strong>
			</p>
			{#if round.status === 'open'}
				<p class="muted">Good luck! Results appear here once the deciding block is mined.</p>
			{/if}
		</section>
	{:else if round.status === 'open'}
		<form class="card bet-form" onsubmit={submit}>
			<h1>Guess the nonce</h1>
			<p class="muted">
				When the host closes betting, the next block mined decides. Its nonce is a number from 0 to
				<span class="num">{formatNumber(MAX_GUESS)}</span>. Closest guess wins; a tie goes to
				whoever bet first.
			</p>

			<label>
				Name
				<input bind:value={name} autocomplete="nickname" placeholder="Satoshi" required />
			</label>
			{#if nameLength > NAME_MAX_CHARS}
				<p class="error">Keep your name to {NAME_MAX_CHARS} characters.</p>
			{/if}

			<label for="guess">Your guess</label>
			<div class="guess-row">
				<input
					id="guess"
					class="num"
					bind:value={guessText}
					inputmode="numeric"
					autocomplete="off"
					placeholder="2,147,483,648"
					required
				/>
				<button type="button" class="secondary" onclick={randomGuess}>🎲 Random</button>
			</div>
			{#if guessText && !guessValid}
				<p class="error">Enter a whole number from 0 to {formatNumber(MAX_GUESS)}.</p>
			{/if}

			{#if submitError}
				<p class="error">{submitError}</p>
			{/if}
			<button type="submit" class="submit" disabled={submitting || !guessValid || !nameValid}>
				{submitting ? 'Placing bet…' : 'Place bet'}
			</button>
		</form>
	{/if}

	<RoundBoard {round} highlightBetId={myBet?.id ?? null} />
{/if}

<style>
	.intro {
		text-align: center;
		padding: 2.5rem 1.25rem;
	}

	.bet-form h1 {
		font-size: 1.6rem;
	}

	.bet-form > label[for='guess'] {
		margin-bottom: 0.35rem;
	}

	.guess-row {
		display: flex;
		gap: 0.5rem;
		margin-bottom: 1rem;
	}

	.guess-row input {
		font-size: 1.15rem;
	}

	.guess-row button {
		flex: none;
	}

	.submit {
		width: 100%;
		font-size: 1.05rem;
		padding: 0.85rem;
	}

	.mine {
		border-color: var(--accent);
		background: var(--accent-soft);
	}

	.mine p:last-child {
		margin-bottom: 0;
	}

	.eyebrow {
		font-size: 0.75rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--accent-strong);
		margin-bottom: 0.25rem;
	}
</style>
