-- Read-only checks. Every violations value should be zero.
SELECT 'stock_ledger' check_name, COUNT(*) violations
FROM products p LEFT JOIN (SELECT product_id,SUM(quantity) quantity FROM stock_movements GROUP BY product_id) m ON m.product_id=p.id
WHERE p.name LIKE '[Demo] %' AND (p.quantity<>COALESCE(m.quantity,0) OR p.quantity<0)
UNION ALL
SELECT 'remaining_stock_ledger',COUNT(*) FROM products p
LEFT JOIN (SELECT product_id,SUM(remaining_delta) remaining FROM stock_movements GROUP BY product_id) m ON m.product_id=p.id
WHERE p.name LIKE '[Demo] %' AND (p.remaining_quantity<>COALESCE(m.remaining,0) OR p.remaining_quantity<0 OR p.remaining_quantity>p.quantity)
UNION ALL
SELECT 'document_totals',COUNT(*) FROM documents d
LEFT JOIN (SELECT document_id,SUM(subtotal) total FROM document_lines GROUP BY document_id) l ON l.document_id=d.id
WHERE d.number LIKE 'DEMO-%' AND d.grand_total<>COALESCE(l.total,0)
UNION ALL
SELECT 'line_subtotals',COUNT(*) FROM document_lines l JOIN documents d ON d.id=l.document_id
WHERE d.number LIKE 'DEMO-%' AND (l.subtotal<>l.quantity*l.price OR l.quantity<0 OR (l.quantity=0 AND d.kind<>'purchase_order'))
UNION ALL
SELECT 'invoice_balances',COUNT(*) FROM documents d
LEFT JOIN (SELECT invoice_id,SUM(amount) paid FROM payment_allocations GROUP BY invoice_id) a ON a.invoice_id=d.id
WHERE d.number LIKE 'DEMO-%' AND d.kind<>'purchase_order' AND d.status<>'Cancelled'
AND (d.remaining_payment<>d.grand_total-COALESCE(a.paid,0) OR d.remaining_payment<0
OR d.status<>CASE WHEN d.remaining_payment=0 THEN 'Paid' WHEN d.remaining_payment=d.grand_total THEN 'Unpaid' ELSE 'Partial Paid' END)
UNION ALL
SELECT 'payment_allocations',COUNT(*) FROM payments p
LEFT JOIN (SELECT payment_id,SUM(amount) allocated FROM payment_allocations GROUP BY payment_id) a ON a.payment_id=p.id
WHERE p.reference LIKE 'DEMO-%' AND p.amount<>COALESCE(a.allocated,0)
UNION ALL
SELECT 'allocation_types',COUNT(*) FROM payment_allocations a
JOIN payments p ON p.id=a.payment_id JOIN documents d ON d.id=a.invoice_id
WHERE p.reference LIKE 'DEMO-%' AND (p.kind<>d.kind OR a.amount<=0)
UNION ALL
SELECT 'po_remaining_quantity',COUNT(*) FROM document_lines l JOIN documents d ON d.id=l.document_id
LEFT JOIN (SELECT inv.purchase_order_id,il.product_id,SUM(il.quantity) received FROM documents inv JOIN document_lines il ON il.document_id=inv.id WHERE inv.kind='purchase_invoice' GROUP BY inv.purchase_order_id,il.product_id) r
ON r.purchase_order_id=d.id AND r.product_id=l.product_id
WHERE d.number LIKE 'DEMO-%' AND d.kind='purchase_order'
AND (l.realized_quantity<>l.quantity-COALESCE(r.received,0)
OR l.status<>CASE WHEN l.realized_quantity<0 THEN 'Warning' WHEN l.realized_quantity=0 THEN 'Complete' WHEN r.received IS NULL THEN 'Open' ELSE 'Partial' END);

SELECT 'products' table_name,COUNT(*) rows_count FROM products WHERE name LIKE '[Demo] %'
UNION ALL SELECT 'documents',COUNT(*) FROM documents WHERE number LIKE 'DEMO-%'
UNION ALL SELECT 'document_lines',COUNT(*) FROM document_lines l JOIN documents d ON d.id=l.document_id WHERE d.number LIKE 'DEMO-%'
UNION ALL SELECT 'payments',COUNT(*) FROM payments WHERE reference LIKE 'DEMO-%'
UNION ALL SELECT 'payment_allocations',COUNT(*) FROM payment_allocations a JOIN payments p ON p.id=a.payment_id WHERE p.reference LIKE 'DEMO-%'
UNION ALL SELECT 'stock_movements',COUNT(*) FROM stock_movements m JOIN products p ON p.id=m.product_id WHERE p.name LIKE '[Demo] %';
