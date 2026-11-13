<script lang="ts">
	import { onMount, tick } from 'svelte';
	import type { Snippet } from 'svelte';
	import { api, money, today } from './api';
	import { workspaceConnection } from './auth';
	import type { Column } from './list-config';
	type Page = {
		items: any[];
		next_cursor: string | null;
		start_cursor?: string;
		has_more: boolean;
		total: number;
		limit: number;
		summary?: any;
	};
	let {
		resource,
		title,
		columns,
		statuses = [],
		dateLabel = 'Date',
		refresh = 0,
		extra = {},
		actions,
		onpage
	}: {
		resource: string;
		title: string;
		columns: Column[];
		statuses?: string[];
		dateLabel?: string;
		refresh?: number;
		extra?: Record<string, string | number>;
		actions?: Snippet<[any]>;
		onpage?: (page: Page) => void;
	} = $props();
	let search = $state(''),
		status = $state(''),
		kind = $state(''),
		preset = $state('all'),
		from = $state(''),
		to = $state(''),
		size = $state(25);
	let result = $state<Page>({ items: [], next_cursor: null, has_more: false, total: 0, limit: 25 });
	let cursors = $state<(string | null)[]>([null]),
		index = $state(0),
		busy = $state(false),
		error = $state(''),
		mounted = $state(false);
	let listContainer: HTMLDivElement, table: HTMLTableElement;
	let columnWidths = $state<number[]>([]),
		horizontalOffset = $state(0);
	let generation = 0,
		previousKey = '',
		currentFilters = '';
	function dates() {
		if (preset === 'custom') return { from, to };
		if (preset === 'all') return {};
		const end = today();
		const d = new Date(end + 'T12:00:00');
		if (preset === 'week') d.setDate(d.getDate() - 6);
		if (preset === 'month') d.setDate(d.getDate() - 29);
		if (preset === 'this-month') d.setDate(1);
		if (preset === 'year') d.setFullYear(d.getFullYear() - 1);
		return {
			from: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`,
			to: end
		};
	}
	async function fetchPage(filters: string, cursor: string | null) {
		const request = ++generation;
		busy = true;
		error = '';
		const params = new URLSearchParams(filters);
		if (cursor) params.set('cursor', cursor);
		try {
			const data = await api<Page>(`/lists/${resource}?${params}`);
			if (request !== generation) return;
			result = data;
			if (index === 0 && data.start_cursor) cursors = [data.start_cursor];
			workspaceConnection.set(true);
			onpage?.(data);
		} catch (e) {
			if (request === generation) {
				error = (e as Error).message;
				result = { items: [], next_cursor: null, has_more: false, total: 0, limit: size };
			}
		} finally {
			if (request === generation) busy = false;
		}
	}
	$effect(() => {
		const filters = new URLSearchParams({
			limit: String(size),
			...Object.fromEntries(Object.entries(extra).map(([k, v]) => [k, String(v)]))
		});
		if (search.trim()) filters.set('q', search.trim());
		if (status) filters.set('status', status);
		if (kind && resource === 'payments') filters.set('kind', kind);
		for (const [k, v] of Object.entries(dates())) if (v) filters.set(k, v);
		const serialized = filters.toString();
		const key = resource + '?' + serialized + ':' + refresh;
		if (!mounted) return;
		if (preset === 'custom' && from && to && from > to) {
			generation++;
			busy = false;
			previousKey = '';
			cursors = [null];
			index = 0;
			error = 'Start date must not be after end date.';
			result = { items: [], next_cursor: null, has_more: false, total: 0, limit: size };
			return;
		}
		generation++;
		busy = true;
		const timer = setTimeout(() => {
			if (key !== previousKey) {
				previousKey = key;
				currentFilters = serialized;
				cursors = [null];
				index = 0;
			}
			void fetchPage(serialized, null);
		}, 250);
		return () => clearTimeout(timer);
	});
	async function move(direction: number) {
		if (busy) return;
		if (direction > 0 && result.next_cursor) {
			cursors = [...cursors.slice(0, index + 1), result.next_cursor];
			index++;
		} else if (direction < 0 && index > 0) index--;
		else return;
		await fetchPage(currentFilters, cursors[index]);
		await tick();
		listContainer.scrollIntoView({ block: 'start', behavior: 'instant' });
	}
	function clear() {
		search = '';
		status = '';
		kind = '';
		preset = 'all';
		from = '';
		to = '';
	}
	onMount(() => {
		mounted = true;
		const resize = new ResizeObserver(() => {
			columnWidths = Array.from(
				table.querySelectorAll('thead th'),
				(cell) => cell.getBoundingClientRect().width
			);
		});
		resize.observe(table);
		table.querySelectorAll('thead th').forEach((cell) => resize.observe(cell));
		const reload = () => {
			previousKey = '';
			cursors = [null];
			index = 0;
			void fetchPage(currentFilters, null);
		};
		window.addEventListener('workspace-refresh', reload);
		return () => {
			generation++;
			resize.disconnect();
			window.removeEventListener('workspace-refresh', reload);
		};
	});
	const badgeClass = (value: any) =>
		['Paid', 'Complete', 'Active', 'Healthy', 'Delivered', 'Approved'].includes(value)
			? 'positive'
			: ['Cancelled', 'Rejected', 'Out of stock', 'Warning'].includes(value)
				? 'danger'
				: 'neutral';
</script>

<section class="cursor-list" aria-label={title}>
	<div class="list-scroll" bind:this={listContainer}>
		<div class="list-controls">
			<div class="list-toolbar">
				<div class="list-title">
					<strong>{title}</strong><span>{result.total.toLocaleString()} records</span><small
						>Newest first</small
					>
				</div>
				<div class="list-filters">
					<label class="list-search"
						>Search<input
							aria-label={`Search ${title.toLowerCase()}`}
							placeholder="Search records..."
							bind:value={search}
							maxlength="200"
						/></label
					>
					{#if statuses.length}<label
							>Status<select aria-label="Status" bind:value={status}
								><option value="">All statuses</option>{#each statuses as s}<option value={s}
										>{s}</option
									>{/each}</select
							></label
						>{/if}
					{#if resource === 'payments'}<label
							>Payment type<select aria-label="Payment type" bind:value={kind}
								><option value="">All types</option><option value="sales_invoice"
									>Customer receipt</option
								><option value="purchase_invoice">Supplier payment</option></select
							></label
						>{/if}
					<label
						>{dateLabel}<select aria-label={`${dateLabel} filter`} bind:value={preset}
							><option value="all">All dates</option><option value="today">Today</option><option
								value="week">Last 7 days</option
							><option value="month">Last 30 days</option><option value="this-month"
								>This month</option
							><option value="year">Last 12 months</option><option value="custom"
								>Custom range</option
							></select
						></label
					>
					{#if preset === 'custom'}<label
							>From<input type="date" bind:value={from} max={to || undefined} /></label
						><label>To<input type="date" bind:value={to} min={from || undefined} /></label>{/if}
					<button
						class="button secondary"
						onclick={clear}
						disabled={!search && !status && !kind && preset === 'all'}>Clear filters</button
					>
				</div>
			</div>
			<div class="list-column-clip" aria-hidden="true">
				<div class="list-column-headings" style={`transform:translateX(-${horizontalOffset}px)`}>
					{#each columns as col, i}<div style={`width:${columnWidths[i] || 120}px`}>
							{col.label}
						</div>{/each}
					{#if actions}<div style={`width:${columnWidths[columns.length] || 100}px`}>
							Actions
						</div>{/if}
				</div>
			</div>
		</div>
		{#if error}<div class="list-error" role="alert">
				{error}{#if !(preset === 'custom' && from && to && from > to)}<button
						onclick={() => fetchPage(currentFilters, cursors[index])}>Retry</button
					>{/if}
			</div>{/if}
		<div
			class="list-table-scroll"
			onscroll={(event) => (horizontalOffset = event.currentTarget.scrollLeft)}
		>
			<table bind:this={table} aria-busy={busy}>
				<thead class="native-head"
					><tr
						>{#each columns as col}<th>{col.label}</th>{/each}{#if actions}<th
								class="action-heading">Actions</th
							>{/if}</tr
					></thead
				>
				<tbody class:loading={busy}>
					{#if busy && !result.items.length}<tr
							><td colspan={columns.length + (actions ? 1 : 0)} class="list-empty"
								><span role="status">Loading records...</span></td
							></tr
						>
					{:else}{#each result.items as row (row.id)}<tr>
								{#each columns as col}<td class:amount={col.type === 'money'}>
										{#if col.type === 'product'}<div class="product-cell">
												{#if row.photo}<img src={row.photo} alt="" />{:else}<span
														class="product-letter">{row.name.slice(0, 2).toUpperCase()}</span
													>{/if}
												<div>
													<a class="record-link" href={col.href?.(row)}>{row.name}</a><small
														>{row.description || 'No description'}</small
													>
												</div>
											</div>
										{:else if col.type === 'badge'}<span
												class={`list-badge ${badgeClass(row[col.key])}`}>{row[col.key]}</span
											>
										{:else if col.href && row[col.key] !== null}<a
												class="record-link"
												href={col.href(row)}
												>{col.format ? col.format(row[col.key], row) : row[col.key]}</a
											>
											{#if col.type === 'document' && row.purchase_order_number}<a
													class="purchase-order-reference list-order-reference"
													href={`/documents/${row.purchase_order_id}/`}
													><span>PO</span>{row.purchase_order_number}</a
												>{/if}
										{:else}{col.format
												? col.format(row[col.key], row)
												: col.type === 'money'
													? money(Number(row[col.key]))
													: (row[col.key] ?? '—')}{/if}
									</td>{/each}
								{#if actions}<td class="actions-cell"
										><div class="list-actions">{@render actions(row)}</div></td
									>{/if}
							</tr>{:else}<tr
								><td colspan={columns.length + (actions ? 1 : 0)} class="list-empty"
									>No records match these filters.</td
								></tr
							>{/each}{/if}
				</tbody>
			</table>
		</div>
	</div>
	<div class="list-pagination">
		<span aria-live="polite"
			>{busy
				? 'Loading...'
				: result.items.length
					? `${index * size + 1}–${index * size + result.items.length} of ${result.total.toLocaleString()}`
					: '0 records'}</span
		>
		<label
			>Rows per page<select aria-label="Rows per page" bind:value={size}
				><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option
				></select
			></label
		>
		<div>
			<button class="button secondary" disabled={busy || index === 0} onclick={() => move(-1)}
				>Previous</button
			><button class="button secondary" disabled={busy || !result.has_more} onclick={() => move(1)}
				>Next</button
			>
		</div>
	</div>
</section>

<style>
	section.cursor-list {
		padding: 0;
		background: white;
		border: 1px solid #dfe7e2;
		border-radius: 14px;
		margin: 20px 0;
		min-width: 0;
	}
	.list-scroll {
		overflow: visible;
		position: relative;
		isolation: isolate;
	}
	.list-controls {
		left: 0;
		position: sticky;
		top: 0;
		z-index: 4;
		background: #fff;
		border-radius: 14px 14px 0 0;
	}
	.cursor-list .list-toolbar {
		padding: 16px 18px;
		border-bottom: 1px solid #e5ece7;
	}
	.list-table-scroll {
		overflow-x: auto;
	}
	.list-column-clip {
		border-bottom: 1px solid #e1e8e3;
		overflow: clip;
		background: #f6f9f6;
	}
	.list-column-headings {
		display: flex;
		width: max-content;
	}
	.list-column-headings > div {
		flex-shrink: 0;
		padding: 11px 16px;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		white-space: nowrap;
		font-weight: 600;
		box-sizing: border-box;
	}
	.native-head th {
		position: static;
		height: 0;
		line-height: 0;
		padding: 0 16px;
		border: 0;
		opacity: 0;
		min-width: 120px;
	}
	.native-head th:last-child {
		min-width: 100px;
	}
	tbody.loading {
		opacity: 0.45;
		pointer-events: none;
	}
	.list-title {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-bottom: 12px;
	}
	.list-title strong {
		font-size: 16px;
	}
	.list-title span {
		background: #edf4ef;
		border-radius: 6px;
		padding: 3px 8px;
		font-size: 12px;
		color: #337658;
	}
	.list-title small {
		margin-left: auto;
		color: #7b8b82;
	}
	.list-filters {
		display: flex;
		gap: 12px;
		flex-wrap: wrap;
		align-items: flex-end;
	}
	.list-filters label,
	.list-pagination label {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 5px;
		font-size: 11px;
		color: #687c70;
		font-weight: 600;
	}
	.list-filters input,
	.list-filters select,
	.list-pagination select {
		height: 39px;
		border: 1px solid #dce5df;
		border-radius: 7px;
		background: white;
		padding: 8px 11px;
		color: #2a4034;
		font: inherit;
		font-size: 13px;
		width: auto;
		min-width: 130px;
	}
	.list-search {
		flex: 1;
		min-width: 180px;
		max-width: 360px;
	}
	.list-search input {
		width: 100%;
	}
	.list-filters button {
		margin: 0;
		min-height: 39px;
		font-size: 12px;
		padding: 8px 12px;
	}
	.cursor-list table {
		width: 100%;
		border-collapse: collapse;
		border-spacing: 0;
		margin: 0;
	}
	thead th {
		background: #f6f9f6;
		border-bottom: 1px solid #dfe7e2;
		padding: 11px 16px;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		text-align: left;
		white-space: nowrap;
	}
	.cursor-list td {
		border: 0;
		white-space: normal;
		line-height: 18px;
		padding: 8px 16px;
		border-bottom: 1px solid #edf1ee;
		font-size: 12px;
		max-width: 420px;
		overflow-wrap: anywhere;
	}
	td.amount {
		white-space: nowrap;
	}
	.list-empty {
		text-align: center;
		padding: 52px 20px;
		color: #76877d;
	}
	.list-badge {
		display: inline-block;
		border-radius: 6px;
		padding: 4px 8px;
		font-size: 12px;
		background: #f4f0e4;
		color: #937335;
		white-space: nowrap;
	}
	.list-badge.positive {
		background: #eaf5ed;
		color: #3e7d58;
	}
	.list-badge.danger {
		background: #fbeeea;
		color: #a95947;
	}
	.list-actions {
		display: flex;
		gap: 8px;
		align-items: center;
		flex-wrap: nowrap;
		min-width: 30px;
	}
	.cursor-list .native-head th.action-heading {
		width: 88px;
		min-width: 88px;
	}
	.cursor-list td.actions-cell {
		width: 88px;
		white-space: nowrap;
	}
	.cursor-list tbody tr:last-child td {
		border-bottom: 0;
	}
	.cursor-list tbody tr:hover td {
		background: #f8fbf9;
	}
	.cursor-list .list-order-reference {
		margin-top: 2px;
		padding: 1px 5px;
		font-size: 10px;
		line-height: 14px;
	}
	.cursor-list td.amount {
		white-space: nowrap;
	}
	.cursor-list :global(.product-letter),
	.cursor-list :global(.product-cell img) {
		width: 32px;
		height: 32px;
		flex-shrink: 0;
	}
	.cursor-list :global(.product-cell small) {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 240px;
		margin-top: 2px;
		line-height: 15px;
	}
	.list-pagination {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 18px;
		gap: 16px;
		border-top: 1px solid #e1e9e3;
		font-size: 12px;
		color: #738278;
	}
	.list-pagination label {
		flex-direction: row;
		align-items: center;
	}
	.list-pagination select {
		min-width: 60px;
	}
	.list-pagination > div {
		display: flex;
		gap: 8px;
	}
	.list-pagination button {
		margin: 0;
		padding: 8px 14px;
		font-size: 12px;
	}
	button:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}
	.list-error {
		padding: 12px 18px;
		background: #fff0eb;
		color: #9a4936;
	}
	.list-error button {
		margin-left: 12px;
	}
	@media (max-width: 700px) {
		.cursor-list .list-toolbar {
			padding: 16px;
		}
		.list-filters {
			gap: 8px;
		}
		.list-filters label {
			flex: 1;
			min-width: 120px;
		}
		.list-search {
			max-width: none;
			flex-basis: 100% !important;
		}
		.list-title {
			flex-wrap: wrap;
		}
		.list-title small {
			margin-left: 0;
		}
		.list-pagination {
			flex-wrap: wrap;
			padding: 12px 16px;
		}
		.list-pagination > div {
			margin-left: auto;
		}

		.cursor-list td {
			padding: 8px 14px;
		}
		.native-head th {
			padding: 0 16px;
		}
		.list-column-headings > div {
			padding: 11px 14px;
		}
	}
</style>
