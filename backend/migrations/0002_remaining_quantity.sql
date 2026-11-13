ALTER TABLE products ADD COLUMN remaining_quantity BIGINT NOT NULL DEFAULT 0 AFTER quantity;
ALTER TABLE documents ADD COLUMN stock_released BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE stock_movements ADD COLUMN remaining_delta BIGINT NOT NULL DEFAULT 0;

-- Preserve the availability of inventory already in use. Historical purchase
-- invoices are marked handled so their later settlement cannot release it twice.
UPDATE products SET remaining_quantity=quantity;
UPDATE stock_movements SET remaining_delta=quantity;
UPDATE documents SET stock_released=TRUE WHERE kind='purchase_invoice';

CREATE TABLE sales_invoice_cancellations (
 sales_invoice_id BIGINT PRIMARY KEY,
 reason VARCHAR(500) NOT NULL,
 refund_due BIGINT NOT NULL DEFAULT 0,
 cancelled_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
 FOREIGN KEY(sales_invoice_id) REFERENCES documents(id)
);
