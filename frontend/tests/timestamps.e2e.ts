import { expect, test } from '@playwright/test';

test.use({
	baseURL: 'http://localhost:4173',
	launchOptions: { channel: 'chrome' },
	timezoneId: 'America/New_York'
});

const metadata = {
	created_at: '2026-11-13 23:45:12.123456',
	updated_at: '2026-11-14 08:15:30.654321'
};
const base = {
	access: { invoices: true, payments: true, delivery: true },
	lines: [],
	details: [],
	payments: [],
	delivery_orders: [],
	purchase_orders: [],
	purchase_invoices: [],
	sales_invoices: [],
	invoices: [],
	total_product: 0,
	total_quantity: 0,
	recent_limit: 10
};
for (const kind of [
	'product',
	'purchase_order',
	'purchase_invoice',
	'sales_invoice',
	'payment',
	'delivery',
	'invoice_approval',
	'delivery_approval'
]) {
	test(`${kind} details show full record timestamps in WIB`, async ({ page }) => {
		await page.route('**/api/**', async (route) => {
			if (new URL(route.request().url()).pathname === '/api/auth/me')
				return route.fulfill({
					json: {
						id: 1,
						username: 'admin',
						role: 'Admin',
						permissions: [
							'products.read',
							'invoices.read',
							'payments.read',
							'delivery.read',
							'invoices.request_void'
						]
					}
				});
			const record = {
				...metadata,
				id: 1,
				name: 'Test product',
				number: 'TEST-001',
				reference: 'PAY-001',
				kind,
				status: 'Approved',
				date: '2026-11-14',
				person: 'Supplier',
				grand_total: 100,
				remaining_payment: 0,
				quantity: 1,
				remaining_quantity: 1,
				price: 100,
				reorder_level: 1,
				amount: 100,
				driver_name: 'Driver'
			};
			const json = kind.endsWith('_approval')
				? {
						...metadata,
						record_id: 1,
						record_number: 'TEST-001',
						status: 'Approved',
						reason: 'Review requested',
						requested_by: 'staff',
						reviewed_by: 'finance',
						date: '2026-11-14'
					}
				: {
						...base,
						[kind === 'product'
							? 'product'
							: kind === 'payment'
								? 'payment'
								: kind === 'delivery'
									? 'order'
									: 'document']: record
					};
			return route.fulfill({ json });
		});
		const path = kind.endsWith('_approval')
			? `/approvals/${kind === 'invoice_approval' ? 'invoices' : 'deliveries'}/1/`
			: `/${kind === 'product' ? 'products' : kind === 'payment' ? 'payments' : kind === 'delivery' ? 'delivery-orders' : 'documents'}/1/`;
		await page.goto(path);
		await expect(page.getByLabel('Record timestamps')).toContainText('13 Nov 2026, 23:45:12 WIB');
		await expect(page.getByLabel('Record timestamps')).toContainText('14 Nov 2026, 08:15:30 WIB');
		await expect(page.locator('time').first()).toHaveAttribute(
			'datetime',
			'2026-11-13T16:45:12.123Z'
		);
		await expect(page.getByRole('alert')).toHaveCount(0);
		if (kind.endsWith('_approval')) {
			await expect(page.getByText('finance', { exact: true })).toBeVisible();
			await expect(page.locator('.request-details a')).toHaveAttribute(
				'href',
				`/${kind === 'invoice_approval' ? 'documents' : 'delivery-orders'}/1/`
			);
		}
		await page.setViewportSize({ width: 390, height: 844 });
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
	});
}
