import { expect, test } from '@playwright/test';

test.use({ baseURL: 'http://localhost:4173', launchOptions: { channel: 'chrome' } });

test.beforeEach(async ({ page }) => {
	await page.route('**/api/**', async (route) => {
		const url = new URL(route.request().url());
		if (url.pathname === '/api/auth/me')
			return route.fulfill({
				json: {
					id: 1,
					username: 'admin',
					role: 'Admin',
					permissions: [
						'invoices.read',
						'invoices.write',
						'products.read',
						'products.write',
						'invoices.request_void',
						'invoices.void',
						'payments.read',
						'delivery.read',
						'delivery.write',
						'delivery.approve',
						'roles.manage'
					]
				}
			});
		if (url.pathname.startsWith('/api/lists/')) {
			const offset = url.searchParams.get('cursor') === 'page-2' ? 25 : 0;
			const filtered = url.searchParams.has('kind') || url.searchParams.has('from');
			const total = filtered ? 30 : 70;
			const items = Array.from({ length: Math.min(25, total - offset) }, (_, i) => ({
				id: offset + i + 1,
				name: `Product ${offset + i + 1}`,
				description: 'A product description long enough to check that catalog rows remain compact.',
				photo: null,
				purchase_order_id: 10,
				purchase_order_number:
					url.pathname.endsWith('purchase-invoices') && i % 2 === 0 ? 'PO-2026-0010' : null,
				quantity: 50,
				remaining_quantity: 20,
				reorder_level: 10,
				price: 100000,
				person: 'Customer / supplier',
				grand_total: 100000,
				remaining_payment: 50000,
				permissions: ['invoices.read', 'products.read'],
				role_id: 1,
				role_name: 'Staff',
				active: true,
				risk: 'Healthy',
				daily_sales: 1,
				days_remaining: 20,
				projected_stock: 10,
				suggested_order: 5,
				number: `DO-${String(offset + i + 1).padStart(6, '0')}`,
				sales_invoice_id: 41,
				sales_invoice_number: 'INV-2026-0041',
				driver_name: 'Driver',
				address: 'Jakarta, Indonesia',
				note: '',
				cancellation_note: '',
				reference: `PAY-${offset + i + 1}`,
				date: '2026-11-10',
				kind: url.searchParams.get('kind') || 'purchase_invoice',
				amount: 100000,
				invoice_count: 2,
				invoice_id: 41,
				invoice_number: 'INV-2026-0041',
				delivery_order_id: 24,
				delivery_order_number: 'DO-000024',
				requested_by: 'staff',
				reviewed_by: null,
				reason: 'Please review',
				status: url.pathname.endsWith('delivery-orders')
					? ['On Delivery', 'Delivered', 'Draft', 'Approved', 'Cancelled'][i % 5]
					: 'Pending'
			}));
			return route.fulfill({
				json: {
					items,
					total,
					limit: 25,
					has_more: offset + 25 < total,
					next_cursor: offset + 25 < total ? 'page-2' : null,
					start_cursor: 'start',
					summary: null
				}
			});
		}
		if (url.pathname === '/api/roles')
			return route.fulfill({
				json: {
					roles: [{ id: 1, name: 'Staff', permissions: ['invoices.read'] }],
					permissions: ['invoices.read']
				}
			});
		if (url.pathname === '/api/users')
			return route.fulfill({ json: [{ id: 1, username: 'staff', role_id: 1, active: true }] });
		return route.fulfill({ json: [] });
	});
});

test('payment pagination resets with filters and keeps controls and headings visible', async ({
	page
}) => {
	await page.goto('/?view=Payments');
	const list = page.locator('.cursor-list');
	await expect(list.locator('tbody tr')).toHaveCount(25);
	await page.getByRole('button', { name: 'Next', exact: true }).click();
	await expect(list.locator('tbody tr').first()).toContainText('PAY-26');
	await page.getByLabel('Payment type', { exact: true }).selectOption('sales_invoice');
	await expect(list.locator('tbody tr').first()).toContainText('PAY-1');
	await expect(page.getByRole('button', { name: 'Previous', exact: true })).toBeDisabled();
	await expect(list.locator('tbody tr').first()).toContainText('Customer receipt');
	await page.getByLabel('Date filter', { exact: true }).selectOption('custom');
	const filtered = page.waitForResponse(
		(r) =>
			r.url().includes('/api/lists/payments') &&
			r.url().includes('from=2026-11-01') &&
			r.url().includes('to=2026-11-14')
	);
	await page.getByLabel('From', { exact: true }).fill('2026-11-01');
	await page.getByLabel('To', { exact: true }).fill('2026-11-14');
	await filtered;
	await expect(list.locator('tbody tr')).toHaveCount(25);
	await page.evaluate(() => window.scrollTo(0, 600));
	const positions = await list.evaluate((el) => ({
		toolbar: el.querySelector('.list-controls')!.getBoundingClientRect().top,
		bottom: el.querySelector('.list-controls')!.getBoundingClientRect().bottom,
		heading: el.querySelector('.list-column-headings')!.getBoundingClientRect().bottom,
		nestedScroll: Array.from(el.querySelectorAll('*')).some((node) => {
			const css = getComputedStyle(node);
			return (
				['auto', 'scroll'].includes(css.overflowY) && node.scrollHeight > node.clientHeight + 1
			);
		})
	}));
	expect(await page.evaluate(() => window.scrollY)).toBeGreaterThan(0);
	expect(Math.abs(positions.toolbar)).toBeLessThan(2);
	expect(Math.abs(positions.heading - positions.bottom)).toBeLessThan(2);
	expect(positions.nestedScroll).toBe(false);
	await page.getByRole('button', { name: 'Next', exact: true }).click();
	await expect(list.locator('tbody tr').first()).toContainText('PAY-26');
	await expect
		.poll(async () => Math.round((await list.boundingBox())!.y))
		.toBeGreaterThanOrEqual(0);
	await page.setViewportSize({ width: 390, height: 844 });
	expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
});

test('approval submenus switch resources and show readable record numbers', async ({ page }) => {
	await page.goto('/approvals/?section=invoices');
	const list = page.locator('.cursor-list');
	await expect(list.getByRole('link', { name: 'INV-2026-0041' }).first()).toHaveAttribute(
		'href',
		'/documents/41/'
	);
	await expect(list.locator('tbody tr').first()).toContainText('staff');
	await page
		.locator('.sidebar-submenu')
		.getByRole('link', { name: 'Delivery cancellations' })
		.click();
	await expect(page).toHaveURL(/section=deliveries/);
	await expect(list.getByRole('link', { name: 'DO-000024' }).first()).toHaveAttribute(
		'href',
		'/delivery-orders/24/'
	);
	await expect(list.locator('tbody tr')).toHaveCount(25);
});

test('delivery actions stay compact and retain labeled, permission-aware transitions', async ({
	page
}) => {
	await page.goto('/delivery-orders/');
	const rows = page.locator('.cursor-list tbody tr');
	await expect(rows).toHaveCount(25);
	await expect(rows.first().getByRole('link', { name: 'View DO-000001' })).toHaveAttribute(
		'href',
		'/delivery-orders/1/'
	);
	await expect(rows.nth(1).getByRole('button', { name: /More actions/ })).toHaveCount(0);
	const firstHeight = (await rows.first().boundingBox())!.height;
	expect(firstHeight).toBeLessThan(100);
	await rows.first().getByRole('button', { name: 'More actions for DO-000001' }).click();
	const menu = page.getByRole('menu', { name: 'Actions for DO-000001' });
	await expect(menu).toBeVisible();
	await expect(menu.getByRole('menuitem', { name: 'Mark delivered' })).toBeVisible();
	await expect(menu.getByRole('menuitem', { name: 'Cancel delivery' })).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(menu).not.toBeVisible();
	await expect(rows.first().getByRole('button', { name: /More actions/ })).toBeFocused();
	await rows
		.first()
		.getByRole('button', { name: /More actions/ })
		.click();
	const refreshed = page.waitForResponse((r) => r.url().includes('/api/lists/delivery-orders'));
	const mutation = page.waitForRequest(
		(r) => r.url().endsWith('/delivery-orders/1/status') && r.method() === 'POST'
	);
	await menu.getByRole('menuitem', { name: 'Mark delivered' }).click();
	expect((await mutation).postDataJSON()).toEqual({ status: 'Delivered', cancellation_note: '' });
	await refreshed;
	await expect(page.locator('.cursor-list tbody.loading')).toHaveCount(0);
	await expect(menu).not.toBeVisible();
	await rows
		.nth(2)
		.getByRole('button', { name: /More actions/ })
		.click();
	const draftMenu = page.getByRole('menu', { name: 'Actions for DO-000003' });
	await expect(draftMenu.getByRole('menuitem', { name: 'Edit draft' })).toBeVisible();
	await expect(draftMenu.getByRole('menuitem', { name: 'Approve delivery' })).toBeVisible();
	await page.mouse.click(80, 100);
	await expect(draftMenu).not.toBeVisible();
});

test('all lists fill the content width and use aligned, compact rows', async ({ page }) => {
	await page.setViewportSize({ width: 1440, height: 1000 });
	for (const path of [
		'/?view=Products',
		'/?view=Purchase%20orders',
		'/?view=Purchase%20invoices',
		'/?view=Sales%20invoices',
		'/?view=Payments',
		'/?view=Stock%20forecast',
		'/delivery-orders/',
		'/approvals/?section=invoices',
		'/approvals/?section=deliveries',
		'/roles/',
		'/roles/?section=assignments'
	]) {
		await page.goto(path);
		const list = page.locator('.cursor-list');
		await expect(list.locator('tbody tr')).toHaveCount(25);
		await expect(list.locator('tbody.loading')).toHaveCount(0);
		const metrics = await list.evaluate((el) => {
			const main = el.closest('main')!;
			const rect = el.getBoundingClientRect();
			const mainRect = main.getBoundingClientRect();
			const native = Array.from(el.querySelectorAll('thead th'));
			const visual = Array.from(el.querySelectorAll('.list-column-headings>div'));
			return {
				mainWidth: mainRect.width,
				listWidth: rect.width,
				toolbarPadding: parseFloat(
					getComputedStyle(el.querySelector('.list-toolbar')!).paddingLeft
				),
				maxHeight: Math.max(
					...Array.from(el.querySelectorAll('tbody tr')).map(
						(row) => row.getBoundingClientRect().height
					)
				),
				alignment: native.map((cell, i) =>
					Math.abs(cell.getBoundingClientRect().width - visual[i].getBoundingClientRect().width)
				),
				borders: Array.from(el.querySelectorAll('tbody tr:first-child td')).map(
					(cell) => getComputedStyle(cell).borderBottomWidth
				),
				textActions: el.querySelectorAll('td .button,td .text-link').length
			};
		});
		expect(metrics.mainWidth, path).toBeCloseTo(1202, 0);
		expect(metrics.listWidth, path).toBeGreaterThan(metrics.mainWidth - 51);
		expect(metrics.toolbarPadding, path).toBeGreaterThanOrEqual(16);
		expect(metrics.maxHeight, path).toBeLessThanOrEqual(58);
		expect(Math.max(...metrics.alignment), path).toBeLessThan(1);
		expect(
			metrics.borders.every((v) => v === '1px'),
			path
		).toBe(true);
		expect(metrics.textActions, path).toBe(0);
	}
});

test('approval menu retains review actions without increasing row height', async ({ page }) => {
	await page.goto('/approvals/?section=invoices');
	const row = page.locator('.cursor-list tbody tr').first();
	await expect(row.getByRole('link', { name: 'View INV-2026-0041' })).toBeVisible();
	await row.getByRole('button', { name: 'More actions for INV-2026-0041' }).click();
	const menu = page.getByRole('menu', { name: 'Actions for INV-2026-0041' });
	await expect(menu.getByRole('menuitem', { name: 'Approve void' })).toBeVisible();
	await expect(menu.getByRole('menuitem', { name: 'Reject request' })).toBeVisible();
	const review = page.waitForRequest(
		(r) => r.url().endsWith('/void-requests/1/approve') && r.method() === 'POST'
	);
	await menu.getByRole('menuitem', { name: 'Approve void' }).click();
	expect((await review).postDataJSON()).toEqual({});
	await expect(menu).not.toBeVisible();
});

test('narrow lists stay within the viewport and synchronize horizontal headings', async ({
	page
}) => {
	await page.setViewportSize({ width: 390, height: 844 });
	for (const path of [
		'/?view=Products',
		'/?view=Purchase%20invoices',
		'/?view=Payments',
		'/approvals/',
		'/delivery-orders/'
	]) {
		await page.goto(path);
		const list = page.locator('.cursor-list');
		await expect(list.locator('tbody tr')).toHaveCount(25);
		const scroll = list.locator('.list-table-scroll');
		await scroll.evaluate((el) => (el.scrollLeft = el.scrollWidth));
		expect(await page.evaluate(() => document.documentElement.scrollWidth), path).toBe(390);
		await expect
			.poll(() =>
				list.evaluate((el) => {
					const native = el.querySelector('thead th')!.getBoundingClientRect();
					const sticky = el.querySelector('.list-column-headings>div')!.getBoundingClientRect();
					return Math.abs(native.left - sticky.left);
				})
			)
			.toBeLessThan(1);
		expect(
			await scroll.evaluate((el) => el.scrollHeight - el.clientHeight),
			path
		).toBeLessThanOrEqual(1);
	}
});
