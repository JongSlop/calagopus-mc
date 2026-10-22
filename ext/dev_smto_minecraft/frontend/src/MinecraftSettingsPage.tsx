import { faKey, faFloppyDisk } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Alert } from '@mantine/core';
import { useEffect, useState } from 'react';
import { axiosInstance, httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import TitleCard from '@/elements/data-display/TitleCard.tsx';
import PasswordInput from '@/elements/input/PasswordInput.tsx';
import AdminContentContainer from '@/elements/containers/AdminContentContainer.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';

interface SettingsResponse {
  curseforge_configured: boolean;
}

const endpoint = '/api/admin/dev.smto.minecraft/settings';

export default function MinecraftSettingsPage() {
  const { addToast } = useToast();

  const [configured, setConfigured] = useState(false);
  const [apiKey, setApiKey] = useState('');
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    axiosInstance
      .get<SettingsResponse>(endpoint)
      .then(({ data }) => setConfigured(data.curseforge_configured))
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'));
  }, []);

  const save = (key: string | null) => {
    setBusy(true);
    axiosInstance
      .put<SettingsResponse>(endpoint, { curseforge_api_key: key })
      .then(({ data }) => {
        setConfigured(data.curseforge_configured);
        setApiKey('');
        addToast(key ? 'CurseForge API key saved.' : 'CurseForge API key removed.', 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  return (
    <AdminContentContainer title='Minecraft Tools'>
      <TitleCard title='CurseForge' icon={<FontAwesomeIcon icon={faKey} />}>
        <Alert color={configured ? 'green' : 'yellow'} variant='light' mb='md'>
          {configured
            ? 'A CurseForge API key is configured. Mods can be searched and installed from CurseForge.'
            : 'No CurseForge API key yet. Get one at console.curseforge.com and paste it here.'}
        </Alert>

        <PasswordInput
          label='API key'
          placeholder={configured ? 'Stored, enter a new key to replace it' : 'Paste your key'}
          value={apiKey}
          onChange={(event) => setApiKey(event.currentTarget.value)}
        />

        <Button
          mt='md'
          onClick={() => save(apiKey)}
          loading={busy}
          disabled={!apiKey.trim()}
          leftSection={<FontAwesomeIcon icon={faFloppyDisk} />}
        >
          Save key
        </Button>

        {configured && (
          <Button mt='md' ml='sm' variant='light' color='red' onClick={() => save(null)} loading={busy}>
            Remove key
          </Button>
        )}
      </TitleCard>
    </AdminContentContainer>
  );
}
