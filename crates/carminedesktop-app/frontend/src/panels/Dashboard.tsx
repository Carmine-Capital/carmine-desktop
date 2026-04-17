import { Show, createMemo, onMount, type JSX } from 'solid-js';

import { ActivityFeed } from '../components/ActivityFeed';
import { AuthBanner } from '../components/AuthBanner';
import { CacheSection } from '../components/CacheSection';
import { DriveList } from '../components/DriveList';
import { ErrorFeed } from '../components/ErrorFeed';
import { QuickStats, type QuickStat } from '../components/QuickStats';
import { Skeleton } from '../components/Skeleton';
import { Tab, TabList, TabPanel, Tabs } from '../components/Tabs';
import { bootstrapCache, cache } from '../store/cache';
import { drives } from '../store/drives';
import { errors } from '../store/errors';
import { pins } from '../store/pins';

/** Dashboard panel.  Every subscription is nested inside the children —
 *  mounting this component just kicks off a one-shot cache refresh, then
 *  the live stream (drive:status / drive:online / drive:upload-progress /
 *  activity:append / error:append / auth:state / pin:health) keeps each
 *  node surgically in sync. */
export const Dashboard = (): JSX.Element => {
  // Refresh cache stats on every mount — but rely on the store's `loaded`
  // flag to decide whether to show the skeleton, so tab round-trips don't
  // flash a placeholder over already-populated content.
  onMount(() => {
    void bootstrapCache().catch(() => {
      /* silent — disk usage is advisory */
    });
  });

  const errorHeading = createMemo(() =>
    errors.entries.length > 0 ? `Erreurs (${errors.entries.length})` : 'Erreurs',
  );

  // Derive quick-stat tiles from the live stores.  Reads stay reactive: each
  // card re-renders independently as its source store mutates.
  const stats = createMemo<QuickStat[]>(() => {
    const driveCount = drives.list.length;
    const pinCount = pins.list.length;
    const errorCount = errors.entries.length;
    return [
      {
        label: 'Lecteurs',
        value: String(driveCount),
        hint: driveCount > 1 ? 'lecteurs connectés' : 'lecteur connecté',
      },
      {
        label: 'Hors-ligne',
        value: String(pinCount),
        hint: pinCount > 1 ? 'dossiers épinglés' : 'dossier épinglé',
      },
      {
        label: 'Erreurs',
        value: String(errorCount),
        hint: errorCount === 0 ? 'Aucune erreur' : 'erreurs récentes',
      },
    ];
  });

  return (
    <>
      <AuthBanner />
      <Tabs defaultTab="overview">
        <TabList>
          <Tab id="overview">Vue</Tab>
          <Tab id="activity">Activité</Tab>
          <Tab id="storage">Stockage</Tab>
        </TabList>

        <TabPanel id="overview">
          <p class="section-heading">Vos Lecteurs</p>
          <Show
            when={drives.loaded}
            fallback={<Skeleton variant="card" label="Chargement…" />}
          >
            <DriveList />
          </Show>
          <QuickStats items={stats()} />
        </TabPanel>

        <TabPanel id="activity">
          <p class="section-heading">Activité récente</p>
          <ActivityFeed />
          <p class="section-heading">{errorHeading()}</p>
          <ErrorFeed />
        </TabPanel>

        <TabPanel id="storage">
          <p class="section-heading">Cache & Hors-ligne</p>
          <Show
            when={cache.loaded}
            fallback={<Skeleton variant="row" label="Chargement…" />}
          >
            <CacheSection />
          </Show>
        </TabPanel>
      </Tabs>
    </>
  );
};
