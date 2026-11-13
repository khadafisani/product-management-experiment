CREATE TABLE products (
 id BIGINT PRIMARY KEY AUTO_INCREMENT, name VARCHAR(200) NOT NULL UNIQUE,
 quantity BIGINT NOT NULL DEFAULT 0, status VARCHAR(30) NOT NULL DEFAULT 'Active',
 photo TEXT NULL, description TEXT NOT NULL, price BIGINT NOT NULL DEFAULT 0,
 reorder_level BIGINT NOT NULL DEFAULT 10, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE documents (
 id BIGINT PRIMARY KEY AUTO_INCREMENT, kind VARCHAR(30) NOT NULL,
 number VARCHAR(100) NOT NULL, date DATE NOT NULL, person VARCHAR(200) NOT NULL,
 purchase_order_id BIGINT NULL, grand_total BIGINT NOT NULL,
 remaining_payment BIGINT NOT NULL, status VARCHAR(30) NOT NULL,
 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
 UNIQUE(kind,number), FOREIGN KEY(purchase_order_id) REFERENCES documents(id)
);
CREATE TABLE document_lines (
 id BIGINT PRIMARY KEY AUTO_INCREMENT, document_id BIGINT NOT NULL,
 product_id BIGINT NOT NULL, item_name VARCHAR(200) NOT NULL, quantity BIGINT NOT NULL,
 realized_quantity BIGINT NOT NULL, price BIGINT NOT NULL, subtotal BIGINT NOT NULL,
 status VARCHAR(30) NOT NULL,
 FOREIGN KEY(document_id) REFERENCES documents(id), FOREIGN KEY(product_id) REFERENCES products(id),
 UNIQUE(document_id,product_id)
);
CREATE TABLE payments (
 id BIGINT PRIMARY KEY AUTO_INCREMENT, reference VARCHAR(100) NOT NULL UNIQUE,
 date DATE NOT NULL, amount BIGINT NOT NULL, kind VARCHAR(30) NOT NULL,
 created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE payment_allocations (
 payment_id BIGINT NOT NULL, invoice_id BIGINT NOT NULL, amount BIGINT NOT NULL,
 PRIMARY KEY(payment_id,invoice_id), FOREIGN KEY(payment_id) REFERENCES payments(id),
 FOREIGN KEY(invoice_id) REFERENCES documents(id)
);
CREATE TABLE stock_movements (
 id BIGINT PRIMARY KEY AUTO_INCREMENT, product_id BIGINT NOT NULL, document_id BIGINT NULL,
 quantity BIGINT NOT NULL, reason VARCHAR(100) NOT NULL, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
 FOREIGN KEY(product_id) REFERENCES products(id), FOREIGN KEY(document_id) REFERENCES documents(id)
);
