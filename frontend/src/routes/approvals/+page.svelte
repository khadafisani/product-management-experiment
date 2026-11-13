<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '../../lib/api';
	import { user } from '../../lib/auth';
	let cancellations = $state<any[]>([]);
	let requests = $state<any[]>([]),
		error = $state(''),
		busy = $state(false);
	async function load() {
		try {
			[requests, cancellations] = await Promise.all([
				api<any[]>('/void-requests'),
				api<any[]>('/delivery-cancellation-requests')
			]);
		} catch (e) {
			error = (e as Error).message;
		}
	}
	onMount(load);
	async function review(id: number, action: string, delivery = false) {
		busy = true;
		error = '';
		try {
			await api(
				`/${delivery ? 'delivery-cancellation-requests' : 'void-requests'}/${id}/${action}`,
				{}
			);
			await load();
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<main class="feature-page">
	<h1>Invoice void requests</h1>
	<p>Staff requests require Finance approval. Invoice and payment history are retained.</p>
	{#if error}<p role="alert" class="error">{error}</p>{/if}
	<table>
		<thead
			><tr
				><th>Request</th><th>Invoice</th><th>Requested by</th><th>Reason</th><th>Status</th><th
					>Actions</th
				></tr
			></thead
		><tbody
			>{#each requests as r}<tr
					><td>#{r.id}</td><td
						><a class="record-link" href={`/documents/${r.invoice_id}/`}>Invoice #{r.invoice_id}</a
						></td
					><td>#{r.requested_by}</td><td>{r.reason}</td><td>{r.status}</td><td
						>{#if r.status === 'Pending' && $user?.permissions.includes('invoices.void')}<button
								disabled={busy}
								onclick={() => review(r.id, 'approve')}>Approve void</button
							><button disabled={busy} onclick={() => review(r.id, 'reject')}>Reject</button
							>{/if}</td
					></tr
				>{:else}<tr><td colspan="6">No requests.</td></tr>{/each}</tbody
		>
	</table>
	<h2>Delivery cancellation requests</h2>
	<table>
		<thead
			><tr
				><th>Request</th><th>Delivery order</th><th>Requested by</th><th>Reason</th><th>Status</th
				><th>Actions</th></tr
			></thead
		><tbody
			>{#each cancellations as r}<tr
					><td>#{r.id}</td><td
						><a class="record-link" href={`/delivery-orders/${r.delivery_order_id}/`}
							>Delivery #{r.delivery_order_id}</a
						></td
					><td>#{r.requested_by}</td><td>{r.reason}</td><td>{r.status}</td><td
						>{#if r.status === 'Pending' && $user?.permissions.includes('delivery.approve')}<button
								disabled={busy}
								onclick={() => review(r.id, 'approve', true)}>Approve cancellation</button
							><button disabled={busy} onclick={() => review(r.id, 'reject', true)}>Reject</button
							>{/if}</td
					></tr
				>{:else}<tr><td colspan="6">No requests.</td></tr>{/each}</tbody
		>
	</table>
</main>
