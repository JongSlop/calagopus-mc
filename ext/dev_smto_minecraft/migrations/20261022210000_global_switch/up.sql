ALTER TABLE dev_smto_minecraft_settings
    ADD COLUMN IF NOT EXISTS auto_updates_enabled boolean NOT NULL DEFAULT true;
