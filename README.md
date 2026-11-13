# Stockroom

Product management with Svelte 5 / SvelteKit, Rust / Axum, MySQL, and Meilisearch's HTTP API (including dev mode with no API key).

## Run locally

MySQL on 3306 and Meilisearch on 7700 are expected. The local `product_management` database is already created. The ignored `backend/.env` uses root with no password on localhost; change it if your local credentials change. No sample business records are inserted.

In one terminal:

```powershell
cd backend
cargo run
```

In another:

```powershell
cd frontend
bun install
bun run dev
```

Open http://localhost:5173. Vite proxies `/api` to Rust on 3000. SQL migrations run automatically on backend startup. Database creation is separate from migrations.

## Production build and run

```powershell
cd frontend
bun run build
cd ../backend
cargo build --release
./target/release/backend.exe
```

Open http://127.0.0.1:3000. The release server serves the compiled Svelte app and API together; a Node server is not required. Run from `backend` so `.env` and the default `../frontend/build` directory resolve. For another working directory, supply environment variables including an absolute `STATIC_DIR`. Deploy the executable, `frontend/build`, and environment configuration together. The executable embeds the SQL migrations. Windows builds run on Windows; build on the target platform for Linux deployment.

`backend/.env.example` documents DATABASE_URL, MEILI_URL, optional MEILI_KEY, BIND_ADDRESS, FRONTEND_ORIGIN, and STATIC_DIR. Root/no-password and unauthenticated Meilisearch are the user's local development settings. Authentication and permission-based roles are enforced on API endpoints; the app binds to loopback by default. See the authentication section below.

## Demo seed data

The demo dataset has been loaded locally. To run it on another database or safely rerun:

```powershell
cd backend
cargo run --bin seed
```

It uses `backend/.env`, applies migrations, and inserts everything in one transaction. Existing business records are preserved. Demo product names start with `[Demo]`; document/payment references start with `DEMO-`. A second run skips an already loaded dataset, including after you interact with its records. It does not reset stock, erase records, or refresh transaction dates. The seed checks its namespace and stops if a partial/conflicting demo dataset exists.

| Table | Demo rows |
|---|---:|
| products | 20 |
| documents | 40 (8 POs, 8 purchase invoices, 24 sales invoices) |
| document_lines | 40 |
| payments | 20 (6 supplier payments, 14 customer receipts) |
| payment_allocations | 24 |
| stock_movements | 50 |

Dates are relative to the seeding day in Asia/Jakarta: opening inventory 180 days earlier, supplier orders/invoices across five months, and sales within the last 30 days. Twelve products have recent demand; seven active products have no sales history; one product is inactive. Stock quantities deliberately include zero, low, and healthy levels. All 18 nonzero opening balances and 32 invoice stock movements reconcile to current inventory. PO examples cover Open, Partial, Complete, and Warning; invoices cover Unpaid, Partial Paid, and Paid. Payments include multiple invoices in one payment and multiple payments against one invoice.

At the default 30-day horizon / 7-day lead time, immediately after seeding, the forecast shows 5 products needing replenishment and 168 recommended units. Try a 60/90-day horizon to compare demand projections. Counts evolve naturally as you create more transactions, and forecasts change as sales age out of the rolling window.

For read-only reconciliation checks, execute [backend/seeds/verify.sql](backend/seeds/verify.sql) against the configured database. It checks stock ledgers, line/header totals, invoice balances and statuses, payment allocations/types, PO remaining quantities, and exact demo row counts. Every `violations` value should be zero. A release seeder can also be built with `cargo build --release --bin seed` and run as `./target/release/seed.exe` from `backend`.

## Backend structure

```text
backend/src/
  main.rs                       # executable entry point
  lib.rs                        # module tree and test registration
  config/                       # app state, MySQL/migrations, HTTP router, startup
  helper/                       # shared errors and date validation
  feature/
    product/
    purchase_order/
    purchase_invoice/
    sale_invoice/
    payment/
    forecast/
    search/
    document/                   # shared document persistence/preview coordinator
```

Each feature contains `entity/mod.rs`, `validation/mod.rs`, `controller.rs`, and `service.rs`. Controllers extract HTTP inputs and format responses; validation handles input rules; services implement transactions and business behavior. Entities define request/response/database types. PO realization belongs to the purchase-order service, receiving/paid-stock release to purchase invoices, sale deduction/cancellation to sales, and shared physical/available ledger changes to products. The document coordinator shares storage for the three document types and calls these domain services, avoiding duplicated transaction code.

Existing `/api/documents` clients remain supported. Type-specific list/create routes are also available at `/api/purchase-orders`, `/api/purchase-invoices`, and `/api/sales-invoices`.

## Remaining quantity and sale cancellation

- `products.quantity` is physical stock, including unpaid received purchases. `products.remaining_quantity` is available to sell; it stays between zero and physical stock.
- Opening/manual stock can specify `remaining_quantity`; omitting it defaults to physical quantity for a new product, or adjusts availability by the same physical delta when editing. Both deltas are recorded separately in `stock_movements` (`quantity` and `remaining_delta`).
- Posting a purchase invoice receives physical units and updates a linked PO immediately, but does not make the units available for sale. All quantities from its details become available when that invoice transitions to fully **Paid**. Partial payment releases no units. `documents.stock_released` prevents repeat release. Payment and availability changes commit in one transaction.
- Posting a sales invoice checks remaining quantity under a database row lock and deducts both physical and remaining stock. Concurrent sales cannot oversell. Product pickers show available quantities, sales inputs use that limit, and the API independently enforces it.
- `POST /api/sales-invoices/{id}/cancel` with `{"reason":"Customer cancelled"}` cancels a sale. It restores physical and remaining units exactly once, sets status **Cancelled**, closes the unpaid balance, preserves the original lines/totals/payment history, and records the cancellation reason/time. Repeating the request is idempotent. Cancelled sales are excluded from sales totals and forecast demand and cannot receive new payments.
- Payments already received on a cancelled sale are retained in the audit history and recorded as `refund_due` in `sales_invoice_cancellations`. Details and the confirmation form expose this amount. This app records the refund obligation; it does not transfer money or implement refund settlement.
- Forecasts, low-stock alerts, and replenishment suggestions use remaining quantity. The forecast API exposes `remaining_quantity` and retains `quantity` as an alias of the available quantity for existing forecast clients. Products expose both actual physical and remaining values.
- Upgrade migration `0002_remaining_quantity.sql` preserves existing physical stock as an available opening baseline and marks historical purchase invoices as already handled. Their later payment will not add stock again. This deliberately avoids retroactively removing availability from stock sold before this rule existed. The paid-only release rule applies to new purchase invoices. Demo fixtures use the same historical-baseline convention; fresh workflows obey the new rules.

## Workflows and rules

- Products have opening stock, price, reorder level, active/inactive status, optional photo URL, and description. Manual stock changes create ledger entries. Create products before entering transaction lines.
- Purchase orders store the plan, with one line per product. They do not add inventory. Product count and total quantity are derived from the details.
- `realized_quantity` means **remaining planned quantity**, following the requested deduction rule. It starts at ordered quantity. Linked purchase invoices subtract from it. Zero is Complete, positive is Partial, negative is Warning. Any negative detail makes the PO Warning.
- Purchase orders and purchase invoices can select an existing product or use **Enter a new product**. **Review & post document** shows a read-only preview of new names, prices, status, and physical/remaining stock after posting. **Confirm products & post** explicitly authorizes creation; **Back to edit** preserves the draft. Preview does not write data. Matching existing names (using MySQL's name collation) reuse the existing product. Newly created products start Active with reorder level 10. POs start at zero stock; purchase invoices receive physical units immediately, with remaining quantity zero until fully paid. Products and documents commit together, so failed posting leaves no orphan products. Sales continue to require existing products.
- Purchase invoices may be standalone or linked to a PO. They receive stock immediately. Product IDs match lines reliably even when names change, and names/prices are snapshotted. An item outside the original linked PO is recorded on that PO as an unplanned line: ordered quantity 0, remaining quantity negative, subtotal 0, status Warning. This preserves the original planned grand total while exposing the discrepancy.
- Products needing replenishment or out of stock have a **Purchase invoice** action in Stock forecast and Products, plus **Create purchase invoice** in dashboard stock alerts. It opens a standalone purchase invoice with the product, suggested quantity (at least 1), current unit price, date, and a suggested invoice number. Review the actual supplier price, enter the creator, and post to receive stock. Opening the action does not write anything.
- Sales invoices deduct stock immediately and reject insufficient inventory. Customer receipts can be entered through Payments and reduce `remaining_payment`.
- Payments use a many-to-many allocation table. Selection order determines allocation order. Multiple invoices require an amount strictly above the first selected invoice's **outstanding balance**. Every selected invoice must receive money; overpayments and duplicate selections are rejected. For two balances of Rp1,000,000, Rp1,200,000 allocates Rp1,000,000 and Rp200,000. Rp900,000 is allowed only against one of them.
- Money is stored as integer Indonesian Rupiah, without fractional rupiah. Quantities are whole units. All totals and statuses are calculated on the backend.
- Posted lines and payments preserve history. Sales cancellation is supported as described above; other correction/return/refund-settlement flows are not included. Product master records can be edited or deactivated.
- Stock and payment changes use MySQL transactions and row locks. Failed writes roll back together. Database uniqueness protects document numbers and payment references.
- Dashboard charts show monthly sales, purchasing, supplier payments, and product stock health. Dashboard totals cover all recorded data; the chart has its own 6/12-month window. CSV exports are available for each view.
- Search runs through the backend so a configured Meilisearch key stays private. It indexes products at startup and every 30 seconds, with an immediate UI-triggered refresh after saves. A rebuild button is available in Products. Indexing is eventually consistent; list filtering falls back locally when search is unavailable. MySQL remains the authoritative store. Index name: `pms_products`.
- Forecast uses non-cancelled sales in the last 30 calendar days, including today in Asia/Jakarta. Daily demand = sold quantity / 30. Coverage = remaining quantity / daily demand, or “No recent sales.” Projected stock = remaining quantity − demand × horizon. Suggested order = max(0, ceil(demand × (horizon + lead time) + reorder level − remaining quantity)). Assumes steady demand, includes zero-sale days, and excludes future incoming POs, seasonality, and lost demand during stockouts.

## API

| Method | Route | Purpose |
|---|---|---|
| GET | `/api/health` | Database/search connectivity |
| GET, POST | `/api/products` | List, create or edit product (`id` edits) |
| GET, POST | `/api/documents` | List/post PO or invoice |
| POST | `/api/documents/preview` | Read-only new product preview for a document draft |
| GET | `/api/documents/{id}` | Header, totals, details and payment history |
| POST | `/api/sales-invoices/{id}/cancel` | Cancel a sale, restore stock, record any refund due |
| GET, POST | `/api/payments` | List/post supplier payments or customer receipts |
| GET | `/api/forecast?days=30&lead_time=7` | Product forecasts |
| GET | `/api/search?q=keyboard` | Meilisearch product search |
| POST | `/api/search/reindex` | Queue product index refresh |

Documents share normalized `documents` and `document_lines` tables with a `kind` discriminator: `purchase_order`, `purchase_invoice`, `sales_invoice`. API field mapping: `number` = purchase_order_no / invoice_no / sales_invoice_no; `person` = pic_name / created_by / customer_name; `date` = date / invoice_date. Details include product IDs and snapshotted item names. The common schema avoids duplicating payment and line-item code while preserving the different business rules.

For new purchase products, omit `product_id` (or use 0) and provide `item_name`, `quantity`, and `price`. Preview with the same document body at `/api/documents/preview`. Its `new_products` list describes the products to be created. Post the reviewed body to `/api/documents` with `confirm_new_products: true` after confirmation; creation is rejected without that flag. The response includes `created_products`. Existing-ID clients remain supported. New product names alone on a sales invoice are rejected.

## Checks

```powershell
cd frontend
bun run check
bun run build
cd ../backend
cargo test
cargo test -- --include-ignored
```

The ignored integration test requires local root MySQL create/drop permission, creates a unique `stockroom_test_*` database, applies migrations, and removes it after success. Override `TEST_MYSQL_ADMIN_URL` for another local account. It verifies PO deductions/warnings, opening stock, invoice stock receipts, split payment statuses, rejected allocations, sale stock deductions, oversell rollback, customer receipts, forecast demand, and the stock ledger. Unit tests cover payment boundaries and amount validation.

Browser visual verification was unavailable in the task environment; Svelte checks, builds, database workflow tests, and HTTP smoke checks are used instead.

## Design

Paste [frontend/pen-dev-design-prompt.md](frontend/pen-dev-design-prompt.md) into pen.dev to generate a coordinated design for every application page and workflow.


## Authentication, permissions and delivery orders

Run `cargo run --bin seed` from `backend` to apply migrations and add `admin`, `staff`, and `finance`, each with password `asdqwe123!`. Existing passwords and role assignments are preserved. Account names are case insensitive. Change demo passwords on the Account page. The local ignored `.env` has been given a random JWT secret; other environments must set `JWT_SECRET` to at least 32 random bytes. `COOKIE_SECURE` defaults to true; use false only for local HTTP development. Set `FRONTEND_ORIGIN` to the exact browser origin, including protocol and port. Production requires HTTPS.

Access JWTs last 15 minutes. Refresh JWTs belong to a session with an absolute seven-day lifetime and rotate on every refresh. Tokens use HS256, a fixed issuer/audience, explicit token types and unique IDs. Cookies are host-only, HttpOnly, SameSite=Strict, and Secure by default; access cookies use `/api`, refresh cookies `/api/auth`. Tokens are never returned in JSON or stored in browser storage. Passwords use salted Argon2id hashes. JSON-only writes and origin/Fetch Metadata checks protect cookie-based operations against CSRF. Five consecutive failed logins lock the username for 15 minutes.

Every protected request checks the session and current database permissions. Logout revokes the session immediately, including access tokens; logout-all, password changes, user deactivation and role assignments revoke affected sessions. Reusing a rotated refresh token revokes its entire session. Concurrent refreshes therefore count as reuse: the frontend shares one refresh promise within a tab; separate tabs must coordinate refreshes or sign in again after a collision. The design follows [RFC 9700 refresh-token rotation guidance](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.14.2). An hourly job deletes expired sessions and their refresh-token history, completed idempotency records older than 24 hours, and old lockouts. Rotated-token history is retained until the family expires so cleanup cannot disable reuse detection.

| Role | Default permissions |
|---|---|
| Admin | Every permission, including products, users and roles |
| Staff | Read business data; create/edit invoices, purchase orders and delivery drafts; request invoice voids; dispatch approved delivery orders |
| Finance | Read business data; manage payments; void invoices and review Staff void requests; approve, dispatch and cancel delivery orders |

The Roles page supports creating roles, changing Staff/Finance/custom-role permissions, creating users, and assigning roles. The Admin role and your own role/active state are protected against accidental lockout. Seed reruns preserve permission edits to existing Staff/Finance roles.

Writes require an `Idempotency-Key` (1-128 ASCII characters), scoped to the authenticated user. Auth writes and document previews are exempt. A key is bound to the method, URI and exact request bytes; replay returns the original status/body, and a different payload returns 409. Concurrent requests use a database unique key to elect one executor. Completed requests are retained for at least 24 hours. The frontend keeps the same key after network failures. A crash or persistence failure may leave a pending record: retries return 409 and never execute the mutation again. Pending records are deliberately retained for manual reconciliation; confirm the business operation before removing one or using a new key. This favors preventing duplicates over silently replaying uncertain operations.

Delivery orders link to a **sales invoice**, as confirmed, and only Paid / Partial Paid invoices qualify. Details reference invoice-line IDs, with server-snapshotted product names, positive quantities and notes. The workflow is `Draft -> Approved -> On Delivery -> Delivered` (Approved can also go straight to Delivered). Finance approval reserves `on_delivery_quantity`; delivery moves the reservation to `delivered_quantity`. The reserved count includes approved orders awaiting dispatch. Drafts do not reserve stock, and approval rechecks the balance under the invoice lock, so competing drafts cannot over-deliver. Staff can request cancellation of approved/in-transit orders through the Approvals page; Finance approves or rejects the request. Cancellation releases the reservation; delivered orders cannot be cancelled. Delivery counts never deduct physical stock a second time, because invoice posting already records that movement.

Invoice metadata can be edited while active. Optional line edits are allowed only before payments or any delivery orders exist; they adjust the stock ledger and linked purchase-order quantities transactionally. Invoice voids retain invoice/payment history and a reason, reviewer and refund-due audit. Staff requests are reviewed by Finance; Finance/Admin may void directly. Cancel associated delivery orders first. Delivered invoices cannot be voided. Purchase voids reverse receipt/released stock only when the resulting inventory remains valid. Refund due is recorded; no cash refund is executed automatically.

### API additions

| Method | Path | Purpose |
|---|---|---|
| POST | `/api/auth/login` | Username/password login; sets cookies |
| POST | `/api/auth/refresh` | Rotate refresh and access JWTs |
| POST | `/api/auth/logout`, `/api/auth/logout-all` | Revoke current/all sessions and clear cookies |
| GET | `/api/auth/me` | Current user, role and permissions |
| POST | `/api/auth/password` | Verify current password, change password and revoke all sessions |
| GET/POST | `/api/roles`, `/api/users` | List/create roles or users (Admin permission) |
| POST | `/api/roles/{id}`, `/api/users/{id}/role` | Edit role permissions or assign user role/active state |
| POST | `/api/invoices/{id}` | Edit number/date/person and optional lines |
| POST | `/api/invoices/{id}/cancel` | Request or directly execute invoice void |
| GET | `/api/void-requests` | List approval requests |
| POST | `/api/void-requests/{id}/approve`, `/reject` | Finance review |
| POST | `/api/delivery-orders/{id}/cancel-request` | Staff requests cancellation with `{ "reason": "..." }` |
| GET | `/api/delivery-cancellation-requests` | List delivery cancellation approvals |
| POST | `/api/delivery-cancellation-requests/{id}/approve`, `/reject` | Finance review of delivery cancellation |
| GET/POST | `/api/delivery-orders` | List/create delivery orders |
| GET/POST | `/api/delivery-orders/{id}` | Detail/edit draft |
| POST | `/api/delivery-orders/{id}/approve`, `/status` | Finance approval or delivery transition |

Delivery create/edit body: `{ "sales_invoice_id": 1, "driver_name": "Driver", "date": "2026-11-13", "address": "Jakarta", "note": "", "details": [{ "sales_invoice_detail_id": 1, "quantity": 2, "notes": "" }] }`. Status body: `{ "status": "On Delivery" }`, `{ "status": "Delivered" }`, or `{ "status": "Cancelled", "cancellation_note": "Reason" }`. Approval takes `{}`. All writes use JSON content type. The legacy sales cancellation route uses the same Finance approval workflow.

Run `cargo test -- --include-ignored` from `backend` to include MySQL workflow tests. These create uniquely named temporary databases and require create/drop permission; override `TEST_MYSQL_ADMIN_URL` if needed. Tests cover migrations, invoice/stock/payment flows, JWT validation, refresh replay/concurrency, revocation, RBAC, idempotent replay/concurrency, Finance void approvals, delivery quantity races and cancellation release. Run `npm run check` and `npm run build` from `frontend` for UI validation.


## Connected record detail pages

Purchase orders, purchase invoices and sales invoices open at `/documents/{id}/`. Payments use `/payments/{id}/`, delivery orders `/delivery-orders/{id}/`, and products `/products/{id}/`. Existing list and dashboard links open full detail pages. Invoice edits/void requests and delivery draft edits/status changes are available on the detail pages.

Sales invoice pages list every allocated payment and related delivery order. Payment pages link back to each allocated invoice and show the amount applied to that invoice. Purchase invoice pages link to their optional purchase order; purchase order pages list the corresponding purchase invoices. Product names link to their product page throughout lists, forecasts, document lines and delivery details. Newly entered products in an unposted preview do not yet have a persisted detail page.

Product pages show inventory plus the latest ten purchase orders, purchase invoices, sales invoices and delivery orders containing the product, ordered by document date and ID descending. Each related entry links to its own detail page. Related sections respect the current user's permissions; a product-only role cannot read invoice or delivery data through the product endpoint.

Detail APIs are `GET /api/documents/{id}`, `/api/payments/{id}`, `/api/delivery-orders/{id}`, and `/api/products/{id}`. Missing records return 404. Payment lists include structured `invoice_links` as well as the existing display text. The existing schema already provides the foreign keys; no data migration is required for these navigation changes.

The static frontend emits `200.html` for dynamic routes. The Rust server serves this fallback for frontend routes, allowing bookmarked detail URLs and reloads. If the frontend is hosted separately, configure the host to rewrite unmatched frontend paths to `200.html` and proxy `/api` to Rust.


## Full-year demo dataset

For a larger, connected dataset covering the trailing twelve months through today in Asia/Jakarta:

```powershell
cd backend
cargo run --bin seed_year
```

This has been loaded locally for **2025-11-15 through 2026-11-14**, with transactions on all 365 days:

| Records added | Count |
|---|---:|
| Products | 40 |
| Purchase orders | 104 |
| Purchase invoices | 128 |
| Sales invoices | 1,460 |
| Document details | 5,563 |
| Payments | 1,382 |
| Payment allocations | 1,548 |
| Delivery orders | 1,460 |
| Delivery details | 4,415 |
| Stock movements | 5,996 |
| Invoice void requests | 152 |
| Delivery cancellation requests | 152 |

The fixture uses separate `[Year Demo]` product names and `YEAR-` references. Existing business records and the smaller `[Demo]` / `DEMO-` fixture are preserved. The completion marker and all new business records commit in one transaction. Concurrent invocations serialize on the marker; reruns skip the existing fixture, including after you edit its records, without resetting dates, passwords or stock. Namespace conflicts without a completion marker stop the run and roll back. To choose a fixed end date on a fresh database, use `cargo run --bin seed_year -- --end-date 2026-11-14`; a rerun always keeps its original range.

Data includes seasonal daily multi-product sales, monthly multi-product purchase orders, linked and standalone purchase invoices, full/partial/unpaid/cancelled invoices, multi-invoice payments and payment installments. Split shipments and all delivery statuses are represented, along with current approval requests and cancellation/refund audit records. Historic shipments are delivered; draft/approved/in-transit examples are concentrated in the last three weeks. Products include active, inactive and no-recent-demand cases. Purchase stock becomes available only after full payment, using the same inventory and payment validation helpers as the application.

Before committing, the seeder checks physical/available stock ledgers, document totals, invoice balances, payment allocations, purchase-order receiving balances, delivery/reservation quantities, payment eligibility for deliveries and paid purchase stock release. Run [backend/seeds/verify_year.sql](backend/seeds/verify_year.sql) for the same read-only reconciliation checks, monthly activity and the saved seed report. All eight violation counts should be zero. The ignored MySQL test also covers concurrent runs, reruns, daily coverage, statuses, preservation of existing records and namespace conflicts.

The dashboard now defaults to the last twelve months. Monetary summaries and recent-transaction amounts use readable abbreviated labels (`Rp 10.1 million`, `Rp 2.4 billion`, `Rp 1.2 trillion`, and larger units); hover to see the exact rupiah value. Invoice, payment and other accounting detail pages keep full amounts.

## List pagination and filters

All business, forecast, role, user and approval list screens use `GET /api/lists/{resource}`. Resources: `products`, `purchase-orders`, `purchase-invoices`, `sales-invoices`, `documents`, `payments`, `delivery-orders`, `invoice-approvals`, `delivery-cancellations`, `roles`, `users`, `forecast`. Each resource requires its existing read permission. The original array endpoints remain available for compatibility and form lookups; dashboard totals still use the complete dataset.

Query parameters:

- `limit`: 1–100, default 25; UI choices are 25, 50 and 100.
- `cursor`: opaque `next_cursor` or `start_cursor` from a previous response. Cursors belong to the user, resource and exact filters/page size; changing these starts a new traversal.
- `q`: literal, case-insensitive substring search, up to 200 characters. Search covers relevant document numbers, names and descriptions rather than just the current page.
- `from`, `to`: optional inclusive `YYYY-MM-DD` boundaries. Both the API and UI reject reversed ranges. The UI includes date presets, custom ranges, and a clear button.
- `status`: applicable business/account/approval status; `kind` on payments (`sales_invoice` for customer receipts, `purchase_invoice` for supplier payments) or the combined documents endpoint.
- `days`, `lead_time`: forecast settings, each 1–365.

Responses contain `items`, `total`, `limit`, `has_more`, `next_cursor`, `start_cursor` and optional forecast `summary`. Records use date descending, then unique ID descending; cursors retain the initial maximum ID so later inserts cannot displace records during a traversal, including when returning to the first page. Existing records edited/deleted during navigation can still change results; refresh starts a new traversal. Filters reset to page one. No arbitrary column sorting is currently exposed. A future sort must use its own cursor ordering and ID tie-breaker, or offset pagination for that list.

Business lists use the document/payment/delivery date. Approval date filters use the linked invoice date or delivery date, as labelled. Product, forecast, role and user filters use record creation date; existing roles/users receive the migration timestamp when creation tracking is added. Lists use the document's vertical scrollbar. Filters and column headings stick to the top of the page while scrolling through the list; wide tables retain horizontal scrolling with synchronized headings. Moving between pages returns to the list header. Payment lists show invoice counts with detail links. CSV exports on list pages export the displayed page.

Approvals has separate sidebar and page submenus at `/approvals/?section=invoices` and `/approvals/?section=deliveries`. Requests display linked invoice numbers or generated delivery numbers (`DO-000001`) and requester/reviewer usernames. Delivery numbers are derived consistently from the existing immutable ID, without changing record relationships.

The MySQL pagination integration test checks tied dates, complete traversal, insert isolation, returning to the initial snapshot, inclusive ranges, invalid cursors/filters, payment types, literal search, resource permissions and readable approval references.

Browser regression checks: from `frontend`, run `npx playwright test tests/lists.e2e.ts --workers=1` (uses installed Google Chrome). These use API fixtures to check filter resets, pagination, sticky controls/headers, mobile width and approval submenu navigation. The real MySQL integration test covers the API queries and permissions.


Delivery list rows use a labelled view icon and an overflow action menu. The menu keeps clear action names, highlights cancellation in red, preserves role/status checks, supports keyboard navigation and Escape, and closes when clicking outside. Finalized deliveries only show the view action. Native popovers keep menus visible outside the table's horizontal viewport.

List layouts share full-width content gutters, compact 8px cell padding, continuous horizontal row separators and synchronized sticky headings. Products, forecasts, documents, payments, deliveries, approvals and role/user administration use the same icon actions; secondary actions remain labelled in the menu. Product descriptions truncate to one line in lists, and purchase-order reference badges use a compact second line. Browser regressions cover every list at desktop width, narrow viewport overflow/alignment, and approval/delivery action menus.
