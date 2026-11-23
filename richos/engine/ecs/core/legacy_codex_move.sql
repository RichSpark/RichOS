INSERT INTO ecs_entities(
    entity_id, person_id, display_name, canonical_root, git_common_dir, status, source_ref, revision
)
SELECT :destination, 'ceo-default', :destination, 'app-entity:' || :destination,
       'app-entity:' || :destination, 'active', 'richos:legacy-codex-company', 1
WHERE EXISTS (
    SELECT 1 FROM ecs_threads WHERE entity_id = :source AND thread_id = :thread
)
ON CONFLICT(entity_id) DO NOTHING;

UPDATE ecs_threads SET entity_id = :destination WHERE entity_id = :source AND thread_id = :thread;
UPDATE ecs_sessions SET entity_id = :destination WHERE entity_id = :source AND thread_id = :thread;
UPDATE ecs_active_context SET entity_id = :destination WHERE entity_id = :source AND thread_id = :thread;
UPDATE ecs_continuity_items SET entity_id = :destination WHERE entity_id = :source AND thread_id = :thread;
UPDATE ecs_work_units SET entity_id = :destination WHERE entity_id = :source AND thread_id = :thread;
UPDATE ecs_actions SET entity_id = :destination WHERE entity_id = :source AND thread_id = :thread;
UPDATE ecs_corrections SET entity_id = :destination WHERE entity_id = :source AND thread_id = :thread;
UPDATE ecs_turn_extractions SET entity_id = :destination WHERE entity_id = :source AND thread_id = :thread;
UPDATE ecs_checkpoints SET entity_id = :destination WHERE entity_id = :source AND thread_id = :thread;
