ALTER TABLE briefing_runs
    DROP CONSTRAINT briefing_runs_kind_check;

UPDATE briefing_runs SET kind = 'report';

ALTER TABLE briefing_runs
    ADD CONSTRAINT briefing_runs_kind_check CHECK (kind = 'report');

UPDATE app_settings
SET value = jsonb_build_object(
        'timezone', COALESCE(value->>'timezone', 'Asia/Shanghai'),
        'time', COALESCE(value->>'time', value->>'evening', value->>'morning', '19:00')
    ),
    updated_at = now()
WHERE key = 'briefing_schedule';
