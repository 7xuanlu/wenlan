SELECT 'foreign_key_violations' AS metric
  FROM pragma_foreign_key_check

UNION ALL
SELECT 'page_sources_missing_owner'
  FROM page_sources ps
 WHERE NOT EXISTS (
     SELECT 1
       FROM memories m
      WHERE m.source != 'episode'
        AND (m.source_id = ps.memory_source_id OR m.id = ps.memory_source_id)
 )

UNION ALL
SELECT 'page_evidence_memory_missing_owner'
  FROM page_evidence pe
 WHERE pe.source_kind = 'memory'
   AND NOT EXISTS (
       SELECT 1
         FROM memories m
        WHERE m.source != 'episode'
          AND (m.source_id = pe.locator OR m.id = pe.locator)
   )

UNION ALL
SELECT 'pages_dangling_entity'
  FROM pages p
 WHERE p.entity_id IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM entities e WHERE e.id = p.entity_id)

UNION ALL
SELECT 'pending_revision_missing_target'
  FROM (
      SELECT DISTINCT revision.source_id
        FROM memories revision
       WHERE revision.source = 'memory'
         AND revision.pending_revision = 1
         AND revision.supersedes IS NOT NULL
         AND NOT EXISTS (
             SELECT 1
               FROM memories target
              WHERE target.source = 'memory'
                AND target.source_id = revision.supersedes
         )
  )

UNION ALL
SELECT 'enrichment_steps_missing_owner'
  FROM (
      SELECT DISTINCT es.source_id
        FROM enrichment_steps es
       WHERE NOT EXISTS (
           SELECT 1
             FROM memories m
            WHERE m.source != 'episode'
              AND m.source_id = es.source_id
       )
  )

UNION ALL
SELECT 'legacy_page_sources_missing_owner'
  FROM pages p
 WHERE NOT json_valid(COALESCE(p.source_memory_ids, '[]'))

UNION ALL
SELECT 'legacy_page_sources_missing_owner'
  FROM pages p,
       json_each(
           CASE
               WHEN json_valid(COALESCE(p.source_memory_ids, '[]'))
               THEN COALESCE(p.source_memory_ids, '[]')
               ELSE '[]'
           END
       ) source
 WHERE source.type = 'text'
   AND NOT EXISTS (
       SELECT 1
         FROM memories m
        WHERE m.source != 'episode'
          AND (m.source_id = source.value OR m.id = source.value)
   )

UNION ALL
SELECT 'relation_self_edges'
  FROM relations
 WHERE from_entity = to_entity

UNION ALL
SELECT 'episodes_missing_parent'
  FROM memories episode
 WHERE episode.source = 'episode'
   AND (episode.episode_of IS NULL OR NOT EXISTS (
       SELECT 1
         FROM memories parent
        WHERE parent.source != 'episode'
          AND parent.source_id = episode.episode_of
   ))

UNION ALL
SELECT 'broken_nonnull_page_links'
  FROM page_links link
 WHERE link.target_page_id IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM pages p WHERE p.id = link.target_page_id)

UNION ALL
SELECT 'done_queue_missing_sync_receipt'
  FROM document_enrichment_queue queue
 WHERE queue.status = 'done'
   AND NOT EXISTS (
       SELECT 1
         FROM source_sync_state sync
        WHERE sync.source_id = queue.source_id
          AND sync.file_path = queue.file_path
   )

UNION ALL
SELECT 'multi_chunk_memory_sources'
  FROM (
      SELECT DISTINCT first.source, first.source_id
        FROM memories first
        JOIN memories second
          ON second.source = first.source
         AND second.source_id = first.source_id
         AND second.id != first.id
       WHERE first.source != 'episode'
  )

UNION ALL
SELECT 'content_hash_missing_heads'
  FROM memories head
 WHERE head.source != 'episode'
   AND head.chunk_index = 0
   AND NULLIF(TRIM(head.content_hash), '') IS NULL
   AND EXISTS (
       SELECT 1
         FROM source_sync_state sync
        WHERE sync.source_id = head.source_id
   );
