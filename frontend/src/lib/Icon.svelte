<script lang="ts">
	let { name, size = 19 }: { name: string; size?: number } = $props();
	const paths: Record<string, string> = {
		eye: 'M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12 M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0',
		more: 'M5 12h.01 M12 12h.01 M19 12h.01',
		edit: 'M16 3l5 5-12 12-6 1 1-6z M14 5l5 5',
		dashboard: 'M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z',
		forecast: 'M3 17l6-6 4 3 8-10 M15 4h6v6 M3 21h18',
		products: 'M3 7l9-5 9 5v10l-9 5-9-5z M3 7l9 5 9-5 M12 12v10 M7 4l10 5',
		order: 'M7 3h10v4H7z M7 5H4v17h16V5h-3 M8 12h8 M8 16h8',
		invoice: 'M6 3h12v18l-3-2-3 2-3-2-3 2z M9 7h6 M9 11h6 M9 15h3',
		payment: 'M3 7h17l-4-4 M21 17H4l4 4',
		sale: 'M4 3h16v18l-4-2-4 2-4-2-4 2z M8 7h8 M8 11h8 M8 15h4',
		delivery:
			'M3 5h11v12H3z M14 9h4l3 4v4h-7 M7 21a2 2 0 1 0 0-4 2 2 0 0 0 0 4 M18 21a2 2 0 1 0 0-4 2 2 0 0 0 0 4',
		roles: 'M12 3l8 3v6c0 5-8 9-8 9s-8-4-8-9V6z M8 12l3 3 5-6',
		users:
			'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8 M17 4a4 4 0 0 1 0 8 M22 21v-2a4 4 0 0 0-3-4',
		spark: 'M12 2l3 7 7 3-7 3-3 7-3-7-7-3 7-3z',
		refresh: 'M20 7v5h-5 M20 12a8 8 0 1 0-2 6',
		arrow: 'M5 12h14 M13 6l6 6-6 6',
		export: 'M12 3v12 M7 10l5 5 5-5 M4 16v5h16v-5',
		clock: 'M12 8v5l3 2 M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0',
		check: 'M5 12l4 4L19 6',
		close: 'M6 6l12 12 M6 18L18 6',
		search: 'M21 21l-5-5 M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0',
		flag: 'M5 22V3 M5 3h14l-3 5 3 5H5',
		up: 'M6 18L18 6 M6 6h12v12',
		down: 'M6 6l12 12 M6 18h12V6'
	};
</script>

<svg
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width="1.6"
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"><path d={paths[name] || paths.products} /></svg
>
