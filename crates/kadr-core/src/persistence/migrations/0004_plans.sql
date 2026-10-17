-- Stage D: what the sorting plan says about each file.
ALTER TABLE operation_item ADD COLUMN role TEXT NOT NULL DEFAULT 'main';
ALTER TABLE operation_item ADD COLUMN parent_item_id INTEGER REFERENCES operation_item(id) ON DELETE CASCADE;
ALTER TABLE operation_item ADD COLUMN plan_status TEXT NOT NULL DEFAULT 'ready';
ALTER TABLE operation_item ADD COLUMN included INTEGER NOT NULL DEFAULT 1;
ALTER TABLE operation_item ADD COLUMN captured_local TEXT;
ALTER TABLE operation_item ADD COLUMN place_label TEXT;
-- Position in the plan, so the table shows files in plan order.
ALTER TABLE operation_item ADD COLUMN seq INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_op_item_parent ON operation_item(parent_item_id);
CREATE INDEX idx_op_item_plan_seq ON operation_item(plan_id, seq);
