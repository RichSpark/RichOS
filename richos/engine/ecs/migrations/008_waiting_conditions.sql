-- A condition and its original instruction travel with the existing obligation.
ALTER TABLE ecs_continuity_items ADD COLUMN condition_json TEXT;
CREATE INDEX ecs_waiting_conditions ON ecs_continuity_items(entity_id, thread_id, item_id)
  WHERE condition_json IS NOT NULL AND status IN ('active', 'accepted', 'blocked', 'pending');
ALTER TABLE ecs_continuity_items ADD COLUMN reply_owed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ecs_continuity_items ADD COLUMN reply_turn TEXT;
ALTER TABLE ecs_continuity_items ADD COLUMN reply_evidence TEXT;
CREATE INDEX ecs_pending_reply ON ecs_continuity_items(entity_id, thread_id, reply_turn)
  WHERE reply_owed=1;
