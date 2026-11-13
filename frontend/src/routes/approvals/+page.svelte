<script lang="ts">
	import { page as route } from '$app/state';
	import CursorList from '../../lib/CursorList.svelte';
	import RowActions from '../../lib/RowActions.svelte';
	import type { Column } from '../../lib/list-config';
	import { api } from '../../lib/api';
	import { user } from '../../lib/auth';
	let section = $derived(
		route.url.searchParams.get('section') === 'deliveries' ? 'deliveries' : 'invoices'
	);
	let delivery = $derived(section === 'deliveries');
	let error = $state(''),
		busy = $state(false),
		refresh = $state(0);
	let columns = $derived<Column[]>([
		{
			key: delivery ? 'delivery_order_number' : 'invoice_number',
			label: delivery ? 'Delivery order' : 'Invoice',
			href: (r) =>
				delivery ? `/delivery-orders/${r.delivery_order_id}/` : `/documents/${r.invoice_id}/`
		},
		{ key: 'date', label: delivery ? 'Delivery date' : 'Invoice date' },
		{ key: 'requested_by', label: 'Requested by' },
		{ key: 'reason', label: 'Reason' },
		{ key: 'status', label: 'Status', type: 'badge' },
		{ key: 'reviewed_by', label: 'Reviewed by' }
	]);
	async function review(id: number, action: string) {
		busy = true;
		error = '';
		try {
			await api(
				`/${delivery ? 'delivery-cancellation-requests' : 'void-requests'}/${id}/${action}`,
				{}
			);
			refresh++;
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Approvals · Stockroom</title></svelte:head>
<main class="feature-page list-page">
	<h1>Approvals</h1>
	<p>Review requests and preserve the history of your invoices and deliveries.</p>
	<nav class="section-tabs" aria-label="Approval sections">
		<a
			href="/approvals/?section=invoices"
			class:active={!delivery}
			aria-current={!delivery ? 'page' : undefined}>Invoice approvals</a
		>
		<a
			href="/approvals/?section=deliveries"
			class:active={delivery}
			aria-current={delivery ? 'page' : undefined}>Delivery cancellations</a
		>
	</nav>
	<h2>{delivery ? 'Delivery cancellation requests' : 'Invoice void requests'}</h2>
	{#if error}<p role="alert" class="error">{error}</p>{/if}
	{#key section}<CursorList
			resource={delivery ? 'delivery-cancellations' : 'invoice-approvals'}
			title={delivery ? 'Delivery cancellations' : 'Invoice approvals'}
			{columns}
			statuses={['Pending', 'Approved', 'Rejected']}
			dateLabel={delivery ? 'Delivery date' : 'Invoice date'}
			{refresh}
		>
			{#snippet actions(r)}<RowActions
					href={`/approvals/${section}/${r.id}/`}
					label={delivery ? r.delivery_order_number : r.invoice_number}
					disabled={busy}
					items={r.status === 'Pending' &&
					$user?.permissions.includes(delivery ? 'delivery.approve' : 'invoices.void')
						? [
								{
									label: delivery ? 'Approve cancellation' : 'Approve void',
									icon: 'check',
									run: () => review(r.id, 'approve')
								},
								{
									label: 'Reject request',
									icon: 'close',
									danger: true,
									run: () => review(r.id, 'reject')
								}
							]
						: []}
				/>{/snippet}
		</CursorList>{/key}
</main>
