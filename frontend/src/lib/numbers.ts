const groups = ['', 'thousand', 'million', 'billion', 'trillion', 'quadrillion', 'quintillion'];
/** Compact dashboard currency; detailed accounting views retain exact amounts. */
export function shortMoney(amount: number): string {
	if (!Number.isFinite(amount)) return 'Rp 0';
	let group = Math.min(
		Math.floor(Math.log10(Math.max(1, Math.abs(amount))) / 3),
		groups.length - 1
	);
	let scaled = amount / 1000 ** group;
	if (Math.abs(Math.round(scaled * 10) / 10) >= 1000 && group < groups.length - 1) {
		group++;
		scaled = amount / 1000 ** group;
	}
	if (group === 0)
		return `Rp ${new Intl.NumberFormat('id-ID', { maximumFractionDigits: 0 }).format(amount)}`;
	return `Rp ${new Intl.NumberFormat('en', { maximumFractionDigits: 1 }).format(scaled)} ${groups[group]}`;
}
