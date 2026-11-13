<script lang="ts">
	import type { Lifecycle } from './api';
	let { record }: { record: Lifecycle } = $props();
	function timestamp(value?: string | null) {
		if (!value) return null;
		// Database timestamps are returned in the database session's Jakarta timezone.
		const iso = value.includes('T') ? value : value.replace(' ', 'T') + '+07:00';
		const date = new Date(iso);
		if (Number.isNaN(date.getTime())) return null;
		return {
			iso: date.toISOString(),
			label:
				new Intl.DateTimeFormat('en-GB', {
					day: '2-digit',
					month: 'short',
					year: 'numeric',
					hour: '2-digit',
					minute: '2-digit',
					second: '2-digit',
					hourCycle: 'h23',
					timeZone: 'Asia/Jakarta'
				}).format(date) + ' WIB'
		};
	}
	let created = $derived(timestamp(record.created_at));
	let updated = $derived(timestamp(record.updated_at));
</script>

<dl class="record-timestamps" aria-label="Record timestamps">
	<div>
		<dt>Created at</dt>
		<dd>
			{#if created}<time datetime={created.iso}>{created.label}</time>{:else}Unavailable{/if}
		</dd>
	</div>
	<div>
		<dt>Updated at</dt>
		<dd>
			{#if updated}<time datetime={updated.iso}>{updated.label}</time>{:else}Unavailable{/if}
		</dd>
	</div>
</dl>

<style>
	.record-timestamps {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem 3rem;
		margin: -0.5rem 0 1.75rem;
		padding: 1rem 0;
		border-bottom: 1px solid #dfe7e2;
	}
	.record-timestamps div {
		display: grid;
		gap: 0.3rem;
	}
	dt {
		font-size: 0.8rem;
		color: #65796e;
	}
	dd {
		margin: 0;
		font-size: 0.9rem;
		color: #344b3f;
		font-variant-numeric: tabular-nums;
	}
</style>
