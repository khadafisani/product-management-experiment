<script lang="ts">
	import { untrack } from 'svelte';
	import CursorList from '../../lib/CursorList.svelte';
	import RowActions from '../../lib/RowActions.svelte';
	let refresh = $state(0);
	import { page as route } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '../../lib/api';
	type Role = { id: number; name: string; permissions: string[] };
	type User = { id: number; username: string; role_id: number; active: boolean };
	let data = $state<{ roles: Role[]; permissions: string[] }>({ roles: [], permissions: [] }),
		users = $state<User[]>([]),
		error = $state(''),
		notice = $state(''),
		name = $state(''),
		selected = $state<string[]>([]),
		userId = $state(0),
		roleId = $state(0),
		active = $state(true),
		busy = $state(false),
		editing = $state<number | null>(null);
	let newUsername = $state(''),
		newPassword = $state(''),
		newRole = $state(0);
	const sections = [
		{ id: 'roles', label: 'Role list' },
		{ id: 'role-editor', label: 'Create role' },
		{ id: 'assignments', label: 'User assignments' },
		{ id: 'users', label: 'Create user' }
	];
	let section = $derived(
		sections.some((s) => s.id === route.url.searchParams.get('section'))
			? route.url.searchParams.get('section')!
			: 'roles'
	);
	const permissionLabels: Record<string, string> = {
		'delivery.approve': 'Approve and cancel approved deliveries',
		'delivery.read': 'View delivery orders',
		'delivery.write': 'Create, edit and dispatch deliveries',
		'invoices.read': 'View invoices and purchase orders',
		'invoices.write': 'Create and edit invoices',
		'invoices.request_void': 'Request invoice voids',
		'invoices.void': 'Void invoices and review requests',
		'payments.read': 'View payments',
		'payments.write': 'Record payments',
		'products.read': 'View products',
		'products.write': 'Manage products and stock',
		'profile.read': 'Access own account',
		'roles.manage': 'Manage roles and users',
		'search.manage': 'Rebuild search index'
	};
	const groupLabels: Record<string, string> = {
		delivery: 'Delivery orders',
		invoices: 'Invoices',
		payments: 'Payments',
		products: 'Products',
		profile: 'Account',
		roles: 'Administration',
		search: 'Search'
	};
	let permissionGroups = $derived(
		[...new Set(data.permissions.map((p) => p.split('.')[0]))].map((group) => ({
			name: groupLabels[group] || group,
			permissions: data.permissions.filter((p) => p.startsWith(group + '.'))
		}))
	);
	$effect(() => {
		const editId = Number(route.url.searchParams.get('edit'));
		const role = data.roles.find((r) => r.id === editId);
		if (section === 'role-editor') {
			editing = editId || null;
			name = role?.name || '';
			selected = role ? [...role.permissions] : [];
		}
	});
	async function load() {
		refresh++;
		try {
			[data, users] = await Promise.all([api<typeof data>('/roles'), api<User[]>('/users')]);
		} catch (e) {
			error = (e as Error).message;
		}
	}
	$effect(() => {
		if (section !== 'roles') untrack(() => void load());
	});
	async function create(e: SubmitEvent) {
		e.preventDefault();
		if (busy) return;
		busy = true;
		error = '';
		try {
			await api(editing ? `/roles/${editing}` : '/roles', { name, permissions: selected });
			name = '';
			selected = [];
			editing = null;
			notice = 'Role saved.';
			await load();
			await goto('/roles/');
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
	async function assign(e: SubmitEvent) {
		e.preventDefault();
		if (busy) return;
		busy = true;
		error = '';
		try {
			await api(`/users/${userId}/role`, { role_id: roleId, active });
			notice = 'Assignment saved; existing sessions revoked.';
			await load();
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
	function selectUser() {
		const u = users.find((u) => u.id === userId);
		if (u) {
			roleId = u.role_id;
			active = u.active;
		}
	}
	async function createUser(e: SubmitEvent) {
		e.preventDefault();
		if (busy) return;
		busy = true;
		error = '';
		try {
			await api('/users', { username: newUsername, password: newPassword, role_id: newRole });
			newUsername = '';
			newPassword = '';
			notice = 'User created.';
			await load();
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Roles and permissions ? Stockroom</title></svelte:head>
<main
	class="feature-page role-page"
	class:list-page={section === 'roles' || section === 'assignments'}
>
	<div class="page-heading">
		<div>
			<div class="eyebrow">WORKSPACE ADMINISTRATION</div>
			<h1>Roles and permissions</h1>
			<p>Control who can access your workspace and what they can do.</p>
		</div>
	</div>
	<nav class="section-tabs" aria-label="Role management sections">
		{#each sections as item}<a
				href={`/roles/?section=${item.id}`}
				class:active={section === item.id}
				aria-current={section === item.id ? 'page' : undefined}
				>{item.id === 'role-editor' && editing ? 'Edit role' : item.label}</a
			>{/each}
	</nav>
	{#if error}<p role="alert" class="error">{error}</p>{/if}{#if notice}<p
			class="success-notice"
			role="status"
		>
			{notice}
		</p>{/if}
	{#if section === 'roles'}
		<div class="role-list-heading">
			<header>
				<div>
					<h2>Workspace roles</h2>
					<p>Choose a role to review and change its permissions.</p>
				</div>
				<a class="button primary" href="/roles/?section=role-editor">Create role</a>
			</header>
		</div>
		<CursorList
			resource="roles"
			title="Workspace roles"
			dateLabel="Created date"
			{refresh}
			columns={[
				{ key: 'name', label: 'Role' },
				{
					key: 'permissions',
					label: 'Access',
					format: (v) =>
						`${v.length} permissions: ${[...new Set(v.map((p: string) => p.split('.')[0]))].join(', ')}`
				},
				{ key: 'date', label: 'Created' }
			]}
		>
			{#snippet actions(role)}{#if role.name !== 'Admin'}<RowActions
						href={`/roles/?section=role-editor&edit=${role.id}`}
						label={role.name}
						primaryIcon="edit"
						primaryLabel={`Edit ${role.name} permissions`}
						primaryTitle="Edit permissions"
					/>{:else}<span class="muted">Protected</span>{/if}{/snippet}
		</CursorList>
	{:else if section === 'role-editor'}
		<form onsubmit={create} class="role-editor">
			<header>
				<div>
					<h2>{editing ? 'Edit role' : 'Create custom role'}</h2>
					<p>Select the actions this role should be allowed to perform.</p>
				</div>
				<span class="selection-count">{selected.length} / {data.permissions.length} selected</span>
			</header>
			<label class="role-name-field"
				>Role name<input
					bind:value={name}
					required
					maxlength="50"
					placeholder="For example, Warehouse team"
				/></label
			>
			<div class="permission-groups">
				{#each permissionGroups as group}<fieldset class="permission-group">
						<legend>{group.name}</legend>{#each group.permissions as permission}<label
								class="check-row"
								><input type="checkbox" bind:group={selected} value={permission} /><span
									>{permissionLabels[permission] || permission}</span
								></label
							>{/each}
					</fieldset>{/each}
			</div>
			<div class="form-actions">
				<button class="button primary" disabled={busy}>{busy ? 'Saving?' : 'Save role'}</button><a
					class="button secondary"
					href="/roles/">Cancel</a
				>
			</div>
		</form>
	{:else if section === 'assignments'}
		<form id="assignment-form" onsubmit={assign} class="assignment-form">
			<h2>Assign user role</h2>
			<p>Choose a user and update their workspace access.</p>
			<div class="fields">
				<label
					>User<select bind:value={userId} onchange={selectUser} required
						><option value={0}>Choose user</option>{#each users as u}<option value={u.id}
								>{u.username}</option
							>{/each}</select
					></label
				><label
					>Role<select bind:value={roleId} required
						><option value={0}>Choose role</option>{#each data.roles as r}<option value={r.id}
								>{r.name}</option
							>{/each}</select
					></label
				>
			</div>
			<label class="check-row account-active"
				><input type="checkbox" bind:checked={active} /><span
					><strong>Active account</strong><small>Allow this user to sign in to the workspace.</small
					></span
				></label
			>
			<p class="muted">Saving access changes signs this user out of their existing sessions.</p>
			<div class="form-actions">
				<button class="button primary" disabled={busy || !roleId || !userId}>Save assignment</button
				>
			</div>
		</form>
		<h2>Workspace users</h2>
		<CursorList
			resource="users"
			title="Workspace users"
			dateLabel="Created date"
			{refresh}
			statuses={['Active', 'Inactive']}
			columns={[
				{ key: 'username', label: 'Username' },
				{ key: 'role_name', label: 'Role' },
				{ key: 'status', label: 'Status', type: 'badge' },
				{ key: 'date', label: 'Created' }
			]}
		>
			{#snippet actions(u)}<RowActions
					label={u.username}
					items={[
						{
							label: 'Manage access',
							icon: 'edit',
							run: () => {
								userId = u.id;
								selectUser();
								document.getElementById('assignment-form')?.scrollIntoView({ block: 'start' });
							}
						}
					]}
				/>{/snippet}
		</CursorList>
	{:else}
		<form onsubmit={createUser}>
			<h2>Create user</h2>
			<p>Give a new team member an account and a workspace role.</p>
			<div class="fields">
				<label
					>Username<input
						bind:value={newUsername}
						autocomplete="off"
						required
						maxlength="100"
					/></label
				><label
					>Password<input
						type="password"
						bind:value={newPassword}
						autocomplete="new-password"
						required
						minlength="10"
					/><small>Use at least 10 characters.</small></label
				><label
					>Role<select bind:value={newRole} required
						><option value={0}>Choose role</option>{#each data.roles as r}<option value={r.id}
								>{r.name}</option
							>{/each}</select
					></label
				>
			</div>
			<div class="form-actions">
				<button class="button primary" disabled={busy || !newRole}>Create user</button>
			</div>
		</form>
	{/if}
</main>
