import { faDownload, faMagnifyingGlass, faTrashCan } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { ActionIcon, Alert, Badge, Image, Loader, Select, Switch, Table } from '@mantine/core';
import { useEffect, useState } from 'react';
import { axiosInstance, httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import TitleCard from '@/elements/data-display/TitleCard.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import Group from '@/elements/layout/Group.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import ServerContentContainer from '@/elements/containers/ServerContentContainer.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import { useServerStore } from '@/stores/server.ts';

type Loader = 'fabric' | 'neoforge' | 'forge';
type Target = 'server' | 'client';

interface InstalledMod {
  provider: string;
  project_id: string;
  project_name: string;
  version_id: string;
  version_name: string;
  filename: string;
  target: Target;
  pinned: boolean;
}

interface Detection {
  loader: Loader | null;
  mc_version: string | null;
}

interface ModsResponse {
  detection: Detection;
  auto_update: boolean;
  mods: InstalledMod[];
}

interface UpdateResponse {
  updated: { project_id: string }[];
  mods: ModsResponse;
}

interface UpdateAvailable {
  project_id: string;
  target: Target;
  current: string;
  latest: string;
}

interface SearchHit {
  project_id: string;
  slug: string;
  title: string;
  description: string;
  icon_url: string | null;
  downloads: number;
  client_side: string;
  server_side: string;
  suggested_target: Target;
}

const endpoint = (serverUuid: string) => `/api/client/servers/${serverUuid}/dev.smto.minecraft/mods`;

const targetLabel = (target: Target) => (target === 'server' ? 'server (/mods)' : 'client (AutoModpack)');

export default function ModsPage() {
  const serverUuid = useServerStore((state) => state.server.uuid);
  const { addToast } = useToast();

  const [mods, setMods] = useState<InstalledMod[]>([]);
  const [autoUpdate, setAutoUpdate] = useState(false);
  const [updates, setUpdates] = useState<UpdateAvailable[] | null>(null);
  const [loader, setLoader] = useState<Loader | null>(null);
  const [mcVersion, setMcVersion] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);

  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [searchTarget, setSearchTarget] = useState<Record<string, Target>>({});

  const apply = (data: ModsResponse) => {
    setMods(data.mods);
    setAutoUpdate(data.auto_update);
    setLoader(data.detection.loader);
    setMcVersion(data.detection.mc_version ?? '');
  };

  const updateFor = (mod: InstalledMod) =>
    updates?.find((entry) => entry.project_id === mod.project_id && entry.target === mod.target);

  useEffect(() => {
    setLoading(true);
    axiosInstance
      .get<ModsResponse>(endpoint(serverUuid))
      .then(({ data }) => apply(data))
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setLoading(false));
  }, [serverUuid]);

  const saveDetection = () => {
    setBusy(true);
    axiosInstance
      .post<ModsResponse>(`${endpoint(serverUuid)}/detection`, { loader, mc_version: mcVersion })
      .then(({ data }) => {
        apply(data);
        addToast('Loader and version saved.', 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const search = () => {
    setBusy(true);
    axiosInstance
      .get<SearchHit[]>(`${endpoint(serverUuid)}/search`, { params: { query } })
      .then(({ data }) => {
        setHits(data);
        setSearchTarget(Object.fromEntries(data.map((hit) => [hit.project_id, hit.suggested_target])));
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const install = (hit: SearchHit) => {
    const target = searchTarget[hit.project_id] ?? hit.suggested_target;
    setBusy(true);
    axiosInstance
      .post<ModsResponse>(`${endpoint(serverUuid)}/install`, {
        project_id: hit.project_id,
        target,
      })
      .then(({ data }) => {
        apply(data);
        addToast(`${hit.title} installed to ${targetLabel(target)}.`, 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const uninstall = (mod: InstalledMod) => {
    setBusy(true);
    axiosInstance
      .post<ModsResponse>(`${endpoint(serverUuid)}/uninstall`, {
        project_id: mod.project_id,
        target: mod.target,
      })
      .then(({ data }) => {
        apply(data);
        addToast(`${mod.project_name} removed.`, 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const setPinned = (mod: InstalledMod, pinned: boolean) => {
    setBusy(true);
    axiosInstance
      .post<ModsResponse>(`${endpoint(serverUuid)}/pin`, { project_id: mod.project_id, target: mod.target, pinned })
      .then(({ data }) => apply(data))
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const setAutoUpdateFlag = (enabled: boolean) => {
    setBusy(true);
    axiosInstance
      .post<ModsResponse>(`${endpoint(serverUuid)}/auto-update`, { enabled })
      .then(({ data }) => {
        apply(data);
        addToast(enabled ? 'Auto update enabled.' : 'Auto update disabled.', 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const checkUpdates = () => {
    setBusy(true);
    axiosInstance
      .get<UpdateAvailable[]>(`${endpoint(serverUuid)}/updates`)
      .then(({ data }) => setUpdates(data))
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const updateMod = (mod?: InstalledMod) => {
    setBusy(true);
    axiosInstance
      .post<UpdateResponse>(`${endpoint(serverUuid)}/update`, mod ? { project_id: mod.project_id, target: mod.target } : {})
      .then(({ data }) => {
        apply(data.mods);
        setUpdates(null);
        addToast(
          data.updated.length ? `${data.updated.length} mod(s) updated.` : 'Everything is current.',
          'success',
        );
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  return (
    <ServerContentContainer title='Mods'>
      <Stack>
        <TitleCard title='Loader and version'>
          <Alert color='blue' variant='light' mb='md'>
            Detected from the server files. Correct it here if detection got it wrong, for example for custom
            installations.
          </Alert>
          <Group>
            <Select
              label='Loader'
              w={180}
              data={[
                { value: 'fabric', label: 'Fabric' },
                { value: 'neoforge', label: 'NeoForge' },
                { value: 'forge', label: 'Forge' },
              ]}
              value={loader}
              onChange={(value) => setLoader(value as Loader | null)}
              clearable
            />
            <TextInput
              label='Minecraft version'
              w={180}
              placeholder='1.21.1'
              value={mcVersion}
              onChange={(event) => setMcVersion(event.currentTarget.value)}
            />
            <Button mt={22} variant='default' onClick={saveDetection} loading={busy}>
              Save
            </Button>
            <Switch
              mt={22}
              label='Auto update while offline'
              checked={autoUpdate}
              onChange={(event) => setAutoUpdateFlag(event.currentTarget.checked)}
            />
          </Group>
        </TitleCard>

        <TitleCard title='Installed mods'>
          {loading ? (
            <Loader />
          ) : mods.length === 0 ? (
            <p>No mods are managed by this extension yet.</p>
          ) : (
            <Table>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Mod</Table.Th>
                  <Table.Th>Version</Table.Th>
                  <Table.Th>Installed in</Table.Th>
                  <Table.Th>Pinned</Table.Th>
                  <Table.Th />
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {mods.map((mod) => (
                  <Table.Tr key={`${mod.project_id}-${mod.target}`}>
                    <Table.Td>{mod.project_name}</Table.Td>
                    <Table.Td>
                      <code>{mod.version_name}</code>
                    </Table.Td>
                    <Table.Td>
                      <Badge variant='light'>{targetLabel(mod.target)}</Badge>
                    </Table.Td>
                    <Table.Td>
                      <Switch checked={mod.pinned} onChange={(event) => setPinned(mod, event.currentTarget.checked)} />
                    </Table.Td>
                    <Table.Td w={120}>
                      <Group gap='xs' wrap='nowrap'>
                        {updateFor(mod) && (
                          <Button size='xs' variant='light' onClick={() => updateMod(mod)}>
                            {updateFor(mod)?.latest}
                          </Button>
                        )}
                        <ActionIcon color='red' variant='subtle' onClick={() => uninstall(mod)}>
                          <FontAwesomeIcon icon={faTrashCan} />
                        </ActionIcon>
                      </Group>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}

          <Group mt='md'>
            <Button variant='default' onClick={checkUpdates} loading={busy}>
              Check for updates
            </Button>
            <Button variant='default' onClick={() => updateMod()} loading={busy}>
              Update all
            </Button>
            {updates && (
              <span>
                {updates.length === 0
                  ? 'Everything is current.'
                  : `${updates.length} update(s) available.`}
              </span>
            )}
          </Group>
        </TitleCard>

        <TitleCard title='Find mods'>
          <Group mb='md'>
            <TextInput
              style={{ flex: 1 }}
              placeholder='Search Modrinth'
              value={query}
              onChange={(event) => setQuery(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') search();
              }}
            />
            <Button onClick={search} loading={busy} leftSection={<FontAwesomeIcon icon={faMagnifyingGlass} />}>
              Search
            </Button>
          </Group>

          <Alert color='blue' variant='light' mb='md'>
            Mods the server needs go to <code>/mods</code>. Client-only mods go to the AutoModpack directory, so players
            get them from the server automatically.
          </Alert>

          {hits?.length === 0 && <p>Nothing found.</p>}

          <Stack>
            {hits?.map((hit) => (
              <Group key={hit.project_id} align='flex-start' wrap='nowrap'>
                {hit.icon_url && <Image src={hit.icon_url} w={48} h={48} radius='sm' />}
                <div style={{ flex: 1 }}>
                  <strong>{hit.title}</strong>
                  <div>{hit.description}</div>
                  <small>
                    client: {hit.client_side} / server: {hit.server_side}
                  </small>
                </div>
                <Select
                  w={200}
                  data={[
                    { value: 'server', label: targetLabel('server') },
                    { value: 'client', label: targetLabel('client') },
                  ]}
                  value={searchTarget[hit.project_id] ?? hit.suggested_target}
                  onChange={(value) =>
                    value && setSearchTarget((current) => ({ ...current, [hit.project_id]: value as Target }))
                  }
                />
                <Button
                  onClick={() => install(hit)}
                  loading={busy}
                  leftSection={<FontAwesomeIcon icon={faDownload} />}
                >
                  Install
                </Button>
              </Group>
            ))}
          </Stack>
        </TitleCard>
      </Stack>
    </ServerContentContainer>
  );
}
