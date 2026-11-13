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
export async function api<T>(path: string, body?: unknown): Promise<T> {
	const response = await fetch(`/api${path}`, {
		method: body === undefined ? 'GET' : 'POST',
		headers: { 'Content-Type': 'application/json' },
		...(body === undefined ? {} : { body: JSON.stringify(body) })
	});
	const data = await response.json().catch(() => ({
		error: 'The API is unavailable. Start the Rust backend and check its database configuration.'
	}));
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
