import { faFloppyDisk, faTrashCan, faUserPlus } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { ActionIcon, Alert, Loader, Switch, Table } from '@mantine/core';
import { useEffect, useState } from 'react';
import { axiosInstance, httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import TitleCard from '@/elements/data-display/TitleCard.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import Group from '@/elements/layout/Group.tsx';
import ServerContentContainer from '@/elements/containers/ServerContentContainer.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import { useServerStore } from '@/stores/server.ts';

interface WhitelistEntry {
  name: string;
  uuid?: string | null;
}

interface WhitelistResponse {
  entries: WhitelistEntry[];
  enforced: boolean;
  online_mode: boolean;
}

/// Skin head from mc-heads. The name resolves to the current skin, the UUID is the
/// fallback for a player who renamed since the whitelist entry was written.
const headUrl = (entry: WhitelistEntry) => `https://mc-heads.net/avatar/${entry.name}/32`;

const uuidHeadUrl = (entry: WhitelistEntry) => `https://mc-heads.net/avatar/${entry.uuid}/32`;

const endpoint = (serverUuid: string) => `/api/client/servers/${serverUuid}/dev.smto.minecraft/whitelist`;

export default function WhitelistPage() {
  const serverUuid = useServerStore((state) => state.server.uuid);
  const { addToast } = useToast();

  const [entries, setEntries] = useState<WhitelistEntry[]>([]);
  const [enforced, setEnforced] = useState(true);
  const [onlineMode, setOnlineMode] = useState(true);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [newName, setNewName] = useState('');

  useEffect(() => {
    setLoading(true);
    axiosInstance
      .get<WhitelistResponse>(endpoint(serverUuid))
      .then(({ data }) => {
        setEntries(data.entries);
        setEnforced(data.enforced);
        setOnlineMode(data.online_mode);
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setLoading(false));
  }, [serverUuid]);

  const addPlayer = () => {
    const name = newName.trim();
    if (!name) return;

    if (entries.some((entry) => entry.name.toLowerCase() === name.toLowerCase())) {
      addToast(`'${name}' is already whitelisted.`, 'error');
      return;
    }

    setEntries((current) => [...current, { name }]);
    setNewName('');
  };

  const removePlayer = (name: string) => {
    setEntries((current) => current.filter((entry) => entry.name !== name));
  };

  const save = (enforcedOverride?: boolean) => {
    setSaving(true);
    axiosInstance
      .put<WhitelistResponse>(endpoint(serverUuid), {
        entries,
        enforced: enforcedOverride ?? enforced,
      })
      .then(({ data }) => {
        setEntries(data.entries);
        setEnforced(data.enforced);
        setOnlineMode(data.online_mode);
        addToast('Whitelist saved.', 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setSaving(false));
  };

  return (
    <ServerContentContainer
      title='Whitelist'
      contentRight={
        <Button
          onClick={() => save()}
          loading={saving}
          disabled={loading}
          leftSection={<FontAwesomeIcon icon={faFloppyDisk} />}
        >
          Save
        </Button>
      }
    >
      <Alert
        color={enforced ? 'green' : 'yellow'}
        mb='md'
        styles={{ message: { display: 'flex', alignItems: 'center', gap: '1rem' } }}
      >
        <Switch
          label='Enforce whitelist'
          description='Writes white-list in server.properties. Without it Minecraft lets everyone in, listed or not.'
          checked={enforced}
          onChange={(event) => {
            const next = event.currentTarget.checked;
            setEnforced(next);
            save(next);
          }}
          disabled={saving}
        />
      </Alert>

      <Alert color='blue' mb='md' variant='light'>
        {onlineMode
          ? 'Online mode: player UUIDs come from Mojang, so only real accounts can be added. A whitelist reload is sent to a running server.'
          : 'Offline mode: UUIDs are derived from the player name, no Mojang lookup needed. A whitelist reload is sent to a running server.'}
      </Alert>

      <TitleCard title='Players'>
        <Group mb='md'>
          <TextInput
            style={{ flex: 1 }}
            placeholder='Player name'
            value={newName}
            onChange={(event) => setNewName(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') addPlayer();
            }}
          />
          <Button variant='default' onClick={addPlayer} leftSection={<FontAwesomeIcon icon={faUserPlus} />}>
            Add
          </Button>
        </Group>

        {loading ? (
          <Loader />
        ) : (
          <Table>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>Player</Table.Th>
                <Table.Th>UUID</Table.Th>
                <Table.Th />
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {entries.map((entry) => (
                <Table.Tr key={entry.name}>
                  <Table.Td>
                    <Group gap='xs'>
                      <img
                        src={headUrl(entry)}
                        alt=''
                        width={28}
                        height={28}
                        style={{ borderRadius: 4, imageRendering: 'pixelated' }}
                        onError={(event) => {
                          if (entry.uuid) {
                            event.currentTarget.src = uuidHeadUrl(entry);
                          }
                        }}
                      />
                      {entry.name}
                    </Group>
                  </Table.Td>
                  <Table.Td>
                    <code>{entry.uuid ?? 'resolved on save'}</code>
                  </Table.Td>
                  <Table.Td w={60}>
                    <ActionIcon color='red' variant='subtle' onClick={() => removePlayer(entry.name)}>
                      <FontAwesomeIcon icon={faTrashCan} />
                    </ActionIcon>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}

        {!loading && entries.length === 0 && <p>Nobody is whitelisted yet.</p>}

      </TitleCard>
    </ServerContentContainer>
  );
}
