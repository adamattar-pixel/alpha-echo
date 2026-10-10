WITH ranked_active AS (
    SELECT id,
           row_number() OVER (ORDER BY created_at DESC, id DESC) AS rank
    FROM briefing_runs
    WHERE status IN ('pending', 'running')
)
UPDATE briefing_runs br
SET status = 'failed',
    validation_status = 'superseded',
    error = 'superseded during unified report migration',
    data_gaps = COALESCE(br.data_gaps, '[]'::jsonb)
        || jsonb_build_array('升级为统一报告时发现重复活动任务；保留最新任务并终止本任务。'),
    completed_at = now()
FROM ranked_active ranked
WHERE br.id = ranked.id
  AND ranked.rank > 1;

CREATE UNIQUE INDEX briefing_runs_one_active
    ON briefing_runs ((true))
    WHERE status IN ('pending', 'running');
