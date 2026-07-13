pub(super) const SQL: &str = "
SELECT invalid FROM (
 SELECT 'activity:'||id AS key,
  CASE WHEN ended_at<started_at THEN 1 ELSE 0 END AS invalid
 FROM activities
 UNION ALL SELECT 'capture:'||c.source_id,
  CASE WHEN a.id IS NULL OR c.timestamp<a.started_at OR c.timestamp>a.ended_at
   OR (c.snapshot_id IS NOT NULL AND (s.id IS NULL OR s.activity_id!=c.activity_id
    OR c.timestamp<s.started_at OR c.timestamp>s.ended_at))
  THEN 1 ELSE 0 END
 FROM capture_refs c
 LEFT JOIN activities a ON a.id=c.activity_id
 LEFT JOIN session_snapshots s ON s.id=c.snapshot_id
 UNION ALL SELECT 'snapshot:'||s.id,
  CASE WHEN a.id IS NULL OR s.ended_at<s.started_at
   OR s.started_at<a.started_at OR s.ended_at>a.ended_at
   OR s.capture_count<0
   OR s.capture_count!=(SELECT COUNT(*) FROM capture_refs c WHERE c.snapshot_id=s.id)
  THEN 1 ELSE 0 END
 FROM session_snapshots s LEFT JOIN activities a ON a.id=s.activity_id
 UNION ALL SELECT 'event:'||printf('%020d',id),
  CASE WHEN timestamp<0 OR TRIM(agent_name)='' OR TRIM(action)='' THEN 1 ELSE 0 END
 FROM agent_activity
) ORDER BY key";
