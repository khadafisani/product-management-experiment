export type Product = {
	id: number;
	name: string;
	quantity: number;
	remaining_quantity: number;
	status: string;
	photo: string | null;
	description: string;
	price: number;
	reorder_level: number;
};
export type Document = {
	id: number;
	kind: string;
	number: string;
	date: string;
	person: string;
	purchase_order_id: number | null;
	grand_total: number;
	remaining_payment: number;
	status: string;
};
export type Payment = {
	id: number;
	reference: string;
	date: string;
	amount: number;
	kind: string;
	invoices: string;
};
export type Forecast = {
	id: number;
	name: string;
	quantity: number;
	daily_sales: number;
	days_remaining: number | null;
	projected_stock: number;
	suggested_order: number;
	risk: string;
};
export type Line = {
	product_id: number;
	quantity: number;
	price: number;
	item_name?: string;
	realized_quantity?: number;
	subtotal?: number;
	status?: string;
};
let refreshing: Promise<boolean> | null = null;
const pendingKeys = new Map<string, string>();
export async function api<T>(path: string, body?: unknown): Promise<T> {
	const serialized = body === undefined ? undefined : JSON.stringify(body);
	const fingerprint = path + ':' + serialized;
	const mutation = body !== undefined && !path.startsWith('/auth/') && !path.endsWith('/preview');
	let key = pendingKeys.get(fingerprint);
	if (mutation && !key) {
		key = crypto.randomUUID();
		pendingKeys.set(fingerprint, key);
	}
	const request = () =>
		fetch(`/api${path}`, {
			method: body === undefined ? 'GET' : 'POST',
			credentials: 'include',
			headers: { 'Content-Type': 'application/json', ...(key ? { 'Idempotency-Key': key } : {}) },
			...(serialized === undefined ? {} : { body: serialized })
		});
	let response = await request();
	if (response.status === 401 && !path.startsWith('/auth/')) {
		if (!refreshing)
			refreshing = fetch('/api/auth/refresh', {
				method: 'POST',
				credentials: 'include',
				headers: { 'Content-Type': 'application/json' },
				body: '{}'
			})
				.then((r) => r.ok)
				.finally(() => {
					refreshing = null;
				});
		if (await refreshing) response = await request();
		else {
			if (typeof window !== 'undefined') window.dispatchEvent(new Event('auth-expired'));
		}
	}
	const data = await response.json().catch(() => ({ error: 'The API is unavailable.' }));
	// Keep the key after network failures, pending responses and server errors.
	if (mutation && response.status < 500 && response.status !== 409 && response.status !== 401)
		pendingKeys.delete(fingerprint);
	if (!response.ok) throw new Error(data.error || 'Request failed');
	return data;
}
export const money = (n: number) =>
	new Intl.NumberFormat('id-ID', {
		style: 'currency',
		currency: 'IDR',
		maximumFractionDigits: 0
	}).format(n);
export const compact = (n: number) =>
	new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 }).format(n);
export const today = () =>
	new Intl.DateTimeFormat('en-CA', {
		timeZone: 'Asia/Jakarta',
		year: 'numeric',
		month: '2-digit',
		day: '2-digit'
	}).format(new Date());
