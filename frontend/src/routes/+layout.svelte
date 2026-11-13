<script lang="ts">
	import './layout.css';
	import { onMount } from 'svelte';
	import { api } from '../lib/api';
	import {
		user,
		workspaceConnection,
		workspaceLowStock,
		detailNavigation,
		type User
	} from '../lib/auth';
	import Icon from '../lib/Icon.svelte';
	import { page as route } from '$app/state';
	let { children } = $props();
	let ready = $state(false),
		username = $state(''),
		password = $state(''),
		error = $state(''),
		busy = $state(false);
	let profileMenu: HTMLDetailsElement | undefined = $state();
	const workspaceLinks = [
		{ label: 'Dashboard', icon: 'dashboard', href: '/' },
		{ label: 'Stock forecast', icon: 'forecast', href: '/?view=Stock%20forecast' },
		{ label: 'Products', icon: 'products', href: '/?view=Products' },
		{ label: 'Purchase orders', icon: 'order', href: '/?view=Purchase%20orders' },
		{ label: 'Purchase invoices', icon: 'invoice', href: '/?view=Purchase%20invoices' },
		{ label: 'Payments', icon: 'payment', href: '/?view=Payments' },
		{ label: 'Sales invoices', icon: 'sale', href: '/?view=Sales%20invoices' },
		{ label: 'Delivery orders', icon: 'delivery', href: '/delivery-orders/' },
		{ label: 'Approvals', icon: 'check', href: '/approvals/' }
	];
	const roleLinks = [
		{ section: 'roles', label: 'Role list' },
		{ section: 'role-editor', label: 'Create role' },
		{ section: 'assignments', label: 'User assignments' },
		{ section: 'users', label: 'Create user' }
	];
	let currentView = $derived(
		route.url.pathname === '/'
			? route.url.searchParams.get('view') || 'Dashboard'
			: route.url.pathname.startsWith('/roles')
				? 'Roles and permissions'
				: route.url.pathname.startsWith('/delivery-orders')
					? 'Delivery orders'
					: route.url.pathname.startsWith('/approvals')
						? 'Approvals'
						: route.url.pathname.startsWith('/documents')
							? $detailNavigation || 'Record details'
							: route.url.pathname.startsWith('/products')
								? 'Products'
								: route.url.pathname.startsWith('/payments')
									? 'Payments'
									: 'Account'
	);
	let roleSection = $derived(route.url.searchParams.get('section') || 'roles');
	let initials = $derived(
		($user?.username || 'U')
			.split(/[\s._-]+/)
			.filter(Boolean)
			.slice(0, 2)
			.map((part) => part[0])
			.join('')
			.toUpperCase()
	);
	function closeProfile(event: MouseEvent) {
		if (profileMenu && !profileMenu.contains(event.target as Node)) profileMenu.open = false;
	}
	function refreshWorkspace() {
		if (route.url.pathname === '/') window.dispatchEvent(new Event('workspace-refresh'));
		else window.location.reload();
	}
	onMount(() => {
		const expired = () => user.set(null);
		window.addEventListener('auth-expired', expired);
		(async () => {
			try {
				user.set(await api<User>('/auth/me'));
			} catch {
				try {
					await api('/auth/refresh', {});
					user.set(await api<User>('/auth/me'));
				} catch {
					user.set(null);
				}
			} finally {
				ready = true;
			}
		})();
		return () => window.removeEventListener('auth-expired', expired);
	});
	async function login(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = '';
		try {
			await api('/auth/login', { username, password });
			password = '';
			user.set(await api<User>('/auth/me'));
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
	async function logout() {
		busy = true;
		try {
			await api('/auth/logout', {});
			user.set(null);
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<svelte:window
	onclick={closeProfile}
	onkeydown={(event) => {
		if (event.key === 'Escape' && profileMenu) {
			profileMenu.open = false;
			profileMenu.querySelector('summary')?.focus();
		}
	}}
/>
{#if !ready}<main class="auth-card">Loading session…</main>
{:else if !$user}
	<main class="auth-card">
		<h1>Sign in to Stockroom</h1>
		<p>Use your assigned account to continue.</p>
		<form onsubmit={login}>
			<label>Username<input autocomplete="username" bind:value={username} required /></label><label
				>Password<input
					type="password"
					autocomplete="current-password"
					bind:value={password}
					required
				/></label
			>{#if error}<p role="alert">{error}</p>{/if}<button disabled={busy}
				>{busy ? 'Signing in…' : 'Sign in'}</button
			>
		</form>
	</main>
{:else}
	<div class="app-shell">
		<aside class="sidebar" aria-label="Main navigation">
			<a class="brand" href="/" aria-label="Stockroom home"
				><span class="brand-icon"><Icon name="products" size={23} /></span> stockroom<span
					class="brand-dot">.</span
				></a
			>
			<div class="workspace">
				<div class="workspace-icon">S</div>
				<div><strong>My workspace</strong><small>Business workspace</small></div>
				<span class="workspace-chevron">&#8964;</span>
			</div>
			<nav aria-label="Workspace">
				{#each [{ title: 'OVERVIEW', links: workspaceLinks.slice(0, 2) }, { title: 'WORKSPACE', links: workspaceLinks.slice(2) }] as group}
					<div class="nav-group">
						<p>{group.title}</p>
						{#each group.links as link}<a
								class="sidebar-link"
								class:active={currentView === link.label}
								aria-current={currentView === link.label ? 'page' : undefined}
								href={link.href}
								aria-label={link.label}
								onclick={() => window.dispatchEvent(new Event('workspace-navigate'))}
								title={link.label}
								><span class="nav-icon"><Icon name={link.icon} /></span><span class="nav-label"
									>{link.label}</span
								>{#if link.label === 'Products' && $workspaceLowStock}<span class="nav-count"
										>{$workspaceLowStock}</span
									>{/if}</a
							>{/each}
					</div>
				{/each}

				{#if route.url.pathname.startsWith('/approvals')}<div
						class="sidebar-submenu"
						aria-label="Approval sections"
					>
						<a
							href="/approvals/?section=invoices"
							class:active={route.url.searchParams.get('section') !== 'deliveries' &&
								!route.url.pathname.startsWith('/approvals/deliveries/')}>Invoice approvals</a
						>
						<a
							href="/approvals/?section=deliveries"
							class:active={route.url.searchParams.get('section') === 'deliveries' ||
								route.url.pathname.startsWith('/approvals/deliveries/')}>Delivery cancellations</a
						>
					</div>{/if}
				{#if $user.permissions.includes('roles.manage')}<div class="nav-group">
						<p>ADMINISTRATION</p>
						<a
							class="sidebar-link"
							class:active={currentView === 'Roles and permissions'}
							href="/roles/"
							title="Roles and permissions"
							><span class="nav-icon"><Icon name="roles" /></span><span class="nav-label"
								>Roles and permissions</span
							></a
						>
						{#if route.url.pathname.startsWith('/roles')}<div class="sidebar-submenu">
								{#each roleLinks as link}<a
										href={`/roles/?section=${link.section}`}
										class:active={roleSection === link.section}
										aria-current={roleSection === link.section ? 'page' : undefined}>{link.label}</a
									>{/each}
							</div>{/if}
					</div>{/if}
			</nav>
			<div class="sidebar-bottom">
				<div class="tip">
					<span><Icon name="spark" size={28} /></span><strong
						>A little foresight goes a long way.</strong
					>
					<p>Know what to restock before your shelves run empty.</p>
					<a href="/?view=Stock%20forecast">Explore forecast <Icon name="arrow" size={16} /></a>
				</div>
				<div class="profile">
					<div class="avatar">{initials}</div>
					<div><strong>{$user.username}</strong><small>{$user.role}</small></div>
					<span class="online-dot"></span>
				</div>
			</div>
		</aside>
		<div class="main-shell">
			<header class="topbar">
				<div class="breadcrumb">Workspace <span>/</span><strong>{currentView}</strong></div>
				<div class="top-actions">
					<span class="connection" class:offline={!$workspaceConnection}
						><i></i>{$workspaceConnection ? 'Live workspace' : 'API offline'}</span
					><button class="icon-button" aria-label="Refresh workspace" onclick={refreshWorkspace}
						><Icon name="refresh" /></button
					>
					<details class="profile-menu" bind:this={profileMenu}>
						<summary class="avatar small" aria-label="Open user menu" title={$user.username}
							>{initials}</summary
						>
						<div class="profile-dropdown">
							<div class="profile-menu-user">
								<strong>{$user.username}</strong><span>{$user.role}</span>
							</div>
							<a
								href="/account/"
								onclick={() => {
									if (profileMenu) profileMenu.open = false;
								}}>Account settings</a
							><button onclick={logout} disabled={busy}>Sign out</button>{#if error}<p role="alert">
									{error}
								</p>{/if}
						</div>
					</details>
				</div>
			</header>
			{@render children()}
		</div>
	</div>
{/if}

<style>
	.auth-card {
		max-width: 420px;
		margin: 12vh auto;
		padding: 32px;
		background: white;
		border: 1px solid #d9e1e8;
		border-radius: 16px;
	}
	.auth-card form,
	.auth-card label {
		display: grid;
		gap: 12px;
	}
	.auth-card form {
		gap: 20px;
	}
	.auth-card input {
		padding: 12px;
		border: 1px solid #b8c4cf;
		border-radius: 6px;
	}
	.auth-card button {
		padding: 10px 16px;
		background: #244f43;
		color: white;
		border: 0;
		border-radius: 6px;
	}
</style>
