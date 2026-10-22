import { faFloppyDisk, faTrashCan, faUserPlus } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { ActionIcon, Alert, Loader, Table } from '@mantine/core';
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

const endpoint = (serverUuid: string) => `/api/client/servers/${serverUuid}/dev.smto.minecraft/whitelist`;
const propertiesEndpoint = (serverUuid: string) =>
  `/api/client/servers/${serverUuid}/dev.smto.minecraft/properties`;

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

  const save = () => {
    setSaving(true);
    axiosInstance
      .put<WhitelistResponse>(endpoint(serverUuid), { entries })
      .then(({ data }) => {
        setEntries(data.entries);
        setEnforced(data.enforced);
        setOnlineMode(data.online_mode);
        addToast('Whitelist saved.', 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setSaving(false));
  };

  const enableWhitelist = () => {
    setSaving(true);
    axiosInstance
      .put(propertiesEndpoint(serverUuid), { edits: [{ key: 'white-list', value: 'true' }] })
      .then(() => {
        setEnforced(true);
        addToast('white-list is now enabled in server.properties.', 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setSaving(false));
  };

  return (
    <ServerContentContainer
      title='Whitelist'
      contentRight={
        <Button onClick={save} loading={saving} disabled={loading} leftSection={<FontAwesomeIcon icon={faFloppyDisk} />}>
          Save
        </Button>
      }
    >
      {!enforced && (
        <Alert
          color='yellow'
          mb='md'
          title='The whitelist is not enabled'
          styles={{ message: { display: 'flex', alignItems: 'center', gap: '1rem' } }}
        >
          <span>Minecraft only enforces this list while white-list=true is set in server.properties.</span>
          <Button variant='light' size='xs' onClick={enableWhitelist} loading={saving}>
            Enable whitelist
          </Button>
        </Alert>
      )}

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
                  <Table.Td>{entry.name}</Table.Td>
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
