UPDATE app_settings
SET value = value || jsonb_build_object('enabled', true),
    updated_at = now()
WHERE key = 'briefing_schedule'
  AND NOT (value ? 'enabled');
