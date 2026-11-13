<script lang="ts">
	import { onDestroy } from 'svelte';
	import { api, money } from './api';
	import { user, detailNavigation } from './auth';
	import RelatedRecords from './RelatedRecords.svelte';
	import RecordTimestamps from './RecordTimestamps.svelte';
	let { category, id }: { category: 'document' | 'payment' | 'product' | 'delivery'; id: string } =
		$props();
	let data = $state<any>(null),
		loading = $state(true),
		error = $state(''),
		notice = $state(''),
		busy = $state(false),
		editing = $state(false),
		showVoid = $state(false),
		reason = $state('');
	let draft = $state({ number: '', date: '', person: '', editLines: false, lines: [] as any[] });
	let deliveryDraft = $state({
		driver_name: '',
		date: '',
		address: '',
		note: '',
		details: [] as any[]
	});
	let generation = 0;
	const labels: Record<string, string> = {
		purchase_order: 'Purchase order',
		purchase_invoice: 'Purchase invoice',
		sales_invoice: 'Sales invoice'
	};
	const views: Record<string, string> = {
		purchase_order: 'Purchase orders',
		purchase_invoice: 'Purchase invoices',
		sales_invoice: 'Sales invoices'
	};
	let heading = $derived(
		category === 'document'
			? data?.document.number
			: category === 'payment'
				? data?.payment.reference
				: category === 'product'
					? data?.product.name
					: `DO-${String(id).padStart(6, '0')}`
	);
	let recordType = $derived(
		category === 'document'
			? labels[data?.document.kind] || 'Document'
			: category === 'payment'
				? 'Payment'
				: category === 'product'
					? 'Product'
					: 'Delivery order'
	);
	let back = $derived(
		category === 'document'
			? `/?view=${encodeURIComponent(views[data?.document.kind] || 'Dashboard')}`
			: category === 'delivery'
				? '/delivery-orders/'
				: category === 'payment'
					? '/?view=Payments'
					: '/?view=Products'
	);
	async function load(recordId: string) {
		const ticket = ++generation;
		loading = true;
		error = '';
		data = null;
		editing = false;
		showVoid = false;
		try {
			if (!/^[1-9][0-9]*$/.test(recordId)) throw Error('Record not found');
			const path =
				category === 'document'
					? 'documents'
					: category === 'payment'
						? 'payments'
						: category === 'product'
							? 'products'
							: 'delivery-orders';
			const result = await api<any>(`/${path}/${recordId}`);
			if (ticket !== generation) return;
			data = result;
			detailNavigation.set(
				category === 'document' ? views[result.document.kind] || 'Record details' : ''
			);
		} catch (e) {
			if (ticket === generation) error = (e as Error).message;
		} finally {
			if (ticket === generation) loading = false;
		}
	}
	$effect(() => {
		void load(id);
	});
	onDestroy(() => {
		generation++;
		detailNavigation.set('');
	});
	function edit() {
		showVoid = false;
		editing = true;
		if (category === 'document')
			draft = {
				number: data.document.number,
				date: data.document.date,
				person: data.document.person,
				editLines: false,
				lines: data.lines.map((l: any) => ({ ...l }))
			};
		else
			deliveryDraft = {
				driver_name: data.order.driver_name,
				date: data.order.date,
				address: data.order.address,
				note: data.order.note,
				details: data.details.map((l: any) => ({ ...l }))
			};
	}
	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (busy) return;
		busy = true;
		try {
			if (category === 'document')
				await api(`/invoices/${id}`, {
					number: draft.number,
					date: draft.date,
					person: draft.person,
					...(draft.editLines
						? {
								lines: draft.lines.map((l) => ({
									product_id: l.product_id,
									quantity: l.quantity,
									price: l.price
								}))
							}
						: {})
				});
			else
				await api(`/delivery-orders/${id}`, {
					...deliveryDraft,
					sales_invoice_id: data.order.sales_invoice_id,
					details: deliveryDraft.details.map((l) => ({
						sales_invoice_detail_id: l.sales_invoice_detail_id,
						quantity: l.quantity,
						notes: l.notes
					}))
				});
			notice = 'Changes saved.';
			await load(id);
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
	async function voidInvoice(event: SubmitEvent) {
		event.preventDefault();
		if (busy) return;
		busy = true;
		try {
			const result = await api<any>(`/invoices/${id}/cancel`, { reason });
			notice = result.status === 'Pending' ? 'Void request sent to Finance.' : 'Invoice voided.';
			await load(id);
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
	async function transition(status: string) {
		if (busy) return;
		let note = '';
		if (status === 'Cancelled') {
			note = prompt('Cancellation reason') || '';
			if (!note.trim()) return;
		}
		busy = true;
		try {
			const request =
				status === 'Cancelled' &&
				data.order.status !== 'Draft' &&
				!$user?.permissions.includes('delivery.approve');
			await api(
				request
					? `/delivery-orders/${id}/cancel-request`
					: status === 'Approved'
						? `/delivery-orders/${id}/approve`
						: `/delivery-orders/${id}/status`,
				request
					? { reason: note }
					: status === 'Approved'
						? {}
						: { status, cancellation_note: note }
			);
			notice = request ? 'Cancellation request sent to Finance.' : `Delivery status: ${status}`;
			await load(id);
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>{heading || recordType} · Stockroom</title></svelte:head>
<main class="feature-page detail-page">
	<a class="detail-back" href={back}
		>← Back to {category === 'document'
			? views[data?.document.kind] || 'workspace'
			: category === 'delivery'
				? 'delivery orders'
				: category === 'payment'
					? 'payments'
					: 'products'}</a
	>
	{#if error}<div class="detail-error" role="alert">
			{error}<button class="button secondary" onclick={() => load(id)}>Retry</button>
		</div>{/if}{#if notice}<p class="success-notice" role="status">{notice}</p>{/if}
	{#if loading}<p role="status">Loading record…</p>{:else if data}
		<header class="page-heading">
			<div>
				<div class="eyebrow">{recordType}</div>
				<h1>{heading}</h1>
				<p>
					{category === 'document'
						? data.document.person
						: category === 'payment'
							? 'Payment allocations and related invoices'
							: category === 'product'
								? 'Inventory and recent related transactions'
								: data.order.driver_name}
				</p>
			</div>
			<div class="detail-actions">
				{#if category === 'document' && data.document.kind !== 'purchase_order' && data.document.status !== 'Cancelled'}{#if $user?.permissions.includes('invoices.write')}<button
							class="button action-edit"
							onclick={edit}>Edit invoice</button
						>{/if}{#if $user?.permissions.includes('invoices.request_void')}<button
							class="button action-void"
							onclick={() => {
								showVoid = true;
								editing = false;
								reason = '';
							}}
							>{$user?.permissions.includes('invoices.void')
								? 'Void invoice'
								: 'Request void'}</button
						>{/if}{/if}
				{#if category === 'delivery'}{#if data.order.status === 'Draft'}{#if $user?.permissions.includes('delivery.write')}<button
								class="button secondary"
								onclick={edit}>Edit draft</button
							>{/if}{#if $user?.permissions.includes('delivery.approve')}<button
								class="button primary"
								disabled={busy}
								onclick={() => transition('Approved')}>Approve</button
							>{/if}{/if}{#if ['Approved', 'On Delivery'].includes(data.order.status) && $user?.permissions.includes('delivery.write')}{#if data.order.status === 'Approved'}<button
								class="button secondary"
								disabled={busy}
								onclick={() => transition('On Delivery')}>On delivery</button
							>{/if}<button
							class="button primary"
							disabled={busy}
							onclick={() => transition('Delivered')}>Delivered</button
						>{/if}{#if ['Draft', 'Approved', 'On Delivery'].includes(data.order.status) && $user?.permissions.includes('delivery.write')}<button
							class="button secondary"
							disabled={busy}
							onclick={() => transition('Cancelled')}
							>{data.order.status === 'Draft' || $user?.permissions.includes('delivery.approve')
								? 'Cancel delivery'
								: 'Request cancellation'}</button
						>{/if}{/if}
			</div>
		</header>
		<RecordTimestamps record={data.document ?? data.payment ?? data.product ?? data.order} />
		{#if editing}<form onsubmit={save} class="detail-edit">
				<h2>{category === 'document' ? 'Edit invoice' : 'Edit delivery draft'}</h2>
				<div class="fields">
					{#if category === 'document'}<label
							>Number<input bind:value={draft.number} required maxlength="100" /></label
						><label>Date<input type="date" bind:value={draft.date} required /></label><label
							>Customer / supplier<input
								bind:value={draft.person}
								required
								maxlength="200"
							/></label
						>{:else}<label
							>Driver<input
								bind:value={deliveryDraft.driver_name}
								required
								maxlength="200"
							/></label
						><label>Date<input type="date" bind:value={deliveryDraft.date} required /></label><label
							>Address<input bind:value={deliveryDraft.address} required maxlength="1000" /></label
						><label>Note<textarea bind:value={deliveryDraft.note}></textarea></label>{/if}
				</div>
				{#if category === 'document'}{#if data.payments.length === 0 && data.delivery_orders.length === 0}<label
							class="inline-check"
							><input type="checkbox" bind:checked={draft.editLines} />Edit quantities and prices</label
						>{/if}{#if draft.editLines}{#each draft.lines as l}<fieldset>
								<legend
									><a class="record-link" href={`/products/${l.product_id}/`}>{l.item_name}</a
									></legend
								>
								<div class="fields">
									<label
										>Quantity<input
											type="number"
											min="1"
											step="1"
											bind:value={l.quantity}
											required
										/></label
									><label
										>Price<input
											type="number"
											min="0"
											step="1"
											bind:value={l.price}
											required
										/></label
									>
								</div>
							</fieldset>{/each}{/if}{:else}{#each deliveryDraft.details as l}<fieldset>
							<legend
								><a class="record-link" href={`/products/${l.product_id}/`}>{l.product_name}</a
								></legend
							>
							<div class="fields">
								<label
									>Quantity<input
										type="number"
										min="1"
										step="1"
										bind:value={l.quantity}
										required
									/></label
								><label>Notes<input bind:value={l.notes} /></label>
							</div>
						</fieldset>{/each}{/if}
				<div class="form-actions">
					<button class="button primary" disabled={busy}>Save changes</button><button
						type="button"
						class="button secondary"
						onclick={() => (editing = false)}>Cancel edit</button
					>
				</div>
			</form>{/if}
		{#if showVoid}<form onsubmit={voidInvoice}>
				<h2>
					{$user?.permissions.includes('invoices.void') ? 'Void invoice' : 'Request invoice void'}
				</h2>
				<p>
					Invoice and payment history will be retained. Staff requests require Finance approval.
				</p>
				<label>Reason<textarea bind:value={reason} maxlength="500" required></textarea></label
				><button class="button action-void-confirm" disabled={busy}
					>{$user?.permissions.includes('invoices.void')
						? 'Void invoice'
						: 'Submit void request'}</button
				><button type="button" class="button secondary" onclick={() => (showVoid = false)}
					>Keep invoice</button
				>
			</form>{/if}
		{#if category === 'document'}
			<div class="detail-summary">
				<div><small>Date</small><strong>{data.document.date}</strong></div>
				<div><small>Status</small><strong>{data.document.status}</strong></div>
				<div><small>Grand total</small><strong>{money(data.document.grand_total)}</strong></div>
				{#if data.document.kind !== 'purchase_order'}<div>
						<small>Outstanding</small><strong>{money(data.document.remaining_payment)}</strong>
					</div>{:else}<div>
						<small>Total quantity</small><strong>{data.total_quantity}</strong>
					</div>{/if}
			</div>

			<section class="related-panel">
				<div class="related-heading">
					<h2>Details</h2>
					<span
						>{data.total_product}
						{data.total_product === 1 ? 'line item' : 'line items'} &middot; {data.total_quantity} units</span
					>
				</div>
				<div class="table-wrap">
					<table>
						<thead
							><tr
								><th>Product</th><th>Quantity</th><th>Price</th
								>{#if data.document.kind === 'purchase_order'}<th>Remaining to receive</th
									>{/if}{#if data.document.kind === 'sales_invoice'}<th>Reserved / on delivery</th
									><th>Delivered</th>{/if}<th>Subtotal</th></tr
							></thead
						><tbody
							>{#each data.lines as l}<tr
									><td
										><a class="record-link" href={`/products/${l.product_id}/`}>{l.item_name}</a
										></td
									><td>{l.quantity}</td><td>{money(l.price)}</td
									>{#if data.document.kind === 'purchase_order'}<td>{l.realized_quantity}</td
										>{/if}{#if data.document.kind === 'sales_invoice'}<td
											>{l.on_delivery_quantity}</td
										><td>{l.delivered_quantity}</td>{/if}<td>{money(l.subtotal)}</td></tr
								>{/each}</tbody
						>
					</table>
				</div>
			</section>
			{#if data.purchase_order}<section class="related-panel purchase-order-breakdown">
					<div class="related-heading">
						<div>
							<h2>Purchase order</h2>
							<p class="section-description">
								The source order and its current receiving progress.
							</p>
						</div>
						<a class="purchase-order-reference" href={`/documents/${data.purchase_order.id}/`}
							><span>PO</span>{data.purchase_order.number}<span aria-hidden="true">&#8599;</span></a
						>
					</div>
					<dl class="purchase-order-meta">
						<div>
							<dt>Order date</dt>
							<dd>{data.purchase_order.date}</dd>
						</div>
						<div>
							<dt>Person in charge</dt>
							<dd>{data.purchase_order.person}</dd>
						</div>
						<div>
							<dt>Status</dt>
							<dd><span class="badge">{data.purchase_order.status}</span></dd>
						</div>
						<div>
							<dt>Order total</dt>
							<dd>{money(data.purchase_order.grand_total)}</dd>
						</div>
					</dl>
					<div class="table-wrap">
						<table>
							<thead
								><tr
									><th>Product</th><th>Ordered</th><th>Received to date</th><th>This invoice</th><th
										>Remaining to receive</th
									><th>Order unit price</th><th>Order subtotal</th></tr
								></thead
							><tbody
								>{#each data.purchase_order_lines as line}<tr
										><td
											><a class="record-link" href={`/products/${line.product_id}/`}
												>{line.item_name}</a
											></td
										><td>{line.ordered_quantity}</td><td>{line.received_quantity}</td><td
											>{line.invoice_quantity}</td
										><td
											>{#if line.remaining_quantity < 0}<span class="over-received"
													>{Math.abs(line.remaining_quantity)} over received</span
												>{:else}{line.remaining_quantity}{/if}</td
										><td>{money(line.price)}</td><td>{money(line.subtotal)}</td></tr
									>{/each}</tbody
							>
						</table>
					</div>
					<p class="receiving-note">
						Received to date includes all active invoices linked to this purchase order.{#if data.document.status === 'Cancelled'}
							This invoice is cancelled and does not contribute to received quantities.{/if}
					</p>
				</section>{:else if data.document.kind === 'purchase_invoice'}<section
					class="related-panel"
				>
					<h2>Purchase order</h2>
					<p class="related-empty">Standalone invoice. No purchase order is linked.</p>
				</section>{/if}
			{#if data.document.kind === 'purchase_order'}<RelatedRecords
					title="Purchase invoices"
					records={data.purchase_invoices}
				/>{:else}{#if data.access.payments}<RelatedRecords
						title="Payments"
						records={data.payments}
						type="payment"
					/>{:else}<p class="related-empty">
						Your role cannot view related payments.
					</p>{/if}{#if data.document.kind === 'sales_invoice'}{#if data.access.delivery}<RelatedRecords
							title="Delivery orders"
							records={data.delivery_orders}
							type="delivery"
						/>{:else}<p class="related-empty">
							Your role cannot view related delivery orders.
						</p>{/if}{/if}{/if}
			{#if data.cancellation}<section class="related-panel">
					<h2>Cancellation</h2>
					<p>{data.cancellation.reason}</p>
					<p>Refund due: <strong>{money(data.cancellation.refund_due)}</strong></p>
				</section>{/if}
		{:else if category === 'payment'}
			<div class="detail-summary">
				<div><small>Date</small><strong>{data.payment.date}</strong></div>
				<div><small>Amount</small><strong>{money(data.payment.amount)}</strong></div>
				<div>
					<small>Type</small><strong
						>{data.payment.kind === 'sales_invoice'
							? 'Customer receipt'
							: 'Supplier payment'}</strong
					>
				</div>
				<div>
					<small>Allocated invoices</small><strong
						>{data.access.invoices ? data.invoices.length : 'Restricted'}</strong
					>
				</div>
			</div>
			{#if data.access.invoices}<RelatedRecords
					title="Invoice allocations"
					records={data.invoices}
				/>{:else}<p>Your role cannot view related invoices.</p>{/if}
		{:else if category === 'delivery'}
			<div class="detail-summary">
				<div><small>Date</small><strong>{data.order.date}</strong></div>
				<div><small>Status</small><strong>{data.order.status}</strong></div>
				<div><small>Driver</small><strong>{data.order.driver_name}</strong></div>
				<div><small>Address</small><strong>{data.order.address}</strong></div>
			</div>
			{#if data.sales_invoice}<section class="related-panel">
					<h2>Sales invoice</h2>
					<a class="record-link" href={`/documents/${data.sales_invoice.id}/`}
						>{data.sales_invoice.number}</a
					>
					<p>{data.sales_invoice.person} · {data.sales_invoice.status}</p>
				</section>{:else if !data.access.invoices}<p>
					Your role cannot view the related invoice.
				</p>{/if}
			<section class="related-panel">
				<h2>Delivery details</h2>
				<div class="table-wrap">
					<table>
						<thead><tr><th>Product</th><th>Quantity</th><th>Notes</th></tr></thead><tbody
							>{#each data.details as l}<tr
									><td
										><a class="record-link" href={`/products/${l.product_id}/`}>{l.product_name}</a
										></td
									><td>{l.quantity}</td><td>{l.notes || '—'}</td></tr
								>{/each}</tbody
						>
					</table>
				</div>
				<p>{data.order.note || 'No delivery note.'}</p>
				{#if data.order.cancellation_note}<p>Cancellation: {data.order.cancellation_note}</p>{/if}
			</section>
		{:else}
			<div class="product-detail-heading">
				{#if data.product.photo}<img src={data.product.photo} alt={data.product.name} />{/if}
				<p>{data.product.description || 'No product description.'}</p>
			</div>
			<div class="detail-summary">
				<div><small>Physical stock</small><strong>{data.product.quantity} units</strong></div>
				<div>
					<small>Available stock</small><strong>{data.product.remaining_quantity} units</strong>
				</div>
				<div><small>Unit price</small><strong>{money(data.product.price)}</strong></div>
				<div>
					<small>Status / reorder level</small><strong
						>{data.product.status} · {data.product.reorder_level} units</strong
					>
				</div>
			</div>
			<p class="muted">Showing the latest {data.recent_limit} records per type, ordered by date.</p>
			{#if data.access.invoices}<RelatedRecords
					title="Latest purchase orders"
					records={data.purchase_orders}
				/><RelatedRecords
					title="Latest purchase invoices"
					records={data.purchase_invoices}
				/><RelatedRecords title="Latest sales invoices" records={data.sales_invoices} />{:else}<p>
					Your role cannot view related orders or invoices.
				</p>{/if}{#if data.access.delivery}<RelatedRecords
					title="Latest delivery orders"
					records={data.delivery_orders}
					type="delivery"
				/>{:else}<p>Your role cannot view related deliveries.</p>{/if}
		{/if}{/if}
</main>
