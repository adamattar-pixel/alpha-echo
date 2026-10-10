CREATE UNIQUE INDEX research_runs_one_active_task
    ON research_runs (briefing_run_id, market, run_type)
    WHERE status = 'running';
