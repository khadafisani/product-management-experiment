CREATE TABLE delivery_cancellation_requests (
 id BIGINT PRIMARY KEY AUTO_INCREMENT,
 delivery_order_id BIGINT NOT NULL,
 requested_by BIGINT NOT NULL,
 reason VARCHAR(500) NOT NULL,
 status VARCHAR(30) NOT NULL DEFAULT 'Pending',
 reviewed_by BIGINT NULL,
 FOREIGN KEY(delivery_order_id) REFERENCES delivery_orders(id),
 FOREIGN KEY(requested_by) REFERENCES users(id),
 FOREIGN KEY(reviewed_by) REFERENCES users(id)
);
ALTER TABLE document_lines ADD CONSTRAINT delivery_quantity_balance CHECK(delivered_quantity>=0 AND on_delivery_quantity>=0 AND delivered_quantity+on_delivery_quantity<=quantity);
ALTER TABLE delivery_order_details ADD CONSTRAINT positive_delivery_quantity CHECK(quantity>0);
