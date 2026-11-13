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

`backend/.env.example` documents DATABASE_URL, MEILI_URL, optional MEILI_KEY, BIND_ADDRESS, FRONTEND_ORIGIN, and STATIC_DIR. Root/no-password and unauthenticated Meilisearch are the user's local development settings. Authentication and user roles are not implemented; the app binds to loopback by default.

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
