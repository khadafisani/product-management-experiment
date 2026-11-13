<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api, today, type Document } from '../../lib/api';
	import { user } from '../../lib/auth';
	type Order = {
		id: number;
		sales_invoice_id: number;
		driver_name: string;
		date: string;
		address: string;
		note: string;
		cancellation_note: string;
		status: string;
	};
	type InvoiceLine = {
		id: number;
		item_name: string;
		product_id: number;
		quantity: number;
		delivered_quantity: number;
		on_delivery_quantity: number;
		selected: number;
		notes: string;
	};
	let orders = $state<Order[]>([]),
		invoices = $state<Document[]>([]),
		lines = $state<InvoiceLine[]>([]);
	let error = $state(''),
		notice = $state(''),
		busy = $state(false),
		editing = $state<number | null>(null),
		showForm = $state(false);
	let draft = $state({
		sales_invoice_id: 0,
		driver_name: '',
		date: today(),
		address: '',
		note: ''
	});
	const canWrite = $derived($user?.permissions.includes('delivery.write'));
	const canApprove = $derived($user?.permissions.includes('delivery.approve'));
	async function load() {
		try {
			[orders, invoices] = await Promise.all([
				api<Order[]>('/delivery-orders'),
				api<Document[]>('/sales-invoices')
			]);
		} catch (e) {
			error = (e as Error).message;
		}
	}
	onMount(load);
	async function selectInvoice() {
		try {
			const d = await api<any>(`/documents/${draft.sales_invoice_id}`);
			lines = d.lines.map((l: any) => ({ ...l, selected: 0, notes: '' }));
		} catch (e) {
			error = (e as Error).message;
		}
	}
	function create() {
		editing = null;
		draft = { sales_invoice_id: 0, driver_name: '', date: today(), address: '', note: '' };
		lines = [];
		showForm = true;
		error = '';
	}
	async function edit(order: Order) {
		editing = order.id;
		draft = {
			sales_invoice_id: order.sales_invoice_id,
			driver_name: order.driver_name,
			date: order.date,
			address: order.address,
			note: order.note
		};
		await selectInvoice();
		const d = await api<any>(`/delivery-orders/${order.id}`);
		for (const l of lines) {
			const x = d.details.find((x: any) => x.sales_invoice_detail_id === l.id);
			if (x) {
				l.selected = x.quantity;
				l.notes = x.notes;
			}
		}
		showForm = true;
	}
	async function save(e: SubmitEvent) {
		e.preventDefault();
		if (busy) return;
		busy = true;
		error = '';
		try {
			await api(editing ? `/delivery-orders/${editing}` : '/delivery-orders', {
				...draft,
				details: lines
					.filter((l) => l.selected > 0)
					.map((l) => ({ sales_invoice_detail_id: l.id, quantity: l.selected, notes: l.notes }))
			});
			showForm = false;
			notice = 'Delivery order saved as Draft.';
			await load();
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
	async function change(o: Order, status: string) {
		if (busy) return;
		let note = '';
		if (status === 'Cancelled') {
			note = prompt('Cancellation note') ?? '';
			if (!note.trim()) return;
		}
		busy = true;
		error = '';
		try {
			const requestCancellation = status === 'Cancelled' && o.status !== 'Draft' && !canApprove;
			await api(
				requestCancellation
					? `/delivery-orders/${o.id}/cancel-request`
					: status === 'Approved'
						? `/delivery-orders/${o.id}/approve`
						: `/delivery-orders/${o.id}/status`,
				requestCancellation
					? { reason: note }
					: status === 'Approved'
						? {}
						: { status, cancellation_note: note }
			);
			notice = requestCancellation
				? 'Cancellation request sent to Finance.'
				: `Delivery #${o.id}: ${status}`;
			await load();
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
	function view(id: number) {
		void goto(`/delivery-orders/${id}/`);
	}
</script>

<main class="feature-page">
	<header>
		<div>
			<h1>Delivery orders</h1>
			<p>Finance approval reserves the remaining sales invoice quantities.</p>
		</div>
		{#if canWrite}<button onclick={create}>New delivery order</button>{/if}
	</header>
	{#if error}<p class="error" role="alert">{error}</p>{/if}{#if notice}<p role="status">
			{notice}
		</p>{/if}
	{#if showForm}<form onsubmit={save}>
			<h2>{editing ? 'Edit draft' : 'Create delivery order'}</h2>
			<div class="fields">
				<label
					>Sales invoice<select
						bind:value={draft.sales_invoice_id}
						onchange={selectInvoice}
						disabled={editing !== null}
						required
						><option value={0}>Choose a paid invoice</option
						>{#each invoices.filter((i) => i.status === 'Paid' || i.status === 'Partial Paid') as i}<option
								value={i.id}>{i.number} · {i.person} · {i.status}</option
							>{/each}</select
					></label
				><label>Driver name<input bind:value={draft.driver_name} maxlength="200" required /></label
				><label>Date<input type="date" bind:value={draft.date} required /></label><label
					>Address<input bind:value={draft.address} maxlength="1000" required /></label
				><label>Note<textarea bind:value={draft.note}></textarea></label>
			</div>
			<table>
				<thead
					><tr
						><th>Product</th><th>Invoice qty</th><th>Delivered</th><th>Reserved / on delivery</th
						><th>Delivery qty</th><th>Notes</th></tr
					></thead
				><tbody
					>{#each lines as l}<tr
							><td><a class="record-link" href={`/products/${l.product_id}/`}>{l.item_name}</a></td
							><td>{l.quantity}</td><td>{l.delivered_quantity}</td><td>{l.on_delivery_quantity}</td
							><td
								><input
									type="number"
									min="0"
									max={l.quantity - l.delivered_quantity - l.on_delivery_quantity}
									step="1"
									bind:value={l.selected}
								/></td
							><td><input bind:value={l.notes} /></td></tr
						>{/each}</tbody
				>
			</table>
			<p>Enter a positive quantity for each product to include.</p>
			<button disabled={busy || !draft.sales_invoice_id}>Save draft</button><button
				type="button"
				onclick={() => (showForm = false)}>Close</button
			>
		</form>{/if}
	<table>
		<thead
			><tr
				><th>ID</th><th>Invoice</th><th>Driver / date</th><th>Address</th><th>Status</th><th
					>Actions</th
				></tr
			></thead
		><tbody
			>{#each orders as o}<tr
					><td><a class="record-link" href={`/delivery-orders/${o.id}/`}>Delivery #{o.id}</a></td
					><td
						><a class="record-link" href={`/documents/${o.sales_invoice_id}/`}
							>{invoices.find((i) => i.id === o.sales_invoice_id)?.number ?? o.sales_invoice_id}</a
						></td
					><td>{o.driver_name}<br />{o.date}</td><td>{o.address}</td><td>{o.status}</td><td
						><button onclick={() => view(o.id)}>Details</button
						>{#if o.status === 'Draft' && canWrite}<button disabled={busy} onclick={() => edit(o)}
								>Edit</button
							>{/if}{#if o.status === 'Draft' && canApprove}<button
								disabled={busy}
								onclick={() => change(o, 'Approved')}>Approve</button
							>{/if}{#if o.status === 'Approved' && canWrite}<button
								disabled={busy}
								onclick={() => change(o, 'On Delivery')}>On delivery</button
							>{/if}{#if ['Approved', 'On Delivery'].includes(o.status) && canWrite}<button
								disabled={busy}
								onclick={() => change(o, 'Delivered')}>Delivered</button
							>{/if}{#if (o.status === 'Draft' && canWrite) || (['Approved', 'On Delivery'].includes(o.status) && (canWrite || canApprove))}<button
								disabled={busy}
								onclick={() => change(o, 'Cancelled')}
								>{o.status !== 'Draft' && !canApprove ? 'Request cancellation' : 'Cancel'}</button
							>{/if}</td
					></tr
				>{:else}<tr><td colspan="6">No delivery orders yet.</td></tr>{/each}</tbody
		>
	</table>
</main>
