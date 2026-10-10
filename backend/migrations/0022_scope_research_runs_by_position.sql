DROP INDEX IF EXISTS research_runs_one_active_task;

CREATE UNIQUE INDEX research_runs_one_active_task
    ON research_runs (
        briefing_run_id,
        market,
        run_type,
        COALESCE(input_snapshot->'task_scope'->>'position_snapshot_item_id', '')
    )
    WHERE status = 'running';
