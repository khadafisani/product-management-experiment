<script lang="ts">
	import { money } from './api';
	let {
		title,
		records,
		type = 'document',
		empty = 'No related records yet.'
	}: {
		title: string;
		records: any[];
		type?: 'document' | 'payment' | 'delivery';
		empty?: string;
	} = $props();
</script>

<section class="related-panel">
	<div class="related-heading">
		<h2>{title}</h2>
		<span class="related-count">{records.length}</span>
	</div>
	{#if records.length}<div class="table-wrap">
			<table>
				<thead
					><tr
						><th
							>{type === 'payment'
								? 'Reference'
								: type === 'delivery'
									? 'Delivery order'
									: 'Document'}</th
						><th>Date</th><th
							>{type === 'payment'
								? 'Allocated amount'
								: type === 'delivery'
									? 'Driver'
									: 'Customer / supplier'}</th
						>{#if type !== 'payment'}<th>Status</th
							>{/if}{#if records.some((r) => r.quantity !== undefined)}<th>Quantity</th
							>{/if}{#if records.some((r) => r.allocated_amount !== undefined)}<th
								>Allocated amount</th
							>{/if}</tr
					></thead
				><tbody
					>{#each records as r}<tr
							><td
								><a
									class="record-link"
									href={type === 'payment'
										? `/payments/${r.id}/`
										: type === 'delivery'
											? `/delivery-orders/${r.id}/`
											: `/documents/${r.id}/`}
									>{r.number || r.reference || `DO-${String(r.id).padStart(6, '0')}`}</a
								></td
							><td>{r.date}</td><td
								>{type === 'payment'
									? money(r.amount)
									: type === 'delivery'
										? r.driver_name
										: r.person}</td
							>{#if type !== 'payment'}<td><span class="badge">{r.status}</span></td
								>{/if}{#if records.some((r) => r.quantity !== undefined)}<td>{r.quantity ?? '—'}</td
								>{/if}{#if records.some((r) => r.allocated_amount !== undefined)}<td
									>{money(r.allocated_amount ?? 0)}</td
								>{/if}</tr
						>{/each}</tbody
				>
			</table>
		</div>{:else}<p class="related-empty">{empty}</p>{/if}
</section>
