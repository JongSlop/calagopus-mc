import { faFloppyDisk, faPlus, faTrashCan } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { ActionIcon, Alert, Loader, SegmentedControl, Table } from '@mantine/core';
import { useEffect, useState } from 'react';
import { axiosInstance, httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import TitleCard from '@/elements/data-display/TitleCard.tsx';
import NumberInput from '@/elements/input/NumberInput.tsx';
import TextArea from '@/elements/input/TextArea.tsx';
import Switch from '@/elements/input/Switch.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import Group from '@/elements/layout/Group.tsx';
import ServerContentContainer from '@/elements/containers/ServerContentContainer.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import { useServerStore } from '@/stores/server.ts';

interface PropertyEntry {
  key: string;
  value: string;
}

interface PropertiesResponse {
  content: string;
  entries: PropertyEntry[];
}

interface Row {
  id: number;
  key: string;
  value: string;
}

let nextRowId = 1;

/** Picks the control by the shape of the value, so unknown keys still get a sane input. */
function RowInput({ row, onChange }: { row: Row; onChange: (value: string) => void }) {
  if (row.value === 'true' || row.value === 'false') {
    return <Switch checked={row.value === 'true'} onChange={(event) => onChange(String(event.currentTarget.checked))} />;
  }

  if (row.value.trim() !== '' && /^-?\d+$/.test(row.value.trim())) {
    return (
      <NumberInput
        style={{ flex: 1 }}
        value={row.value === '' ? '' : Number(row.value)}
        allowDecimal={false}
        onChange={(value) => onChange(String(value ?? ''))}
      />
    );
  }

  return (
    <TextInput
      style={{ flex: 1 }}
      value={row.value}
      onChange={(event) => onChange(event.currentTarget.value)}
    />
  );
}

function toRows(entries: PropertyEntry[]): Row[] {
  return entries.map((entry) => ({ id: nextRowId++, ...entry }));
}

const endpoint = (serverUuid: string) =>
  `/api/client/servers/${serverUuid}/dev.smto.minecraft/properties`;

export default function PropertiesPage() {
  const serverUuid = useServerStore((state) => state.server.uuid);
  const serverState = useServerStore((state) => state.state);
  const { addToast } = useToast();

  const [rows, setRows] = useState<Row[]>([]);
  const [original, setOriginal] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [filter, setFilter] = useState('');
  const [mode, setMode] = useState<'table' | 'raw'>('table');
  const [raw, setRaw] = useState('');

  useEffect(() => {
    setLoading(true);
    axiosInstance
      .get<PropertiesResponse>(endpoint(serverUuid))
      .then(({ data }) => {
        setRows(toRows(data.entries));
        setOriginal(Object.fromEntries(data.entries.map((entry) => [entry.key, entry.value])));
        setRaw(data.content);
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setLoading(false));
  }, [serverUuid]);

  const saveRaw = () => {
    setSaving(true);
    axiosInstance
      .put<PropertiesResponse>(`${endpoint(serverUuid)}/raw`, { content: raw })
      .then(({ data }) => {
        setRows(toRows(data.entries));
        setOriginal(Object.fromEntries(data.entries.map((entry) => [entry.key, entry.value])));
        setRaw(data.content);
        addToast('server.properties saved.', 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setSaving(false));
  };

  const updateRow = (id: number, patch: Partial<PropertyEntry>) => {
    setRows((current) => current.map((row) => (row.id === id ? { ...row, ...patch } : row)));
  };

  const save = () => {
    const edits: { key: string; value: string | null }[] = [];
    const currentKeys = new Set<string>();

    for (const row of rows) {
      if (!row.key.trim()) continue;
      currentKeys.add(row.key);

      if (original[row.key] !== row.value) {
        edits.push({ key: row.key, value: row.value });
      }
    }

    for (const key of Object.keys(original)) {
      if (!currentKeys.has(key)) {
        edits.push({ key, value: null });
      }
    }

    setSaving(true);
    axiosInstance
      .put<PropertiesResponse>(endpoint(serverUuid), { edits })
      .then(({ data }) => {
        setRows(toRows(data.entries));
        setOriginal(Object.fromEntries(data.entries.map((entry) => [entry.key, entry.value])));
        addToast('server.properties saved.', 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setSaving(false));
  };

  const needle = filter.trim().toLowerCase();
  const visible = rows.filter(
    (row) => !needle || row.key.toLowerCase().includes(needle) || row.value.toLowerCase().includes(needle),
  );

  return (
    <ServerContentContainer
      title='server.properties'
      contentRight={
        <Button
          onClick={mode === 'raw' ? saveRaw : save}
          loading={saving}
          disabled={loading}
          leftSection={<FontAwesomeIcon icon={faFloppyDisk} />}
        >
          Save
        </Button>
      }
    >
      {serverState === 'running' && (
        <Alert color='yellow' mb='md' title='The server is running'>
          Minecraft writes server.properties when it stops, so edits made now can be overwritten. Stop the server first, or
          restart it after saving.
        </Alert>
      )}

      <TitleCard
        title='Properties'
        rightSection={
          <SegmentedControl
            size='xs'
            value={mode}
            onChange={(value) => setMode(value as 'table' | 'raw')}
            data={[
              { value: 'table', label: 'Form' },
              { value: 'raw', label: 'Raw file' },
            ]}
          />
        }
      >
        {loading ? (
          <Loader />
        ) : mode === 'raw' ? (
          <TextArea
            autosize
            minRows={20}
            maxRows={40}
            styles={{ input: { fontFamily: 'monospace' } }}
            value={raw}
            onChange={(event) => setRaw(event.currentTarget.value)}
          />
        ) : (
          <Table>
            <Table.Tbody>
              {visible.map((row) => (
                <Table.Tr key={row.id}>
                  <Table.Td w='40%'>
                    <TextInput
                      value={row.key}
                      onChange={(event) => updateRow(row.id, { key: event.currentTarget.value })}
                    />
                  </Table.Td>
                  <Table.Td>
                    <Group>
                      <RowInput row={row} onChange={(value) => updateRow(row.id, { value })} />
                      <ActionIcon color='red' variant='subtle' onClick={() => setRows((c) => c.filter((r) => r.id !== row.id))}>
                        <FontAwesomeIcon icon={faTrashCan} />
                      </ActionIcon>
                    </Group>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}

        {mode === 'table' && (
          <>
            <Group mt='md' justify='space-between'>
              <TextInput
                w={280}
                placeholder='Filter keys'
                value={filter}
                onChange={(event) => setFilter(event.currentTarget.value)}
              />
              <Button
                variant='default'
                onClick={() => setRows((current) => [...current, { id: nextRowId++, key: '', value: '' }])}
                leftSection={<FontAwesomeIcon icon={faPlus} />}
              >
                Add property
              </Button>
            </Group>
            {visible.length === 0 && <p>No key matches the filter.</p>}
          </>
        )}
      </TitleCard>
    </ServerContentContainer>
  );
}
