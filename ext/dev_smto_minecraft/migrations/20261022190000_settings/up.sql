CREATE TABLE IF NOT EXISTS dev_smto_minecraft_settings (
    id integer PRIMARY KEY DEFAULT 1,
    curseforge_api_key text,
    CHECK (id = 1)
);

INSERT INTO dev_smto_minecraft_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
