import {
  faBoxOpen,
  faClockRotateLeft,
  faCodeBranch,
  faDownload,
  faMagnifyingGlass,
  faTrashCan,
} from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { ActionIcon, Alert, Badge, Image, Loader, Modal, ScrollArea, Select, Switch, Table, Tabs } from '@mantine/core';
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
type Provider = 'modrinth' | 'curseforge';

interface InstalledMod {
  provider: string;
  project_id: string;
  project_name: string;
  version_id: string;
  version_name: string;
  filename: string;
  target: Target;
  pinned: boolean;
  previous_version: string | null;
  previous_version_id: string | null;
}

interface Detection {
  loader: Loader | null;
  mc_version: string | null;
}

interface ModsResponse {
  detection: Detection;
  auto_update: boolean;
  auto_update_interval_hours: number;
  auto_update_last_run: string | null;
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

interface AdoptResponse {
  adopted: {
    project_name: string;
    provider: string;
    version_name: string;
    filename: string;
    target: Target;
  }[];
  already_tracked: number;
  unmatched: string[];
  mods: ModsResponse;
}

interface VersionOption {
  id: string;
  version_number: string;
  filename: string | null;
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

const targetLabel = (target: Target) =>
  target === 'server' ? '/mods (server and players)' : 'client only (AutoModpack)';

export default function ModsPage() {
  const serverUuid = useServerStore((state) => state.server.uuid);
  const { addToast } = useToast();

  const [mods, setMods] = useState<InstalledMod[]>([]);
  const [autoUpdate, setAutoUpdate] = useState(false);
  const [intervalHours, setIntervalHours] = useState(6);
  const [lastRun, setLastRun] = useState<string | null>(null);
  const [updates, setUpdates] = useState<UpdateAvailable[] | null>(null);
  const [loader, setLoader] = useState<Loader | null>(null);
  const [mcVersion, setMcVersion] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);

  const [provider, setProvider] = useState<Provider>('modrinth');
  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [searchTarget, setSearchTarget] = useState<Record<string, Target>>({});

  const [picker, setPicker] = useState<{
    projectId: string;
    title: string;
    provider: Provider;
    target: Target;
  } | null>(null);
  const [pickerVersions, setPickerVersions] = useState<VersionOption[] | null>(null);
  const [pickedVersion, setPickedVersion] = useState<string | null>(null);
  const [modFilter, setModFilter] = useState('');
  const [adopting, setAdopting] = useState(false);
  const [adoptReport, setAdoptReport] = useState<AdoptResponse | null>(null);
  const [targetFilter, setTargetFilter] = useState<'all' | Target>('all');

  const apply = (data: ModsResponse) => {
    setMods(data.mods);
    setAutoUpdate(data.auto_update);
    setIntervalHours(data.auto_update_interval_hours);
    setLastRun(data.auto_update_last_run);
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

    // Checked on open, so updates show up without pressing a button.
    axiosInstance
      .get<UpdateAvailable[]>(`${endpoint(serverUuid)}/updates`)
      .then(({ data }) => setUpdates(data))
      .catch(() => setUpdates(null));
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
      .get<SearchHit[]>(`${endpoint(serverUuid)}/search`, { params: { query, provider } })
      .then(({ data }) => {
        setHits(data);
        setSearchTarget(Object.fromEntries(data.map((hit) => [hit.project_id, hit.suggested_target])));
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const openPicker = (projectId: string, title: string, provider: Provider, target: Target) => {
    setPicker({ projectId, title, provider, target });
    setPickerVersions(null);
    setPickedVersion(null);
    setBusy(true);
    axiosInstance
      .get<VersionOption[]>(`${endpoint(serverUuid)}/versions/${projectId}`, { params: { provider } })
      .then(({ data }) => setPickerVersions(data))
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const installVersion = () => {
    if (!picker || !pickedVersion) return;

    setBusy(true);
    axiosInstance
      .post<ModsResponse>(`${endpoint(serverUuid)}/install`, {
        project_id: picker.projectId,
        version_id: pickedVersion,
        target: picker.target,
        provider: picker.provider,
      })
      .then(({ data }) => {
        apply(data);
        setPicker(null);
        addToast(`${picker.title} installed to ${targetLabel(picker.target)}.`, 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const revert = (mod: InstalledMod) => {
    setBusy(true);
    axiosInstance
      .post<ModsResponse>(`${endpoint(serverUuid)}/revert`, { project_id: mod.project_id, target: mod.target })
      .then(({ data }) => {
        apply(data);
        addToast(`${mod.project_name} rolled back.`, 'success');
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
        provider,
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

  const setAutoUpdateFlag = (enabled: boolean, hours = intervalHours) => {
    setBusy(true);
    axiosInstance
      .post<ModsResponse>(`${endpoint(serverUuid)}/auto-update`, { enabled, interval_hours: hours })
      .then(({ data }) => {
        apply(data);
        addToast(enabled ? 'Auto update enabled.' : 'Auto update disabled.', 'success');
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setBusy(false));
  };

  const adopt = () => {
    setAdopting(true);
    axiosInstance
      .post<AdoptResponse>(`${endpoint(serverUuid)}/adopt`)
      .then(({ data }) => {
        apply(data.mods);
        setAdoptReport(data);
      })
      .catch((msg) => addToast(httpErrorToHuman(msg), 'error'))
      .finally(() => setAdopting(false));
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

  const needle = modFilter.trim().toLowerCase();
  const visibleMods = mods.filter((mod) => {
    if (targetFilter !== 'all' && mod.target !== targetFilter) return false;
    if (!needle) return true;

    return [mod.project_name, mod.project_id, mod.filename, mod.version_name]
      .join(' ')
      .toLowerCase()
      .includes(needle);
  });

  return (
    <ServerContentContainer title='Mods'>
      <Tabs defaultValue='installed'>
        <Tabs.List mb='md'>
          <Tabs.Tab value='installed'>
            Installed mods{updates !== null && updates.length > 0 ? ` (${updates.length} updates)` : ''}
          </Tabs.Tab>
          <Tabs.Tab value='find'>Find mods</Tabs.Tab>
          <Tabs.Tab value='loader'>Loader and version</Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel value='loader'>
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
            <Select
              label='Every'
              w={140}
              data={[
                { value: '1', label: 'hour' },
                { value: '6', label: '6 hours' },
                { value: '12', label: '12 hours' },
                { value: '24', label: 'day' },
                { value: '72', label: '3 days' },
                { value: '168', label: 'week' },
              ]}
              value={String(intervalHours)}
              onChange={(value) => value && setAutoUpdateFlag(autoUpdate, Number(value))}
              disabled={!autoUpdate}
            />
            {lastRun && (
              <small style={{ marginTop: 26 }}>last run {new Date(lastRun).toLocaleString()}</small>
            )}
          </Group>
        </TitleCard>
        </Tabs.Panel>

        <Tabs.Panel value='installed'>
        <TitleCard
          title='Installed mods'
          leftSection={updates !== null && updates.length > 0 ? <Badge color='blue'>{updates.length} updates</Badge> : undefined}
        >
          <Group mb='md'>
            <TextInput
              style={{ flex: 1 }}
              placeholder='Filter by name, id or file'
              value={modFilter}
              onChange={(event) => setModFilter(event.currentTarget.value)}
            />
            <Select
              w={180}
              data={[
                { value: 'all', label: 'Server and client' },
                { value: 'server', label: 'In /mods' },
                { value: 'client', label: 'Client only' },
              ]}
              value={targetFilter}
              onChange={(value) => value && setTargetFilter(value as 'all' | Target)}
            />
            <small>
              {visibleMods.length} of {mods.length}
            </small>
          </Group>

          {loading ? (
            <Loader />
          ) : mods.length === 0 ? (
            <p>No mods are managed by this extension yet.</p>
          ) : visibleMods.length === 0 ? (
            <p>No mod matches the filter.</p>
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
                {visibleMods.map((mod) => (
                  <Table.Tr key={`${mod.project_id}-${mod.target}`}>
                    <Table.Td>{mod.project_name}</Table.Td>
                    <Table.Td>
                      <code>{mod.version_name}</code>
                      {mod.previous_version && (
                        <div>
                          <small>
                            <FontAwesomeIcon icon={faClockRotateLeft} /> {mod.previous_version}
                          </small>
                        </div>
                      )}
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
                        {mod.previous_version_id && (
                          <Button size='xs' variant='default' onClick={() => revert(mod)}>
                            {`Revert to ${mod.previous_version}`}
                          </Button>
                        )}
                        <ActionIcon
                          variant='subtle'
                          aria-label='Choose version'
                          onClick={() =>
                            openPicker(mod.project_id, mod.project_name, mod.provider as Provider, mod.target)
                          }
                        >
                          <FontAwesomeIcon icon={faCodeBranch} />
                        </ActionIcon>
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
            <Button
              variant='default'
              onClick={adopt}
              loading={adopting}
              leftSection={<FontAwesomeIcon icon={faBoxOpen} />}
            >
              Adopt existing mods
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
        </Tabs.Panel>

        <Tabs.Panel value='find'>
        <TitleCard title='Find mods'>
          <Group mb='md'>
            <Select
              w={160}
              data={[
                { value: 'modrinth', label: 'Modrinth' },
                { value: 'curseforge', label: 'CurseForge' },
              ]}
              value={provider}
              onChange={(value) => value && setProvider(value as Provider)}
            />
            <TextInput
              style={{ flex: 1 }}
              placeholder='Search for mods'
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
            Everything in <code>/mods</code> is loaded by the server and synced to players when they join, so
            server-side mods belong there. Mods the server itself does not need go to the AutoModpack directory and are
            still delivered to players. A mod that must never reach players has to be excluded in the AutoModpack config,
            which is rarely needed.
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
                  variant='default'
                  onClick={() =>
                    openPicker(
                      hit.project_id,
                      hit.title,
                      provider,
                      searchTarget[hit.project_id] ?? hit.suggested_target,
                    )
                  }
                >
                  Version
                </Button>
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
        </Tabs.Panel>
      </Tabs>

      <Modal
        opened={!!adoptReport}
        onClose={() => setAdoptReport(null)}
        title='Adopt existing mods'
        size='lg'
      >
        {adoptReport && (
          <Stack>
            <p>
              {adoptReport.adopted.length === 0
                ? 'No new mods were recognised.'
                : `${adoptReport.adopted.length} mod(s) are now managed by this extension.`}{' '}
              {adoptReport.already_tracked > 0 &&
                `${adoptReport.already_tracked} jar(s) were already tracked.`}
            </p>

            {adoptReport.adopted.length > 0 && (
              <ScrollArea h={240}>
                <Table>
                  <Table.Tbody>
                    {adoptReport.adopted.map((entry) => (
                      <Table.Tr key={`${entry.filename}-${entry.target}`}>
                        <Table.Td>{entry.project_name}</Table.Td>
                        <Table.Td>
                          <code>{entry.version_name}</code>
                        </Table.Td>
                        <Table.Td>
                          <Badge variant='light'>{targetLabel(entry.target)}</Badge>
                        </Table.Td>
                      </Table.Tr>
                    ))}
                  </Table.Tbody>
                </Table>
              </ScrollArea>
            )}

            {adoptReport.unmatched.length > 0 && (
              <Alert color='yellow' title={`${adoptReport.unmatched.length} jar(s) not recognised`}>
                These stay on disk and unmanaged. Usually a jar from a source that is not Modrinth
                or CurseForge, or a file that was modified after download.
                <ul style={{ marginBottom: 0 }}>
                  {adoptReport.unmatched.map((name) => (
                    <li key={name}>
                      <code>{name}</code>
                    </li>
                  ))}
                </ul>
              </Alert>
            )}

            <Group justify='flex-end'>
              <Button onClick={() => setAdoptReport(null)}>Close</Button>
            </Group>
          </Stack>
        )}
      </Modal>

      <Modal opened={!!picker} onClose={() => setPicker(null)} title={`Version of ${picker?.title ?? ''}`}>
        {pickerVersions === null ? (
          <Loader />
        ) : pickerVersions.length === 0 ? (
          <p>This mod has no version for the detected loader and Minecraft version.</p>
        ) : (
          <Stack>
            <Select
              label='Version'
              placeholder='Pick a version'
              searchable
              data={pickerVersions.map((version) => ({
                value: version.id,
                label: version.filename ?? version.version_number,
              }))}
              value={pickedVersion}
              onChange={setPickedVersion}
            />
            <small>
              Installing a specific version does not pin it. Use the pin switch afterwards if it should stay.
            </small>
            <Group justify='flex-end'>
              <Button variant='default' onClick={() => setPicker(null)}>
                Cancel
              </Button>
              <Button onClick={installVersion} loading={busy} disabled={!pickedVersion}>
                Install
              </Button>
            </Group>
          </Stack>
        )}
      </Modal>
    </ServerContentContainer>
  );
}
