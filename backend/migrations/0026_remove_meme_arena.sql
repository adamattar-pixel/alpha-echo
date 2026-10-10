-- Retire dedicated Meme storage without changing checksums of applied migrations.
DROP TABLE meme_token_blacklist;
DROP TABLE meme_token_observations;
DROP TABLE meme_report_runs;
DELETE FROM app_settings WHERE key = 'meme_report_schedule';
-- Shared legacy wallet columns and market enums remain for schema compatibility.
