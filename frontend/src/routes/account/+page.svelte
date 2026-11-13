<script lang="ts">
	import { api } from '../../lib/api';
	import { user } from '../../lib/auth';
	let current = $state(''),
		password = $state(''),
		error = $state(''),
		busy = $state(false);
	async function change(e: SubmitEvent) {
		e.preventDefault();
		if (busy) return;
		busy = true;
		error = '';
		try {
			await api('/auth/password', { current_password: current, new_password: password });
			current = '';
			password = '';
			user.set(null);
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
	async function revoke() {
		busy = true;
		error = '';
		try {
			await api('/auth/logout-all', {});
			user.set(null);
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<main class="feature-page">
	<h1>Account</h1>
	<p>{$user?.username} · {$user?.role}</p>
	{#if error}<p role="alert" class="error">{error}</p>{/if}
	<form onsubmit={change}>
		<h2>Change password</h2>
		<p>Changing your password signs out every session.</p>
		<label
			>Current password<input
				type="password"
				autocomplete="current-password"
				bind:value={current}
				required
			/></label
		><label
			>New password<input
				type="password"
				autocomplete="new-password"
				bind:value={password}
				minlength="10"
				required
			/></label
		><button disabled={busy}>Change password and sign out</button>
	</form>
	<button onclick={revoke} disabled={busy}>Sign out all sessions</button>
</main>
