<script lang="ts">
	import { page } from '$app/state';
	import { api, type Lifecycle } from '../../../../lib/api';
	import { user } from '../../../../lib/auth';
	import RecordTimestamps from '../../../../lib/RecordTimestamps.svelte';
	type Request = Lifecycle & {
		record_id: number;
		record_number: string;
		date: string;
		reason: string;
		status: string;
		requested_by: string;
		reviewed_by: string | null;
	};
	let data = $state<Request | null>(null),
		error = $state(''),
		busy = $state(false),
		loading = $state(true);
	let delivery = $derived(page.params.kind === 'deliveries');
	let endpoint = $derived(delivery ? 'delivery-cancellation-requests' : 'void-requests');
	let generation = 0;
	async function load(kind: string | undefined, id: string | undefined) {
		const ticket = ++generation;
		loading = true;
		data = null;
		error = '';
		try {
			if (!['invoices', 'deliveries'].includes(kind ?? '') || !/^[1-9][0-9]*$/.test(id ?? ''))
				throw Error('Request not found');
			const result = await api<Request>(
				`/${kind === 'deliveries' ? 'delivery-cancellation-requests' : 'void-requests'}/${id}`
			);
			if (ticket === generation) data = result;
		} catch (e) {
			if (ticket === generation) error = (e as Error).message;
		} finally {
			if (ticket === generation) loading = false;
		}
	}
	$effect(() => {
		void load(page.params.kind, page.params.id);
	});
	async function review(action: string) {
		if (busy) return;
		busy = true;
		error = '';
		try {
			await api(`/${endpoint}/${page.params.id}/${action}`, {});
			await load(page.params.kind, page.params.id);
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Approval details · Stockroom</title></svelte:head>
<main class="feature-page detail-page">
	<a class="detail-back" href={`/approvals/?section=${delivery ? 'deliveries' : 'invoices'}`}
		>← Back to approvals</a
	>
	{#if error}<p role="alert" class="error">{error}</p>{/if}
	{#if loading}<p role="status">Loading request…</p>{:else if data}
		<header class="page-heading">
			<div>
				<div class="eyebrow">{delivery ? 'Delivery cancellation' : 'Invoice void'} request</div>
				<h1>{data.record_number}</h1>
				<p>{data.status}</p>
			</div>
			{#if data.status === 'Pending' && $user?.permissions.includes(delivery ? 'delivery.approve' : 'invoices.void')}
				<div class="detail-actions">
					<button class="button primary" disabled={busy} onclick={() => review('approve')}
						>Approve {delivery ? 'cancellation' : 'void'}</button
					><button class="button action-void" disabled={busy} onclick={() => review('reject')}
						>Reject request</button
					>
				</div>
			{/if}
		</header>
		<RecordTimestamps record={data} />
		<section class="request-details" aria-label="Request details">
			<h2>Request details</h2>
			<dl>
				<div>
					<dt>{delivery ? 'Delivery order' : 'Invoice'}</dt>
					<dd>
						<a
							class="record-link"
							href={`/${delivery ? 'delivery-orders' : 'documents'}/${data.record_id}/`}
							>{data.record_number}</a
						>
					</dd>
				</div>
				<div>
					<dt>{delivery ? 'Delivery' : 'Invoice'} date</dt>
					<dd>{data.date}</dd>
				</div>
				<div>
					<dt>Requested by</dt>
					<dd>{data.requested_by}</dd>
				</div>
				<div>
					<dt>Reviewed by</dt>
					<dd>{data.reviewed_by ?? 'Awaiting review'}</dd>
				</div>
				<div class="reason">
					<dt>Reason</dt>
					<dd>{data.reason}</dd>
				</div>
			</dl>
		</section>
	{/if}
</main>

<style>
	.request-details {
		padding: 1.5rem;
		background: white;
		border: 1px solid #dfe7e2;
		border-radius: 1rem;
	}
	dl {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: 1.5rem;
	}
	dt {
		color: #65796e;
		font-size: 0.85rem;
		margin-bottom: 0.5rem;
	}
	dd {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.reason {
		grid-column: 1 / -1;
		white-space: pre-wrap;
	}
</style>
