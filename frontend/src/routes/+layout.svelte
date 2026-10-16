<script lang="ts">
	import '../app.css';
	import favicon from '#lib/assets/favicon.svg';
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const links = [
		{ href: '/', label: 'Play' },
		{ href: '/rounds', label: 'History' },
		{ href: '/admin', label: 'Host' }
	];

	function isCurrent(href: string): boolean {
		return href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);
	}
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>BitDevs Lottery</title>
</svelte:head>

<header class="site">
	<a class="brand" href="/">
		<img src={favicon} alt="" width="28" height="28" />
		<span>BitDevs Lottery</span>
	</a>
	<nav aria-label="Main">
		{#each links as link (link.href)}
			<a href={link.href} aria-current={isCurrent(link.href) ? 'page' : undefined}>{link.label}</a>
		{/each}
	</nav>
</header>

<main>
	{@render children()}
</main>

<footer class="muted">Guess the nonce of the next block · closest guess wins</footer>

<style>
	.site {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		flex-wrap: wrap;
		max-width: 860px;
		margin: 0 auto;
		padding: 1rem 16px;
	}

	.brand {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		font-weight: 800;
		font-size: 1.15rem;
		color: var(--text);
		text-decoration: none;
	}

	nav {
		display: flex;
		gap: 0.25rem;
	}

	nav a {
		color: var(--muted);
		text-decoration: none;
		font-weight: 600;
		padding: 0.4rem 0.75rem;
		border-radius: 999px;
	}

	nav a[aria-current='page'] {
		color: var(--text);
		background: var(--accent-soft);
	}

	main {
		max-width: 860px;
		margin: 0 auto;
		padding: 0.5rem 16px 2rem;
	}

	footer {
		text-align: center;
		font-size: 0.85rem;
		padding: 1rem 16px 2rem;
	}
</style>
