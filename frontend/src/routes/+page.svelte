<script lang="ts">
	import { onMount } from 'svelte';
	import { page as route } from '$app/state';
	import { goto } from '$app/navigation';
	import { workspaceConnection, workspaceLowStock } from '../lib/auth';
	import Icon from '../lib/Icon.svelte';
	import { user } from '../lib/auth';
	import {
		api,
		money,
		compact,
		today,
		type Product,
		type Document,
		type Payment,
		type Forecast,
		type Line
	} from '../lib/api';
	let page = $derived(route.url.searchParams.get('view') || 'Dashboard');
	function focusModal(node: HTMLDialogElement) {
		node.showModal();
		return {
			destroy() {
				node.close();
			}
		};
	}
	const kinds: Record<string, string> = {
		'Purchase orders': 'purchase_order',
		'Purchase invoices': 'purchase_invoice',
		'Sales invoices': 'sales_invoice'
	};
	let products = $state<Product[]>([]),
		documents = $state<Document[]>([]),
		payments = $state<Payment[]>([]),
		forecasts = $state<Forecast[]>([]);
	let loading = $state(true),
		connected = $state(false),
		error = $state(''),
		notice = $state(''),
		query = $state(''),
		status = $state('All statuses'),
		busy = $state(false);

	let period = $state('6'),
		horizon = $state(30),
		lead = $state(7),
		modal = $state('');
	let product = $state<Product>({
		id: 0,
		name: '',
		quantity: 0,
		remaining_quantity: 0,
		status: 'Active',
		photo: null,
		description: '',
		price: 0,
		reorder_level: 10
	});
	let draft = $state({
		kind: 'purchase_order',
		number: '',
		date: today(),
		person: '',
		purchase_order_id: null as number | null,
		lines: [{ product_id: 0, quantity: 1, price: 0 }] as Line[]
	});
	let payment = $state({
		reference: '',
		date: today(),
		amount: 0,
		kind: 'purchase_invoice',
		invoice_ids: [] as number[]
	});
	let searchIds = $state<number[] | null>(null),
		searchTimer: ReturnType<typeof setTimeout>;
	let filteredProducts = $derived(
		products.filter(
			(p) =>
				(!query ||
					(searchIds
						? searchIds.includes(p.id)
						: (p.name + ' ' + p.description).toLowerCase().includes(query.toLowerCase()))) &&
				(status === 'All statuses' || p.status === status)
		)
	);
	let filteredDocs = $derived(
		documents.filter(
			(d) =>
				d.kind === kinds[page] &&
				(d.number + ' ' + d.person).toLowerCase().includes(query.toLowerCase()) &&
				(status === 'All statuses' || d.status === status)
		)
	);
	let filteredPayments = $derived(
		payments.filter((p) =>
			(p.reference + ' ' + p.invoices).toLowerCase().includes(query.toLowerCase())
		)
	);
	let activeProducts = $derived(products.filter((p) => p.status === 'Active'));
	let lowStock = $derived(
		products.filter((p) => p.status === 'Active' && p.remaining_quantity <= p.reorder_level)
	);
	let sales = $derived(
		documents.filter((d) => d.kind === 'sales_invoice' && d.status !== 'Cancelled')
	);
	let purchases = $derived(documents.filter((d) => d.kind === 'purchase_invoice'));
	let salesTotal = $derived(sales.reduce((s, d) => s + d.grand_total, 0));
	let purchaseTotal = $derived(purchases.reduce((s, d) => s + d.grand_total, 0));
	let payable = $derived(purchases.reduce((s, d) => s + d.remaining_payment, 0));
	let receivable = $derived(sales.reduce((s, d) => s + d.remaining_payment, 0));
	let months = $derived.by(() => {
		const now = new Date(today() + 'T12:00:00');
		return Array.from({ length: Number(period) }, (_, i) => {
			const d = new Date(now.getFullYear(), now.getMonth() - Number(period) + 1 + i, 1);
			const prefix = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`;
			return {
				name: d.toLocaleDateString('en', { month: 'short' }),
				sales: sales
					.filter((d) => d.date.startsWith(prefix))
					.reduce((s, d) => s + d.grand_total, 0),
				purchases: purchases
					.filter((d) => d.date.startsWith(prefix))
					.reduce((s, d) => s + d.grand_total, 0),
				payments: payments
					.filter((p) => p.date.startsWith(prefix) && p.kind === 'purchase_invoice')
					.reduce((s, p) => s + p.amount, 0)
			};
		});
	});
	let chartMax = $derived(
		Math.max(1, ...months.flatMap((m) => [m.sales, m.purchases, m.payments]))
	);
	let selectedInvoices = $derived(
		payment.invoice_ids
			.map((id) => documents.find((d) => d.id === id))
			.filter((d): d is Document => !!d)
	);
	let allocations = $derived.by(() => {
		let left = payment.amount;
		return selectedInvoices.map((d) => {
			const amount = Math.min(Math.max(0, left), d.remaining_payment);
			left -= amount;
			return { number: d.number, amount, remaining: d.remaining_payment - amount };
		});
	});
	let paymentError = $derived(
		payment.amount <= 0
			? 'Enter a positive amount.'
			: selectedInvoices.length === 0
				? 'Select at least one invoice.'
				: selectedInvoices.length > 1 && payment.amount <= selectedInvoices[0].remaining_payment
					? 'Amount must exceed the first invoice balance to cover multiple invoices.'
					: payment.amount > selectedInvoices.reduce((s, d) => s + d.remaining_payment, 0)
						? 'Amount exceeds the selected balances.'
						: allocations.some((a) => a.amount === 0)
							? 'Remove invoices this payment does not reach.'
							: ''
	);
	let draftTotal = $derived(draft.lines.reduce((s, l) => s + l.quantity * l.price, 0));
	let purchaseReview = $state<{
		draft: typeof draft;
		new_products: {
			name: string;
			price: number;
			quantity: number;
			reorder_level: number;
			status: string;
		}[];
	} | null>(null);
	async function load() {
		loading = true;
		error = '';
		try {
			const [p, d, py, f] = await Promise.all([
				api<Product[]>('/products'),
				api<Document[]>('/documents'),
				api<Payment[]>('/payments'),
				api<Forecast[]>(`/forecast?days=${horizon}&lead_time=${lead}`)
			]);
			products = p;
			documents = d;
			payments = py;
			forecasts = f;
			connected = true;
		} catch (e) {
			error = (e as Error).message;
			connected = false;
		} finally {
			loading = false;
		}
	}
	onMount(() => {
		void load();
		const refresh = () => {
			void load();
		};
		const reset = () => {
			query = '';
			status = 'All statuses';
			searchIds = null;
			error = '';
		};
		window.addEventListener('workspace-refresh', refresh);
		window.addEventListener('workspace-navigate', reset);
		return () => {
			window.removeEventListener('workspace-refresh', refresh);
			window.removeEventListener('workspace-navigate', reset);
		};
	});
	$effect(() => {
		workspaceConnection.set(connected);
		workspaceLowStock.set(lowStock.length);
	});
	function navigate(next: string) {
		void goto(next === 'Dashboard' ? '/' : `/?view=${encodeURIComponent(next)}`);
		query = '';
		status = 'All statuses';
		searchIds = null;
		error = '';
	}
	function purchaseOrderNumber(id: number | null) {
		return documents.find((d) => d.id === id && d.kind === 'purchase_order')?.number;
	}
	function flash(message: string) {
		notice = message;
		setTimeout(() => (notice = ''), 5000);
	}
	function openProduct(p?: Product) {
		if (!$user?.permissions.includes('products.write')) {
			flash('Your role does not have permission for this action.');
			return;
		}
		purchaseReview = null;
		product = p
			? { ...p }
			: {
					id: 0,
					name: '',
					quantity: 0,
					remaining_quantity: 0,
					status: 'Active',
					photo: null,
					description: '',
					price: 0,
					reorder_level: 10
				};
		modal = 'product';
		error = '';
	}
	function openDocument() {
		if (!$user?.permissions.includes('invoices.write')) {
			flash('Your role does not have permission for this action.');
			return;
		}
		purchaseReview = null;
		draft = {
			kind: kinds[page] || 'purchase_order',
			number: '',
			date: today(),
			person: '',
			purchase_order_id: null,
			lines: [{ product_id: 0, quantity: 1, price: 0 }]
		};
		modal = 'document';
		error = '';
	}
	function replenish(p: { id: number; name: string }, quantity?: number) {
		if (!$user?.permissions.includes('invoices.write')) return;
		const existing = products.find((product) => product.id === p.id);
		if (!existing || existing.status !== 'Active') return;
		const prediction = forecasts.find((f) => f.id === p.id);
		navigate('Purchase invoices');
		openDocument();
		draft.kind = 'purchase_invoice';
		draft.number = `PINV-${today().replaceAll('-', '')}-${Date.now().toString().slice(-6)}`;
		draft.lines = [
			{
				product_id: p.id,
				quantity: Math.max(
					1,
					quantity ??
						prediction?.suggested_order ??
						existing.reorder_level - existing.remaining_quantity
				),
				price: existing.price
			}
		];
	}
	function openPayment() {
		if (!$user?.permissions.includes('payments.write')) {
			flash('Your role does not have permission for this action.');
			return;
		}
		purchaseReview = null;
		payment = {
			reference: '',
			date: today(),
			amount: 0,
			kind: 'purchase_invoice',
			invoice_ids: []
		};
		modal = 'payment';
		error = '';
	}
	function create() {
		if (page === 'Products') openProduct();
		else if (page === 'Payments') openPayment();
		else openDocument();
	}
	async function syncSearch() {
		if (!$user?.permissions.includes('search.manage')) return;
		try {
			await api('/search/reindex', {});
			flash('Search indexing queued. Results update shortly.');
		} catch {
			flash('Search indexing unavailable; database filtering remains available.');
		}
	}
	async function save(confirmed = false) {
		if (busy) return;
		const successMessage = 'Saved successfully';
		busy = true;
		error = '';
		try {
			if (modal === 'product') await api('/products', product);
			else if (modal === 'document') {
				const payload =
					confirmed && purchaseReview
						? purchaseReview.draft
						: (JSON.parse(JSON.stringify($state.snapshot(draft))) as typeof draft);
				payload.lines = payload.lines.map((line) => ({
					...line,
					product_id: Math.max(0, line.product_id),
					item_name: line.product_id <= 0 ? line.item_name?.trim() : undefined
				}));
				if (!confirmed || !purchaseReview) {
					const preview = await api<{
						new_products: NonNullable<typeof purchaseReview>['new_products'];
					}>('/documents/preview', payload);
					if (preview.new_products.length) {
						purchaseReview = { draft: payload, new_products: preview.new_products };
						return;
					}
				}
				await api('/documents', {
					...payload,
					confirm_new_products: confirmed && !!purchaseReview
				});
				purchaseReview = null;
			} else await api('/payments', payment);
			modal = '';
			await load();
			flash(successMessage);
			void syncSearch();
		} catch (e) {
			error = (e as Error).message;
		} finally {
			busy = false;
		}
	}
	async function linkPO() {
		if (!draft.purchase_order_id) return;
		try {
			const detail = await api<any>(`/documents/${draft.purchase_order_id}`);
			draft.lines = detail.lines
				.filter((l: Line) => (l.realized_quantity || 0) > 0)
				.map((l: Line) => ({
					product_id: l.product_id,
					quantity: l.realized_quantity || 1,
					price: l.price
				}));
			if (!draft.lines.length) draft.lines = [{ product_id: 0, quantity: 1, price: 0 }];
		} catch (e) {
			error = (e as Error).message;
		}
	}
	function pickProduct(line: Line) {
		line.item_name = '';
		line.price = products.find((p) => p.id === line.product_id)?.price || 0;
	}
	function toggleInvoice(id: number) {
		payment.invoice_ids = payment.invoice_ids.includes(id)
			? payment.invoice_ids.filter((i) => i !== id)
			: [...payment.invoice_ids, id];
	}
	function searchProducts() {
		clearTimeout(searchTimer);
		searchIds = null;
		const q = query;
		if (!q) return;
		searchTimer = setTimeout(async () => {
			try {
				const result = await api<{ hits: Product[] }>(`/search?q=${encodeURIComponent(q)}`);
				if (query === q) searchIds = result.hits.map((p) => p.id);
			} catch {}
		}, 250);
	}
	async function updateForecast() {
		try {
			forecasts = await api(`/forecast?days=${horizon}&lead_time=${lead}`);
		} catch (e) {
			error = (e as Error).message;
		}
	}
	function exportCsv() {
		const rows: any[] =
			page === 'Products'
				? filteredProducts
				: page === 'Payments'
					? filteredPayments
					: page === 'Stock forecast'
						? forecasts
						: page === 'Dashboard'
							? documents
							: filteredDocs;
		if (!rows.length) {
			flash('No records to export');
			return;
		}
		const keys = Object.keys(rows[0]);
		const quote = (v: any) => {
			let s = String(v ?? '');
			if (/^[=+@\-]/.test(s)) s = "'" + s;
			return '"' + s.replaceAll('"', '""') + '"';
		};
		const csv = [
			keys.map(quote).join(','),
			...rows.map((r) => keys.map((k) => quote(r[k])).join(','))
		].join('\r\n');
		const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
		const a = document.createElement('a');
		a.href = url;
		a.download = page.toLowerCase().replaceAll(' ', '-') + '.csv';
		a.click();
		URL.revokeObjectURL(url);
	}
	const badge = (s: string) =>
		['Paid', 'Complete', 'Active', 'Healthy'].includes(s)
			? 'good'
			: ['Warning', 'Out of stock'].includes(s)
				? 'danger'
				: ['Partial Paid', 'Partial', 'Reorder now'].includes(s)
					? 'warn'
					: 'neutral';
	const descriptions: Record<string, string> = {
		Dashboard: 'A clear view of your business, all in one place.',
		Products: 'Your product catalog and inventory, always in sync.',
		'Purchase orders': 'Plan purchases and keep supplier fulfillment on track.',
		'Purchase invoices': 'Record incoming goods and manage what you owe.',
		'Sales invoices': 'Manage customer invoices, stock, and outstanding balances.',
		Payments: 'One payment, multiple invoices. Every allocation accounted for.',
		'Stock forecast': 'Turn recent sales into a smarter replenishment plan.'
	};
</script>

<svelte:head
	><title>{page} · Stockroom</title><meta
		name="description"
		content="Stockroom product management: purchasing, inventory, sales, payments and stock forecasts."
	/></svelte:head
>
<main>
	<div class="page-heading">
		<div>
			<div class="eyebrow">YOUR BUSINESS, IN FOCUS</div>
			<h1>{page === 'Dashboard' ? 'Business overview' : page}</h1>
			<p>{descriptions[page]}</p>
		</div>
		<div class="heading-actions">
			<button class="button secondary" onclick={exportCsv}
				><Icon name="export" size={15} /> Export</button
			><button
				class="button primary"
				onclick={page === 'Dashboard' || page === 'Stock forecast'
					? () => {
							navigate('Purchase orders');
							openDocument();
						}
					: create}
				><span>+</span>{page === 'Dashboard' || page === 'Stock forecast'
					? 'Create order'
					: page === 'Products'
						? 'Add product'
						: page === 'Payments'
							? 'Record payment'
							: 'Create ' + (page === 'Purchase orders' ? 'order' : 'invoice')}</button
			>
		</div>
	</div>
	{#if error && !modal}<div class="alert" role="alert">
			{error}<button onclick={load}>Retry connection &#8635;</button>
		</div>{/if}
	{#if notice}<div class="toast" role="status">&#10003; {notice}</div>{/if}
	{#if loading}<div class="loading" role="status">Loading your workspace…</div>{/if}
	{#if page === 'Dashboard'}
		<div class="stats-grid">
			{#each [{ label: 'Total sales', value: money(salesTotal), icon: 'up', note: `${sales.length} customer invoices`, color: 'green' }, { label: 'Total purchases', value: money(purchaseTotal), icon: 'down', note: `${purchases.length} supplier invoices`, color: 'blue' }, { label: 'Outstanding payables', value: money(payable), icon: 'clock', note: `${purchases.filter((d) => d.remaining_payment > 0).length} invoices awaiting payment`, color: 'orange' }, { label: 'Products in stock', value: compact(products.reduce((s, p) => s + p.remaining_quantity, 0)), icon: 'products', note: `${products.length} products in your catalog`, color: 'purple' }] as stat}
				<section class="stat-card">
					<div class="stat-title">
						{stat.label}<span class="stat-icon {stat.color}"><Icon name={stat.icon} /></span>
					</div>
					<div class="stat-value">{stat.value}</div>
					<p>{stat.note}</p>
				</section>{/each}
		</div>
		<div class="chart-grid">
			<section class="panel revenue-panel">
				<div class="panel-heading">
					<div>
						<h2>Cash flow overview</h2>
						<p>Sales, purchases, and supplier payments</p>
					</div>
					<select aria-label="Chart period" bind:value={period}
						><option value="6">Last 6 months</option><option value="12">Last 12 months</option
						></select
					>
				</div>
				<div class="legend">
					<span><i class="sales-dot"></i>Sales</span><span
						><i class="purchase-dot"></i>Purchases</span
					><span><i class="payment-dot"></i>Payments</span>
				</div>
				<div class="bar-chart">
					<div class="y-axis">
						{#each [1, 0.75, 0.5, 0.25, 0] as fraction}<span
								>{chartMax === 1 ? '0' : compact(chartMax * fraction)}</span
							>{/each}
					</div>
					<div class="plot">
						<div class="grid-lines">
							{#each [1, 2, 3, 4, 5] as _}<i></i>{/each}
						</div>
						<div class="bar-groups">
							{#each months as m}<div class="month-group">
									<div class="bars">
										<div
											class="bar sales-bar"
											style:height={`${(m.sales / chartMax) * 100}%`}
											title={`${m.name} sales: ${money(m.sales)}`}
										></div>
										<div
											class="bar purchase-bar"
											style:height={`${(m.purchases / chartMax) * 100}%`}
											title={`${m.name} purchases: ${money(m.purchases)}`}
										></div>
										<div
											class="bar payment-bar"
											style:height={`${(m.payments / chartMax) * 100}%`}
											title={`${m.name} payments: ${money(m.payments)}`}
										></div>
									</div>
									<span>{m.name}</span>
								</div>{/each}
						</div>
						{#if documents.length === 0}<div class="chart-empty">
								Your cash flow story starts with your first invoice.
							</div>{/if}
					</div>
				</div>
				<div class="chart-footer">
					<span><span class="tiny-dot"></span> Based on recorded transactions</span><strong
						>IDR</strong
					>
				</div>
			</section>
			<section class="panel stock-panel">
				<div class="panel-heading">
					<div>
						<h2>Inventory health</h2>
						<p>A pulse check on your stock</p>
					</div>
					<span><Icon name="arrow" size={16} /></span>
				</div>
				<div
					class="donut"
					class:empty-inventory={activeProducts.length === 0}
					style={`--healthy:${activeProducts.length ? ((activeProducts.length - lowStock.length) / activeProducts.length) * 100 : 0}%;--low:${activeProducts.length ? ((activeProducts.length - lowStock.filter((p) => p.remaining_quantity === 0).length) / activeProducts.length) * 100 : 0}%`}
				>
					<div><strong>{activeProducts.length}</strong><span>Active products</span></div>
				</div>
				<div class="health-legend">
					<div>
						<span><i class="sales-dot"></i>Healthy stock</span><strong
							>{activeProducts.length - lowStock.length}</strong
						>
					</div>
					<div>
						<span><i class="orange-dot"></i>Low stock</span><strong
							>{lowStock.filter((p) => p.remaining_quantity > 0).length}</strong
						>
					</div>
					<div>
						<span><i class="red-dot"></i>Out of stock</span><strong
							>{lowStock.filter((p) => p.remaining_quantity === 0).length}</strong
						>
					</div>
				</div>
				<button class="text-link" onclick={() => navigate('Products')}
					>View inventory <span><Icon name="arrow" size={16} /></span></button
				>
			</section>
		</div>
		<section class="receivables-banner" aria-labelledby="receivables-title">
			<div class="receivables-label">
				<span class="receivables-icon"><Icon name="payment" size={22} /></span>
				<div>
					<h2 id="receivables-title">Receivables</h2>
					<p>
						Outstanding customer balances across {sales.filter((d) => d.remaining_payment > 0)
							.length} invoices
					</p>
				</div>
			</div>
			<strong class="receivables-amount">{money(receivable)}</strong>
			<a class="button secondary receivables-link" href="/?view=Sales%20invoices"
				>View sales invoices <Icon name="arrow" size={15} /></a
			>
		</section>
		<div class="bottom-grid">
			<section class="panel">
				<div class="panel-heading">
					<div>
						<h2>Recent transactions</h2>
						<p>The latest activity in your workspace</p>
					</div>
					<button class="text-link" onclick={() => navigate('Purchase invoices')}
						>View all &#8594;</button
					>
				</div>
				<div class="table-wrap">
					<table>
						<thead><tr><th>TRANSACTION</th><th>TYPE</th><th>AMOUNT</th><th>STATUS</th></tr></thead
						><tbody
							>{#each documents.slice(0, 5) as d}<tr
									><td
										><a class="record-link" href={`/documents/${d.id}/`}>{d.number}</a><small
											>{d.person} · {d.date}</small
										></td
									><td>{d.kind.replaceAll('_', ' ')}</td><td class="amount"
										>{money(d.grand_total)}</td
									><td><span class="badge {badge(d.status)}">{d.status}</span></td></tr
								>{:else}<tr
									><td colspan="4" class="empty-cell"
										><span><Icon name="arrow" size={16} /></span><strong>No transactions yet</strong
										>
										<p>Create an order or invoice to get started.</p></td
									></tr
								>{/each}</tbody
						>
					</table>
				</div>
			</section>
			<section class="panel attention">
				<div class="panel-heading">
					<div>
						<h2>
							<span class="attention-icon"><Icon name="flag" size={15} /></span> Needs attention
						</h2>
						<p>A few things to keep on your radar</p>
					</div>
					<span class="count-pill"
						>{lowStock.length + documents.filter((d) => d.status === 'Warning').length}</span
					>
				</div>
				<div class="stock-alert-list">
					{#each lowStock.slice(0, 3) as p}<article class="stock-alert-card">
							<a class="stock-alert-product" href={`/products/${p.id}/`}
								><span class="stock-alert-image"
									>{#if p.photo}<img src={p.photo} alt="" />{:else}<Icon
											name="products"
											size={20}
										/>{/if}</span
								><span class="stock-alert-name"
									><strong>{p.name}</strong><small
										>{p.remaining_quantity} available &middot; Reorder level {p.reorder_level}</small
									></span
								><Icon name="arrow" size={15} /></a
							>
							<div class="stock-alert-footer">
								<span class="stock-alert-status" class:out-of-stock={p.remaining_quantity === 0}
									>{p.remaining_quantity === 0 ? 'Out of stock' : 'Low stock'}</span
								>
								{#if $user?.permissions.includes('invoices.write')}<button
										class="stock-alert-action"
										aria-label={`Create purchase invoice for ${p.name}`}
										onclick={() => replenish(p)}
										><Icon name="invoice" size={15} />Create purchase invoice<Icon
											name="arrow"
											size={14}
										/></button
									>{/if}
							</div>
						</article>
					{:else}<div class="all-clear">
							<span><Icon name="check" /></span><strong>Looking good</strong>
							<p>No stock alerts at the moment.</p>
						</div>{/each}
				</div>
				{#if documents.some((d) => d.status === 'Warning')}<button
						class="warning-link"
						onclick={() => {
							navigate('Purchase orders');
							status = 'Warning';
						}}>Review PO quantity discrepancies &#8594;</button
					>{/if}<button class="forecast-link" onclick={() => navigate('Stock forecast')}
					><span
						><Icon name="spark" size={14} /> <strong>Plan your next move</strong><small
							>Explore your stock forecast</small
						></span
					><span><Icon name="arrow" size={16} /></span></button
				>
			</section>
		</div>
		<div class="dashboard-note">
			All amounts in Indonesian Rupiah <span>&middot;</span> Updated from your workspace
		</div>
	{:else if page === 'Stock forecast'}
		<section class="forecast-banner">
			<div class="forecast-spark"><Icon name="spark" size={40} /></div>
			<div>
				<h2>Stay one step ahead of demand.</h2>
				<p>A 30-day sales average estimates stock coverage and recommended replenishment.</p>
			</div>
			<div class="forecast-controls">
				<label
					>Forecast horizon<select bind:value={horizon} onchange={updateForecast}
						><option value={7}>7 days</option><option value={30}>30 days</option><option value={60}
							>60 days</option
						><option value={90}>90 days</option></select
					></label
				><label
					>Supplier lead time<input
						type="number"
						min="1"
						max="365"
						bind:value={lead}
						onchange={updateForecast}
					/></label
				>
			</div>
		</section>
		<div class="stats-grid three">
			<section class="stat-card">
				<div class="stat-title">
					Needs replenishment <span><Icon name="arrow" size={16} /></span>
				</div>
				<div class="stat-value">{forecasts.filter((f) => f.risk !== 'Healthy').length}</div>
				<p>Products at risk or below reorder level</p>
			</section>
			<section class="stat-card">
				<div class="stat-title">
					Suggested order <span><Icon name="arrow" size={16} /></span>
				</div>
				<div class="stat-value">
					{compact(forecasts.reduce((s, f) => s + f.suggested_order, 0))} <small>units</small>
				</div>
				<p>To cover horizon, lead time, and buffer</p>
			</section>
			<section class="stat-card">
				<div class="stat-title">
					Forecast window <span><Icon name="arrow" size={16} /></span>
				</div>
				<div class="stat-value">{horizon} <small>days</small></div>
				<p>Based on the last 30 calendar days</p>
			</section>
		</div>
		<section class="panel">
			<div class="panel-heading">
				<div>
					<h2>Stock outlook</h2>
					<p>Projected stock can go below zero to show unmet demand.</p>
				</div>
			</div>
			<div class="table-wrap">
				<table>
					<thead
						><tr
							><th>PRODUCT</th><th>REMAINING QTY</th><th>DAILY SALES</th><th>COVERAGE</th><th
								>PROJECTED STOCK</th
							><th>ORDER SUGGESTION</th><th>STATUS</th><th>ACTION</th></tr
						></thead
					><tbody
						>{#each forecasts as f}<tr
								><td><a class="record-link" href={`/products/${f.id}/`}>{f.name}</a></td><td
									>{f.quantity}</td
								><td>{f.daily_sales.toFixed(2)}</td><td
									>{f.days_remaining === null
										? 'No recent sales'
										: f.days_remaining.toFixed(1) + ' days'}</td
								><td class:negative={f.projected_stock < 0}>{f.projected_stock.toFixed(0)}</td><td
									><strong>{f.suggested_order} units</strong></td
								><td><span class="badge {badge(f.risk)}">{f.risk}</span></td><td
									>{#if f.risk !== 'Healthy'}<button
											class="button secondary replenish-button"
											aria-label={`Create purchase invoice for ${f.name}`}
											onclick={() => replenish(f, f.suggested_order)}>+ Purchase invoice</button
										>{/if}</td
								></tr
							>{:else}<tr
								><td colspan="8" class="empty-cell"
									><span><Icon name="arrow" size={16} /></span><strong
										>Your forecast starts here</strong
									>
									<p>Add products and record sales to see demand estimates.</p></td
								></tr
							>{/each}</tbody
					>
				</table>
			</div>
		</section>
		<p class="method-note">
			Method: daily demand = units sold in the last 30 days ÷ 30. Suggested order = demand ×
			(horizon + lead time) + reorder buffer - current stock, rounded up. Estimates assume steady
			demand and exclude incoming POs.
		</p>
	{:else}
		<section class="panel records">
			<div class="records-toolbar">
				<div class="tabs">
					<button class="selected" onclick={() => (status = 'All statuses')}
						>All {page.toLowerCase()}
						<span
							>{page === 'Products'
								? products.length
								: page === 'Payments'
									? payments.length
									: documents.filter((d) => d.kind === kinds[page]).length}</span
						></button
					>
				</div>
				<div class="filters">
					<label class="search-field"
						><span><Icon name="search" size={16} /></span><input
							aria-label="Search records"
							placeholder={`Search ${page.toLowerCase()}...`}
							bind:value={query}
							oninput={page === 'Products' ? searchProducts : () => {}}
						/></label
					>{#if page !== 'Payments'}<select aria-label="Filter by status" bind:value={status}
							><option>All statuses</option
							>{#each page === 'Products' ? ['Active', 'Inactive'] : page === 'Purchase orders' ? ['Open', 'Partial', 'Complete', 'Warning'] : ['Unpaid', 'Partial Paid', 'Paid', ...(page === 'Sales invoices' ? ['Cancelled'] : [])] as s}<option
									>{s}</option
								>{/each}</select
						>{/if}{#if page === 'Products'}<button
							class="icon-button"
							title="Rebuild search index"
							onclick={syncSearch}><Icon name="refresh" /></button
						>{/if}
				</div>
			</div>
			<div class="table-wrap">
				<table>
					<thead
						>{#if page === 'Products'}<tr
								><th>PRODUCT</th><th>PHYSICAL STOCK</th><th>REMAINING QTY</th><th>UNIT PRICE</th><th
									>REORDER LEVEL</th
								><th>STATUS</th><th></th></tr
							>{:else if page === 'Payments'}<tr
								><th>REFERENCE</th><th>DATE</th><th>TYPE</th><th>INVOICES</th><th>AMOUNT</th><th
									aria-label="Actions"
								></th></tr
							>{:else}<tr
								><th>{page === 'Purchase orders' ? 'ORDER' : 'INVOICE'} NUMBER</th><th
									>{page === 'Sales invoices' ? 'CUSTOMER' : 'PERSON IN CHARGE'}</th
								><th>DATE</th><th>GRAND TOTAL</th>{#if page !== 'Purchase orders'}<th
										>OUTSTANDING</th
									>{/if}<th>STATUS</th><th></th></tr
							>{/if}</thead
					><tbody>
						{#if page === 'Products'}{#each filteredProducts as p}<tr
									><td
										><div class="product-cell">
											{#if p.photo}<img src={p.photo} alt="" />{:else}<span class="product-letter"
													>{p.name.slice(0, 2).toUpperCase()}</span
												>{/if}
											<div>
												<a class="record-link" href={`/products/${p.id}/`}>{p.name}</a><small
													>{p.description || 'No description'}</small
												>
											</div>
										</div></td
									><td
										><strong class:negative={p.remaining_quantity <= p.reorder_level}
											>{p.quantity}</strong
										>
										<span class="muted">units</span
										>{#if p.remaining_quantity <= p.reorder_level}<span class="stock-alert"
												>Low stock</span
											>{/if}</td
									><td
										><strong class:negative={p.remaining_quantity <= p.reorder_level}
											>{p.remaining_quantity}</strong
										> <span class="muted">available</span></td
									><td>{money(p.price)}</td><td>{p.reorder_level} units</td><td
										><span class="badge {badge(p.status)}">{p.status}</span></td
									><td
										><button class="text-link" onclick={() => openProduct(p)}>Edit &#8599;</button
										>{#if p.status === 'Active' && (p.remaining_quantity <= p.reorder_level || forecasts.some((f) => f.id === p.id && f.risk !== 'Healthy'))}<button
												class="button secondary replenish-button"
												aria-label={`Create purchase invoice for ${p.name}`}
												onclick={() => replenish(p)}>+ Purchase invoice</button
											>{/if}</td
									></tr
								>{:else}<tr
									><td colspan="7" class="empty-cell"
										><span><Icon name="arrow" size={16} /></span><strong
											>{query ? 'No matching products' : 'Make room for your first product'}</strong
										>
										<p>Add products to start managing your inventory.</p>
										<button class="button primary" onclick={() => openProduct()}
											>+ Add product</button
										></td
									></tr
								>{/each}
						{:else if page === 'Payments'}{#each filteredPayments as p}<tr
									><td><a class="record-link" href={`/payments/${p.id}/`}>{p.reference}</a></td><td
										>{p.date}</td
									><td>{p.kind === 'sales_invoice' ? 'Customer receipt' : 'Supplier payment'}</td
									><td
										><div class="inline-records">
											<a
												class="invoice-count-link"
												href={`/payments/${p.id}/`}
												aria-label={`View ${p.invoice_links?.length || 0} invoices for ${p.reference}`}
												>{p.invoice_links?.length || 0}
												{p.invoice_links?.length === 1 ? 'invoice' : 'invoices'}</a
											>
										</div></td
									><td class="amount">{money(p.amount)}</td><td
										><a
											class="text-link"
											href={`/payments/${p.id}/`}
											aria-label={`View payment ${p.reference}`}>View &#8599;</a
										></td
									></tr
								>{:else}<tr
									><td colspan="6" class="empty-cell"
										><span><Icon name="arrow" size={16} /></span><strong>No payments found</strong>
										<p>Record a payment against one or more outstanding invoices.</p></td
									></tr
								>{/each}
						{:else}{#each filteredDocs as d}<tr
									><td
										><a class="record-link" href={`/documents/${d.id}/`}>{d.number}</a
										>{#if d.purchase_order_id && purchaseOrderNumber(d.purchase_order_id)}<a
												class="purchase-order-reference list-order-reference"
												href={`/documents/${d.purchase_order_id}/`}
												><span>PO</span>{purchaseOrderNumber(d.purchase_order_id)}</a
											>{/if}</td
									><td>{d.person}</td><td>{d.date}</td><td class="amount">{money(d.grand_total)}</td
									>{#if page !== 'Purchase orders'}<td>{money(d.remaining_payment)}</td>{/if}<td
										><span class="badge {badge(d.status)}">{d.status}</span></td
									><td><a class="text-link" href={`/documents/${d.id}/`}>View &#8599;</a></td></tr
								>{:else}<tr
									><td colspan="7" class="empty-cell"
										><span><Icon name="arrow" size={16} /></span><strong
											>{query || status !== 'All statuses'
												? 'No matching records'
												: 'A fresh start for your ' + page.toLowerCase()}</strong
										>
										<p>Create your first record to bring your workspace to life.</p>
										<button class="button primary" onclick={openDocument}
											>+ Create {page === 'Purchase orders' ? 'order' : 'invoice'}</button
										></td
									></tr
								>{/each}{/if}</tbody
					>
				</table>
			</div>
			<div class="table-footer">
				<span
					>{page === 'Products'
						? filteredProducts.length
						: page === 'Payments'
							? filteredPayments.length
							: filteredDocs.length} records</span
				><span>All amounts in IDR</span>
			</div>
		</section>
	{/if}
	<footer class="main-footer">
		<span
			><span class="brand-mini"><Icon name="products" size={13} /></span> A little more clarity. A lot
			more control.</span
		><span>Stockroom · Product management</span>
	</footer>
</main>
{#if modal}
	<div class="modal-backdrop" role="presentation">
		<dialog
			class="modal"
			use:focusModal
			oncancel={(event) => {
				if (busy) event.preventDefault();
				else {
					modal = '';
					error = '';
				}
			}}
			class:wide={modal === 'document'}
			aria-modal="true"
			aria-label={modal === 'product'
				? 'Product'
				: modal === 'document'
					? 'Create document'
					: modal === 'payment'
						? 'Record payment'
						: 'Document details'}
			tabindex="-1"
		>
			<div class="modal-heading">
				<div>
					<div class="eyebrow">STOCKROOM WORKSPACE</div>
					<h2>
						{modal === 'product'
							? product.id
								? 'Edit product'
								: 'Add a product'
							: modal === 'document'
								? 'Create ' + draft.kind.replaceAll('_', ' ')
								: modal === 'payment'
									? 'Record payment'
									: 'Product'}
					</h2>
				</div>
				<button
					class="icon-button"
					aria-label="Close dialog"
					onclick={() => {
						modal = '';
						error = '';
					}}
					disabled={busy}><Icon name="close" /></button
				>
			</div>
			{#if error}<div class="alert" role="alert">{error}</div>{/if}
			<form
				onsubmit={(e) => {
					e.preventDefault();
					void save(!!purchaseReview);
				}}
			>
				{#if modal === 'document' && purchaseReview}
					<section class="purchase-preview" aria-label="New product creation preview">
						<div class="preview-heading">
							<Icon name="products" size={24} />
							<div>
								<h3>Review new products</h3>
								<p>These products will be created when you confirm and post this document.</p>
							</div>
						</div>
						<div class="detail-meta">
							<div><small>DOCUMENT</small><strong>{purchaseReview.draft.number}</strong></div>
							<div><small>DATE</small><strong>{purchaseReview.draft.date}</strong></div>
							<div>
								<small>TOTAL</small><strong
									>{money(
										purchaseReview.draft.lines.reduce(
											(sum, line) => sum + line.quantity * line.price,
											0
										)
									)}</strong
								>
							</div>
						</div>
						<div class="table-wrap">
							<table>
								<thead
									><tr
										><th>NEW PRODUCT</th><th>UNIT PRICE</th><th>PHYSICAL STOCK AFTER POSTING</th><th
											>REMAINING QTY</th
										><th>STATUS</th></tr
									></thead
								><tbody
									>{#each purchaseReview.new_products as p}<tr
											><td><strong>{p.name}</strong></td><td>{money(p.price)}</td><td
												>{p.quantity} units</td
											><td>0 units</td><td><span class="badge good">{p.status}</span></td></tr
										>{/each}</tbody
								>
							</table>
						</div>
						<p class="method-note">
							Default reorder level: 10 units. {purchaseReview.draft.kind === 'purchase_order'
								? 'Purchase orders create products with zero stock; goods are received with a purchase invoice.'
								: 'This invoice receives physical stock immediately; remaining quantity stays zero until the invoice is fully Paid.'}
							No products or documents have been saved yet.
						</p>
						{#if purchaseReview.draft.purchase_order_id}<p class="validation-message">
								Items outside the original PO will appear as unplanned deliveries and flag the PO as
								Warning.
							</p>{/if}
					</section>
				{:else if modal === 'product'}{#if product.id > 0}<a
							class="record-link"
							href={`/products/${product.id}/`}>View product detail page &#8599;</a
						>{/if}
					<div class="form-grid">
						<label class="full"
							>Product name<input
								required
								maxlength="200"
								bind:value={product.name}
								placeholder="e.g. Wireless keyboard"
							/></label
						><label
							>Physical stock<input
								required
								type="number"
								min="0"
								step="1"
								oninput={(event) => {
									if (!product.id) product.remaining_quantity = Number(event.currentTarget.value);
								}}
								bind:value={product.quantity}
							/></label
						><label
							>Remaining quantity<input
								required
								type="number"
								min="0"
								max={product.quantity}
								step="1"
								bind:value={product.remaining_quantity}
							/><small class="muted">Available to sell; cannot exceed physical stock.</small></label
						><label
							>Unit price (IDR)<input
								required
								type="number"
								min="0"
								step="1"
								bind:value={product.price}
							/></label
						><label
							>Reorder level<input
								required
								type="number"
								min="0"
								step="1"
								bind:value={product.reorder_level}
							/></label
						><label
							>Status<select bind:value={product.status}
								><option>Active</option><option>Inactive</option></select
							></label
						><label class="full"
							>Photo URL <span class="muted">(optional)</span><input
								type="url"
								bind:value={product.photo}
								placeholder="https://..."
							/></label
						><label class="full"
							>Description<textarea
								rows="3"
								bind:value={product.description}
								placeholder="A few details about this product"></textarea></label
						>
					</div>
					{#if product.id}<p class="method-note">
							Changing stock records a manual adjustment in the stock ledger.
						</p>{/if}
				{:else if modal === 'document'}<div class="form-grid">
						<label
							>Document number<input
								required
								maxlength="100"
								bind:value={draft.number}
								placeholder={draft.kind === 'purchase_order'
									? 'PO-2026-001'
									: draft.kind === 'purchase_invoice'
										? 'PINV-2026-001'
										: 'SINV-2026-001'}
							/></label
						><label>Date<input type="date" required bind:value={draft.date} /></label><label
							class:full={draft.kind !== 'purchase_invoice'}
							>{draft.kind === 'sales_invoice'
								? 'Customer name'
								: draft.kind === 'purchase_invoice'
									? 'Created by'
									: 'Person in charge'}<input
								required
								maxlength="200"
								bind:value={draft.person}
							/></label
						>{#if draft.kind === 'purchase_invoice'}<label
								>Purchase order <span class="muted">(optional)</span><select
									bind:value={draft.purchase_order_id}
									onchange={linkPO}
									><option value={null}>Standalone invoice</option
									>{#each documents.filter((d) => d.kind === 'purchase_order') as d}<option
											value={d.id}>{d.number}</option
										>{/each}</select
								></label
							>{/if}
					</div>
					<div class="line-title">
						<h3>Line items</h3>
						<button
							type="button"
							class="text-link"
							onclick={() => draft.lines.push({ product_id: 0, quantity: 1, price: 0 })}
							>+ Add line</button
						>
					</div>
					{#each draft.lines as l, i}<div class="line-inputs">
							<div class="product-picker">
								<label
									>Product<select
										required
										bind:value={l.product_id}
										onchange={() => pickProduct(l)}
									>
										<option value={0} disabled>Select a product</option>
										{#if draft.kind !== 'sales_invoice'}<option value={-1}
												>+ Enter a new product</option
											>{/if}
										{#each products.filter((p) => p.status === 'Active') as p}<option value={p.id}
												>{p.name} ({p.remaining_quantity} available)</option
											>{/each}
									</select></label
								>
								{#if l.product_id > 0}<a class="record-link" href={`/products/${l.product_id}/`}
										>View product details &#8599;</a
									>{/if}
								{#if l.product_id === -1}<label
										>New product name<input
											required
											maxlength="200"
											bind:value={l.item_name}
											placeholder="e.g. Ergonomic Office Chair"
										/><small class="muted"
											>New products require confirmation. Matching names reuse existing products.</small
										></label
									>{/if}
							</div>
							<label
								>Quantity<input
									required
									type="number"
									min="1"
									step="1"
									max={draft.kind === 'sales_invoice'
										? (products.find((p) => p.id === l.product_id)?.remaining_quantity ?? 0)
										: undefined}
									bind:value={l.quantity}
								/></label
							><label
								>Price (IDR)<input
									required
									type="number"
									min="0"
									step="1"
									bind:value={l.price}
								/></label
							>
							<div class="line-subtotal">{money(l.quantity * l.price)}</div>
							<button
								type="button"
								class="icon-button"
								aria-label={`Remove line ${i + 1}`}
								disabled={draft.lines.length === 1}
								onclick={() => draft.lines.splice(i, 1)}>×</button
							>
						</div>{/each}
					<div class="document-total">Grand total <strong>{money(draftTotal)}</strong></div>
					<p class="method-note">
						{draft.kind === 'purchase_order'
							? 'Orders reserve the purchasing plan. Stock changes when a purchase invoice is posted.'
							: draft.kind === 'purchase_invoice'
								? 'Posting receives physical stock. Units become available for sale when the purchase invoice is fully Paid. Linked PO quantities are deducted immediately.'
								: 'Posting deducts stock immediately. Record customer receipts in Payments.'}
					</p>
				{:else if modal === 'payment'}<div class="form-grid">
						<label
							>Reference<input
								required
								maxlength="100"
								bind:value={payment.reference}
								placeholder="PAY-2026-001"
							/></label
						><label>Date<input type="date" required bind:value={payment.date} /></label><label
							>Payment type<select
								bind:value={payment.kind}
								onchange={() => (payment.invoice_ids = [])}
								><option value="purchase_invoice">Supplier payment</option><option
									value="sales_invoice">Customer receipt</option
								></select
							></label
						><label
							>Amount (IDR)<input
								required
								type="number"
								min="1"
								step="1"
								bind:value={payment.amount}
							/></label
						>
					</div>
					<h3>Select invoices in allocation order</h3>
					<p class="method-note">
						The first selected invoice is paid first. Each following invoice receives the remaining
						amount.
					</p>
					<div class="invoice-picker">
						{#each documents.filter((d) => d.kind === payment.kind && d.remaining_payment > 0) as d}<label
								><input
									type="checkbox"
									checked={payment.invoice_ids.includes(d.id)}
									onchange={() => toggleInvoice(d.id)}
								/><span
									><a class="record-link" href={`/documents/${d.id}/`}>{d.number}</a><small
										>{d.person}</small
									></span
								><strong>{money(d.remaining_payment)}</strong
								>{#if payment.invoice_ids.includes(d.id)}<span class="selection-order"
										>{payment.invoice_ids.indexOf(d.id) + 1}</span
									>{/if}</label
							>{:else}<p class="method-note">
								No outstanding invoices. Create an invoice first.
							</p>{/each}
					</div>
					{#if allocations.length}<h3>Allocation preview</h3>
						{#each allocations as a}<div class="allocation">
								<span
									>{a.number}<small
										>{a.remaining === 0 ? 'Paid' : 'Partial Paid'} · remaining {money(
											a.remaining
										)}</small
									></span
								><strong>{money(a.amount)}</strong>
							</div>{/each}{/if}{#if paymentError}<p class="validation-message">
							{paymentError}
						</p>{/if}
				{/if}
				<div class="modal-actions">
					<button
						type="button"
						class="button secondary"
						disabled={busy}
						onclick={() => {
							if (purchaseReview) {
								purchaseReview = null;
							} else {
								modal = '';
							}
							error = '';
						}}>{purchaseReview ? 'Back to edit' : 'Cancel'}</button
					><button class="button primary" disabled={busy || (modal === 'payment' && !!paymentError)}
						>{busy
							? 'Saving…'
							: modal === 'document'
								? purchaseReview
									? 'Confirm products & post'
									: 'Review & post document'
								: modal === 'payment'
									? 'Record payment'
									: 'Save product'}</button
					>
				</div>
			</form>
		</dialog>
	</div>
{/if}
