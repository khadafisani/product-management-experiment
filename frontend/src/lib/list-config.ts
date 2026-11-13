export type Column = {
	key: string;
	label: string;
	type?: string;
	href?: (row: any) => string;
	format?: (value: any, row: any) => string;
};
export const productColumns: Column[] = [
	{ key: 'name', label: 'Product', type: 'product', href: (r) => `/products/${r.id}/` },
	{ key: 'quantity', label: 'Physical stock' },
	{ key: 'remaining_quantity', label: 'Available' },
	{ key: 'price', label: 'Unit price', type: 'money' },
	{ key: 'reorder_level', label: 'Reorder level' },
	{ key: 'date', label: 'Created' },
	{ key: 'status', label: 'Status', type: 'badge' }
];
export const paymentColumns: Column[] = [
	{ key: 'reference', label: 'Reference', href: (r) => `/payments/${r.id}/` },
	{ key: 'date', label: 'Date' },
	{
		key: 'kind',
		label: 'Type',
		format: (v) => (v === 'sales_invoice' ? 'Customer receipt' : 'Supplier payment')
	},
	{
		key: 'invoice_count',
		label: 'Invoices',
		href: (r) => `/payments/${r.id}/`,
		format: (v) => (v === null ? 'View details' : `${v} invoice${v === 1 ? '' : 's'}`)
	},
	{ key: 'amount', label: 'Amount', type: 'money' }
];
export function documentColumns(orders: boolean): Column[] {
	return [
		{
			key: 'number',
			label: orders ? 'Order number' : 'Invoice number',
			type: 'document',
			href: (r) => `/documents/${r.id}/`
		},
		{ key: 'person', label: 'Person / customer' },
		{ key: 'date', label: 'Date' },
		{ key: 'grand_total', label: 'Grand total', type: 'money' },
		...(!orders ? [{ key: 'remaining_payment', label: 'Outstanding', type: 'money' }] : []),
		{ key: 'status', label: 'Status', type: 'badge' }
	];
}
export const deliveryColumns: Column[] = [
	{ key: 'number', label: 'Delivery order', href: (r) => `/delivery-orders/${r.id}/` },
	{
		key: 'sales_invoice_number',
		label: 'Sales invoice',
		href: (r) => `/documents/${r.sales_invoice_id}/`
	},
	{ key: 'date', label: 'Date' },
	{ key: 'driver_name', label: 'Driver' },
	{ key: 'address', label: 'Address' },
	{ key: 'status', label: 'Status', type: 'badge' }
];
export const forecastColumns: Column[] = [
	{ key: 'name', label: 'Product', href: (r) => `/products/${r.id}/` },
	{ key: 'quantity', label: 'Available' },
	{ key: 'daily_sales', label: 'Daily sales', format: (v) => Number(v).toFixed(2) },
	{
		key: 'days_remaining',
		label: 'Coverage',
		format: (v) => (v === null ? 'No recent sales' : `${Number(v).toFixed(1)} days`)
	},
	{ key: 'projected_stock', label: 'Projected stock', format: (v) => Number(v).toFixed(0) },
	{ key: 'suggested_order', label: 'Suggested order' },
	{ key: 'risk', label: 'Status', type: 'badge' }
];
